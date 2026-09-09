//! Экран: снимки и accessibility-дерево (ТЗ §6, §30).

use base64::Engine as _;
use image::ImageEncoder;
use xcap::{Monitor, Window};
use yuki_system::{
    AccessibilityNode, CaptureOptions, Rect, ScreenAdapter, ScreenCapture, SystemError,
    SystemResult,
};

pub struct DesktopScreenAdapter {
    accessibility: Box<dyn yuki_accessibility::AccessibilityProvider>,
}

impl DesktopScreenAdapter {
    pub fn new() -> Self {
        Self {
            accessibility: yuki_accessibility::provider(),
        }
    }
}

impl Default for DesktopScreenAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn platform_err(e: impl std::fmt::Display) -> SystemError {
    SystemError::Platform(e.to_string())
}

/// Кодирует кадр в PNG/base64 — в таком виде его принимает vision-модель (ТЗ §6).
fn encode(image: xcap::image::RgbaImage, display_index: usize) -> SystemResult<ScreenCapture> {
    let (width, height) = (image.width(), image.height());
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(image.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .map_err(platform_err)?;

    Ok(ScreenCapture {
        width,
        height,
        png_base64: base64::engine::general_purpose::STANDARD.encode(&png),
        display_index,
    })
}

/// Пересечение запрошенной области с кадром.
///
/// Отдельной функцией ради теста: область за краем экрана — обычное дело
/// (модель считает координаты по прошлому снимку, окно успело сдвинуться), и
/// паниковать на ней нельзя. `None` означает, что пересечения нет вовсе.
pub fn clamp_region(frame: (u32, u32), region: Rect) -> Option<(u32, u32, u32, u32)> {
    let (frame_width, frame_height) = frame;

    let left = region.x.max(0) as u32;
    let top = region.y.max(0) as u32;
    if left >= frame_width || top >= frame_height {
        return None;
    }

    // Отрицательные x/y срезают часть ширины: запрошенная область начиналась
    // левее кадра, и эта часть в него не попадёт.
    let width = (region.width + region.x.min(0)).max(0) as u32;
    let height = (region.height + region.y.min(0)).max(0) as u32;

    let width = width.min(frame_width - left);
    let height = height.min(frame_height - top);

    if width == 0 || height == 0 {
        return None;
    }

    Some((left, top, width, height))
}

/// Во сколько раз уменьшать кадр, чтобы уложиться в ограничение ширины.
///
/// Уменьшение — не украшение: снимок 2560×1440 в PNG весит под мегабайт, и на
/// каждом шаге агента это мегабайт в запросе к модели. Увеличивать при этом
/// нельзя никогда: пикселей от этого не прибавится, а вес вырастет.
pub fn target_size(width: u32, height: u32, max_width: Option<u32>) -> Option<(u32, u32)> {
    let limit = max_width?;
    if limit == 0 || width <= limit {
        return None;
    }

    let scaled_height = ((height as u64 * limit as u64) / width as u64).max(1) as u32;
    Some((limit, scaled_height))
}

/// Применяет область и уменьшение.
fn prepare(
    mut image: xcap::image::RgbaImage,
    options: &CaptureOptions,
) -> SystemResult<xcap::image::RgbaImage> {
    if let Some(region) = options.region {
        let (x, y, width, height) = clamp_region((image.width(), image.height()), region)
            .ok_or_else(|| {
                SystemError::NotFound(format!(
                    "область {}×{} в точке ({}, {}) не пересекается с экраном",
                    region.width, region.height, region.x, region.y
                ))
            })?;
        image = image::imageops::crop(&mut image, x, y, width, height).to_image();
    }

    if let Some((width, height)) = target_size(image.width(), image.height(), options.max_width) {
        // Треугольный фильтр: Lanczos на снимке экрана даёт звон на тексте,
        // а ближайший сосед — рвань. Для чтения интерфейса это заметно.
        image = image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
    }

    Ok(image)
}

impl ScreenAdapter for DesktopScreenAdapter {
    fn capture_with(&self, options: &CaptureOptions) -> SystemResult<ScreenCapture> {
        let monitors = Monitor::all().map_err(platform_err)?;
        let index = options.display_index.unwrap_or(0);
        let monitor = monitors
            .get(index)
            .ok_or_else(|| SystemError::NotFound(format!("монитор #{index}")))?;
        let image = monitor.capture_image().map_err(platform_err)?;
        encode(prepare(image, options)?, index)
    }

    fn capture_window(&self, window_id: u64) -> SystemResult<ScreenCapture> {
        let windows = Window::all().map_err(platform_err)?;
        let window = windows
            .into_iter()
            .find(|w| w.id() as u64 == window_id)
            .ok_or_else(|| SystemError::NotFound(format!("окно #{window_id}")))?;
        let image = window.capture_image().map_err(platform_err)?;
        encode(image, 0)
    }

    fn accessibility_tree(&self, window_id: Option<u64>) -> SystemResult<AccessibilityNode> {
        self.accessibility.tree(window_id)
    }

    fn display_count(&self) -> SystemResult<usize> {
        Ok(Monitor::all().map_err(platform_err)?.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn keeps_a_region_that_fits_entirely_on_screen() {
        assert_eq!(
            clamp_region((1920, 1080), rect(100, 200, 400, 300)),
            Some((100, 200, 400, 300))
        );
    }

    #[test]
    fn trims_a_region_that_hangs_off_the_right_edge() {
        // Модель считает координаты по прошлому снимку — окно могло сдвинуться.
        assert_eq!(
            clamp_region((1920, 1080), rect(1800, 1000, 400, 300)),
            Some((1800, 1000, 120, 80))
        );
    }

    #[test]
    fn a_region_starting_left_of_the_screen_loses_the_part_that_is_outside() {
        assert_eq!(
            clamp_region((1920, 1080), rect(-50, -20, 200, 100)),
            Some((0, 0, 150, 80))
        );
    }

    #[test]
    fn a_region_entirely_outside_the_screen_is_nothing_not_an_empty_image() {
        assert_eq!(clamp_region((1920, 1080), rect(2000, 100, 200, 200)), None);
        assert_eq!(clamp_region((1920, 1080), rect(-500, 0, 200, 200)), None);
        assert_eq!(clamp_region((1920, 1080), rect(0, 0, 0, 100)), None);
    }

    #[test]
    fn scales_down_keeping_the_proportions() {
        assert_eq!(target_size(2560, 1440, Some(1280)), Some((1280, 720)));
        assert_eq!(target_size(1920, 1080, Some(1280)), Some((1280, 720)));
    }

    #[test]
    fn never_scales_up() {
        // Растянутый снимок весит больше, а деталей в нём не прибавляется.
        assert_eq!(target_size(800, 600, Some(1280)), None);
        assert_eq!(target_size(1280, 720, Some(1280)), None);
    }

    #[test]
    fn without_a_limit_the_frame_is_left_alone() {
        assert_eq!(target_size(2560, 1440, None), None);
        assert_eq!(target_size(2560, 1440, Some(0)), None);
    }

    #[test]
    fn a_very_wide_frame_keeps_at_least_one_pixel_of_height() {
        // Полоса 4000×3 при пределе 100 не должна схлопнуться в нулевую высоту.
        assert_eq!(target_size(4000, 3, Some(100)), Some((100, 1)));
    }
}
