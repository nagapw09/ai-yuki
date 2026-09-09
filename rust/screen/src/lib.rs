//! Экран: снимки и accessibility-дерево (ТЗ §6, §30).

use base64::Engine as _;
use image::ImageEncoder;
use xcap::{Monitor, Window};
use yuki_system::{
    AccessibilityNode, ScreenAdapter, ScreenCapture, SystemError, SystemResult,
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

impl ScreenAdapter for DesktopScreenAdapter {
    fn capture(&self, display_index: Option<usize>) -> SystemResult<ScreenCapture> {
        let monitors = Monitor::all().map_err(platform_err)?;
        let index = display_index.unwrap_or(0);
        let monitor = monitors
            .get(index)
            .ok_or_else(|| SystemError::NotFound(format!("монитор #{index}")))?;
        let image = monitor.capture_image().map_err(platform_err)?;
        encode(image, index)
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
