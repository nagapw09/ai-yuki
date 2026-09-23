//! Personal avatar library and positioning of Yuki's own overlay window.
use crate::{
    avatar::{set_setting, setting, WINDOW_LABEL},
    state::AppState,
};
use serde::Serialize;
use std::{io::Read, path::Path, sync::Mutex, time::Instant};
use tauri::{Emitter, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    models: usize,
    animations: usize,
    unsupported: Vec<String>,
}

fn asset_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
        .take(80)
        .collect()
}
fn glb_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    if bytes.len() < 20 || &bytes[..4] != b"glTF" || &bytes[16..20] != b"JSON" {
        return Err("Файл не является VRM/VRMA (glTF binary)".into());
    }
    let size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let data = bytes
        .get(20..20usize.saturating_add(size))
        .ok_or("Повреждён заголовок модели")?;
    serde_json::from_slice(data).map_err(|_| "Повреждены метаданные модели".into())
}

#[tauri::command]
pub async fn companion_import(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<ImportReport, String> {
    tauri::async_runtime::spawn_blocking(move || import_files(&app, paths))
        .await
        .map_err(|e| e.to_string())?
}

fn import_files(app: &tauri::AppHandle, paths: Vec<String>) -> Result<ImportReport, String> {
    if paths.len() > 40 {
        return Err("Выберите не больше 40 файлов за один импорт".into());
    }
    let state = app.state::<AppState>();
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("companions");
    let animations = root.join("animations");
    std::fs::create_dir_all(&animations).map_err(|e| e.to_string())?;
    let mut report = ImportReport {
        models: 0,
        animations: 0,
        unsupported: vec![],
    };
    let mut assets: Vec<(String, Vec<u8>)> = vec![];
    let mut total = 0u64;
    for path in paths {
        let source = Path::new(&path);
        if source
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("zip"))
        {
            let file = std::fs::File::open(source).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            if zip.len() > 3000 {
                return Err("В архиве слишком много файлов".into());
            }
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
                let name = entry.name().replace('\\', "/");
                let ext = Path::new(&name)
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(
                    ext.as_str(),
                    "unitypackage" | "anim" | "fbx" | "vmd" | "blend" | "resonitepackage"
                ) {
                    report.unsupported.push(
                        Path::new(&name)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
                if !matches!(ext.as_str(), "vrm" | "vrma") {
                    continue;
                }
                if entry.size() > 160 * 1024 * 1024 {
                    return Err("Один ресурс превышает 160 МБ".into());
                }
                total += entry.size();
                if total > 512 * 1024 * 1024 {
                    return Err("Импорт превышает 512 МБ".into());
                }
                let mut bytes = Vec::new();
                (&mut entry)
                    .take(160 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 160 * 1024 * 1024 {
                    return Err("Ресурс превышает лимит".into());
                }
                assets.push((name, bytes));
            }
        } else {
            let ext = source
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !matches!(ext.as_str(), "vrm" | "vrma") {
                report.unsupported.push(
                    source
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
                continue;
            }
            let len = source.metadata().map_err(|e| e.to_string())?.len();
            total += len;
            if len > 160 * 1024 * 1024 || total > 512 * 1024 * 1024 {
                return Err("Импорт превышает лимит размера".into());
            }
            assets.push((
                source
                    .file_name()
                    .ok_or("Нет имени файла")?
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read(source).map_err(|e| e.to_string())?,
            ));
        }
    }
    // Validate everything before writing any imported model.
    for (name, data) in &assets {
        let meta = glb_json(data)?;
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let extensions = &meta["extensions"];
        if (ext == "vrm" && extensions.get("VRM").is_none() && extensions.get("VRMC_vrm").is_none())
            || (ext == "vrma" && extensions.get("VRMC_vrm_animation").is_none())
        {
            return Err(format!("{name}: отсутствует расширение VRM/VRMA"));
        }
    }
    let batch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for (index, (name, data)) in assets.into_iter().enumerate() {
        let path = Path::new(&name);
        let stem = asset_name(
            path.file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("avatar"),
        );
        if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("vrma"))
        {
            let label = match stem.as_str() {
                "VRMA_01" => "showcase",
                "VRMA_02" => "greeting",
                "VRMA_03" => "peace",
                "VRMA_04" => "shoot",
                "VRMA_05" => "spin",
                "VRMA_06" => "pose",
                "VRMA_07" => "squat",
                _ => &stem,
            };
            std::fs::write(animations.join(format!("{label}.vrma")), data)
                .map_err(|e| e.to_string())?;
            report.animations += 1;
        } else {
            let meta = glb_json(&data)?;
            let title = meta["extensions"]["VRM"]["meta"]["title"]
                .as_str()
                .or_else(|| meta["extensions"]["VRMC_vrm"]["meta"]["name"].as_str())
                .unwrap_or(&stem);
            let title: String = title.chars().take(32).collect();
            let file = root.join(format!("{batch}-{index}-{stem}.vrm"));
            std::fs::write(&file, data).map_err(|e| e.to_string())?;
            let snapshot = serde_json::json!({"persona.name": title, "persona.role":"assistant", "persona.custom":"", "persona.extra":"", "avatar.model":file.to_string_lossy(), "avatar.animations":animations.to_string_lossy(), "avatar.pose":"full", "avatar.behavior":"calm", "voice.tts.engine":"system", "voice.tts.voice":setting(&state,"voice.tts.voice").unwrap_or_default()});
            state
                .storage
                .with_conn(|c| {
                    c.execute(
                        "INSERT INTO profiles (id,name,data) VALUES (?1,?2,?3)",
                        rusqlite::params![
                            format!("avatar-{batch}-{index}"),
                            title,
                            snapshot.to_string()
                        ],
                    )
                })
                .map_err(|e| e.to_string())?;
            report.models += 1;
        }
    }
    if report.animations > 0 {
        set_setting(&state, "avatar.animations", &animations.to_string_lossy())?;
    }
    let _ = app.emit("yuki://avatar-reload", ());
    Ok(report)
}

pub struct MotionClock(Mutex<(Instant, i32)>, Mutex<Option<Jump>>);
impl Default for MotionClock {
    fn default() -> Self {
        Self(Mutex::new((Instant::now(), 1)), Mutex::new(None))
    }
}

/// Перелёт на другую поверхность (разбор Desktop Mate §16).
///
/// Скелет прыгает клипом, а само окно летит по дуге: `P(t) = lerp(start, end, t)
/// + 4·H·t·(1−t)`. Раньше смена места была телепортом — персонаж пропадал с
/// панели задач и возникал на окне.
#[derive(Clone, Copy)]
struct Jump {
    from: (i32, i32),
    to: (i32, i32),
    progress: f64,
}

/// Смещение, с которого смена места — прыжок, а не следование за окном.
/// Окно, которое тянут мышью, сдвигается по чуть-чуть, и его надо просто
/// догонять; прыжок нужен, когда поверхность сменилась целиком.
const JUMP_DISTANCE: i32 = 150;
/// Длительность прыжка и высота дуги.
const JUMP_SECONDS: f64 = 0.7;
const JUMP_HEIGHT: f64 = 140.0;

fn jump_point(jump: &Jump) -> (i32, i32) {
    let t = jump.progress.clamp(0.0, 1.0);
    let x = jump.from.0 as f64 + (jump.to.0 - jump.from.0) as f64 * t;
    let y = jump.from.1 as f64 + (jump.to.1 - jump.from.1) as f64 * t - 4.0 * JUMP_HEIGHT * t * (1.0 - t);
    (x.round() as i32, y.round() as i32)
}

#[tauri::command]
pub fn companion_motion_tick(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    clock: State<'_, MotionClock>,
    contact: f64,
    walking: bool,
    target: Option<u64>,
) -> Result<i32, String> {
    let mut motion = clock.0.lock().map_err(|e| e.to_string())?;
    let delta = motion.0.elapsed().as_secs_f64().min(0.15);
    motion.0 = Instant::now();
    let window = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or("Окно аватара закрыто")?;
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("Монитор недоступен")?;
    let area = monitor.work_area();
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let anchor = setting(&state, "avatar.anchor").unwrap_or_default();
    let mut left = area.position.x;
    let mut right = left + area.size.width as i32;
    let mut surface = area.position.y + area.size.height as i32;
    if anchor == "window" {
        if let Some(target) = target.and_then(|id| {
            state
                .adapters
                .system
                .list_windows()
                .ok()?
                .into_iter()
                .find(|w| w.id == id && !w.is_minimized)
        }) {
            let scale = if cfg!(target_os = "macos") {
                monitor.scale_factor()
            } else {
                1.0
            };
            left = (target.bounds.x as f64 * scale) as i32;
            right = left + (target.bounds.width as f64 * scale) as i32;
            surface = (target.bounds.y as f64 * scale) as i32;
        }
    }
    let min_x = left.max(area.position.x);
    let max_x =
        (right.min(area.position.x + area.size.width as i32) - size.width as i32).max(min_x);
    let x = if walking {
        let next = pos.x + (delta * 60.0 * monitor.scale_factor() * motion.1 as f64).round() as i32;
        if next <= min_x {
            motion.1 = 1;
        } else if next >= max_x {
            motion.1 = -1;
        }
        next.clamp(min_x, max_x)
    } else {
        pos.x.clamp(min_x, max_x)
    };
    if anchor != "free" || walking {
        let ratio = if contact.is_finite() {
            contact.clamp(0.05, 1.0)
        } else {
            1.0
        };
        let y = (surface - (size.height as f64 * ratio).round() as i32).max(area.position.y);
        let mut jump = clock.1.lock().map_err(|e| e.to_string())?;
        let far = (pos.x - x).abs().max((pos.y - y).abs()) > JUMP_DISTANCE;
        if jump.is_none() && far && !walking {
            *jump = Some(Jump { from: (pos.x, pos.y), to: (x, y), progress: 0.0 });
        }
        let (nx, ny) = match jump.as_mut() {
            Some(flight) => {
                // Цель могла сдвинуться за время полёта — летим туда, где она сейчас.
                flight.to = (x, y);
                flight.progress += delta / JUMP_SECONDS;
                let point = jump_point(flight);
                if flight.progress >= 1.0 {
                    *jump = None;
                }
                point
            }
            None => (x, y),
        };
        if pos.x != nx || pos.y != ny {
            window
                .set_position(tauri::PhysicalPosition::new(nx, ny))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(motion.1)
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompanionContext {
    music_playing: bool,
    idle_seconds: f64,
    media_available: bool,
}

#[tauri::command]
pub fn companion_pointer(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    interactive: bool,
) -> Result<serde_json::Value, String> {
    let win = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or("Окно аватара закрыто")?;
    let cursor = win.cursor_position().map_err(|e| e.to_string())?;
    let pos = win.outer_position().map_err(|e| e.to_string())?;
    let size = win.inner_size().map_err(|e| e.to_string())?;
    let ignore = setting(&state, "avatar.click_through").as_deref() == Some("on") || !interactive;
    win.set_ignore_cursor_events(ignore)
        .map_err(|e| e.to_string())?;
    Ok(
        serde_json::json!({"x":(cursor.x-pos.x as f64)/size.width.max(1) as f64,"y":(cursor.y-pos.y as f64)/size.height.max(1) as f64}),
    )
}

#[tauri::command]
pub async fn companion_context() -> Result<CompanionContext, String> {
    tauri::async_runtime::spawn_blocking(read_context)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn read_context() -> CompanionContext {
    use windows::{
        Media::{
            Control::{
                GlobalSystemMediaTransportControlsSessionManager as MediaManager,
                GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
            },
            MediaPlaybackType,
        },
        Win32::{
            System::SystemInformation::GetTickCount,
            UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        },
    };
    let mut result = CompanionContext::default();
    let mut input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        if GetLastInputInfo(&mut input).as_bool() {
            result.idle_seconds = GetTickCount().wrapping_sub(input.dwTime) as f64 / 1000.0;
        }
    }
    ensure_com();
    let media = (|| -> windows::core::Result<bool> {
        let manager = MediaManager::RequestAsync()?.get()?;
        let sessions = manager.GetSessions()?;
        for session in sessions {
            if session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Playing {
                continue;
            }
            let meta = session.TryGetMediaPropertiesAsync()?.get()?;
            let kind = meta.PlaybackType().and_then(|v| v.Value()).ok();
            if kind == Some(MediaPlaybackType::Music)
                || (kind != Some(MediaPlaybackType::Video) && !meta.Artist()?.is_empty())
            {
                return Ok(true);
            }
        }
        Ok(false)
    })();
    result.media_available = media.is_ok();
    result.music_playing = media.unwrap_or(false);
    result
}

#[cfg(not(windows))]
fn read_context() -> CompanionContext {
    CompanionContext::default()
}

/// Окно, на верхнем крае которого можно сидеть.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    id: u64,
    title: String,
}

/// Куда можно перепрыгнуть: видимые окна с достаточно широким верхним краем.
///
/// Отсеиваются свёрнутые, крошечные, развёрнутые во весь экран (над ними нет
/// места — персонаж улетел бы за верх экрана) и окна самой Yuki.
#[tauri::command]
pub fn companion_surfaces(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Vec<Surface>, String> {
    let window = app.get_webview_window(WINDOW_LABEL).ok_or("Окно аватара закрыто")?;
    let height = window.outer_size().map_err(|e| e.to_string())?.height as i32;
    let own = std::process::id();
    let list = state.adapters.system.list_windows().map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .filter(|w| {
            !w.is_minimized
                && w.pid != own
                && !w.title.trim().is_empty()
                && w.bounds.width >= 320
                && w.bounds.height >= 200
                // Над окном должно хватать места для персонажа во весь рост.
                && w.bounds.y > height / 2
        })
        .map(|w| Surface { id: w.id, title: w.title })
        .collect())
}

#[tauri::command]
pub fn companion_attach(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    target: Option<u64>,
) -> Result<(), String> {
    set_setting(
        &state,
        "avatar.anchor",
        if target.is_some() {
            "window"
        } else {
            "taskbar"
        },
    )?;
    set_setting(
        &state,
        "avatar.window",
        &target.map(|v| v.to_string()).unwrap_or_default(),
    )?;
    let _ = app.emit("yuki://companion-placement", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_jump_arcs_over_and_lands_on_the_target() {
        let flight = super::Jump { from: (0, 1000), to: (400, 600), progress: 0.0 };
        assert_eq!(super::jump_point(&flight), (0, 1000));
        let top = super::jump_point(&super::Jump { progress: 0.5, ..flight });
        // На середине пути выше прямой между точками на высоту дуги.
        assert_eq!(top, (200, 800 - super::JUMP_HEIGHT as i32));
        assert_eq!(super::jump_point(&super::Jump { progress: 1.0, ..flight }), (400, 600));
    }

    #[test]
    fn archive_names_cannot_escape_library() {
        assert_eq!(asset_name("../../evil:path\\file"), "evilpathfile");
    }
    #[test]
    fn invalid_binary_is_rejected() {
        assert!(glb_json(b"not a model").is_err());
        assert!(glb_json(b"glTF00000000\xff\xff\xff\xffJSON").is_err());
    }
}

// ── Что сейчас играет (ТЗ §35) ──────────────────────────────────────────────────
//
// Windows отдаёт единый список медиасессий: в нём и плеер, и браузер, и что
// угодно ещё, объявившее себя источником звука. Мы ничего не проигрываем сами —
// только показываем и передаём нажатия тому, кто играет.

/// Дорожка, играющая прямо сейчас.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// Есть ли вообще медиасессия. Без неё карточку показывать нечего.
    pub available: bool,
    /// Играет или на паузе.
    pub playing: bool,
    pub title: String,
    pub artist: String,
    /// Кто играет: `chrome.exe`, `Spotify.exe` и так далее.
    ///
    /// Нужен как подпись, когда плеер не сообщил названия: показать «что-то
    /// играет в Chrome» честнее, чем спрятать карточку совсем.
    pub source: String,
    /// Секунды с начала дорожки.
    pub position: f64,
    /// Длительность в секундах; 0 — плеер её не сообщил.
    pub duration: f64,
    /// Какие кнопки плеер объявляет рабочими.
    ///
    /// У одиночного видео в браузере нет ни следующего трека, ни предыдущего, и
    /// кнопка, которая просто ничего не делает, выглядит сломанной. Лучше
    /// показать её выключенной.
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
}

#[tauri::command]
pub async fn media_now_playing() -> Result<NowPlaying, String> {
    tauri::async_runtime::spawn_blocking(read_now_playing)
        .await
        .map_err(|e| e.to_string())
}

/// Передаёт нажатие тому плееру, который играет.
///
/// `pause` · `play` · `play_pause` · `next` · `previous`. Плеер вправе
/// отказать — например, у радиопотока нет предыдущей дорожки, — и это не
/// ошибка Yuki.
#[tauri::command]
pub async fn media_control(action: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || control_media(&action))
        .await
        .map_err(|e| e.to_string())?
}

/// Держит многопоточный апартамент COM живым на всё время работы процесса.
///
/// WinRT отказывается работать в потоке, где COM не инициализирован, а
/// `spawn_blocking` выдаёт потоки из пула и ничего не инициализирует. Из-за
/// этого медиасессия читалась как «ничего не играет», хотя музыка играла.
/// `CoIncrementMTAUsage` решает это один раз на процесс, а не на каждый поток.
#[cfg(windows)]
fn ensure_com() {
    use std::sync::OnceLock;
    use windows::Win32::System::Com::CoIncrementMTAUsage;
    static MTA: OnceLock<()> = OnceLock::new();
    MTA.get_or_init(|| {
        // Cookie намеренно не освобождается: апартамент нужен до конца работы.
        unsafe { std::mem::forget(CoIncrementMTAUsage()) };
    });
}

/// Сессия, которой принадлежит звук прямо сейчас.
///
/// Windows не всегда назначает «текущую» сессию: у фоновой вкладки браузера её
/// может не быть, хотя музыка идёт. Тогда берём первую играющую из общего
/// списка. Один и тот же выбор нужен и карточке, и кнопкам — иначе кнопки
/// управляют не тем, что показано.
#[cfg(windows)]
fn current_session(
) -> windows::core::Result<windows::Media::Control::GlobalSystemMediaTransportControlsSession> {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as MediaManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };

    let manager = MediaManager::RequestAsync()?.get()?;
    manager.GetCurrentSession().or_else(|error| {
        manager
            .GetSessions()?
            .into_iter()
            .find(|s| {
                s.GetPlaybackInfo()
                    .and_then(|info| info.PlaybackStatus())
                    .map(|status| status == Status::Playing)
                    .unwrap_or(false)
            })
            .ok_or(error)
    })
}

/// Текущее время в тех же единицах, что `DateTime` из WinRT.
///
/// Сотни наносекунд с 1601 года — общий счёт у FILETIME и WinRT.
#[cfg(windows)]
fn now_ticks() -> i64 {
    use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    let ft = unsafe { GetSystemTimeAsFileTime() };
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

#[cfg(windows)]
fn read_now_playing() -> NowPlaying {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;

    ensure_com();
    let mut result = NowPlaying::default();
    let read = (|| -> windows::core::Result<NowPlaying> {
        let session = current_session()?;
        let meta = session.TryGetMediaPropertiesAsync()?.get()?;
        let timeline = session.GetTimelineProperties()?;
        let playback = session.GetPlaybackInfo()?;
        let status = playback.PlaybackStatus()?;

        // WinRT считает время в сотнях наносекунд.
        let seconds = |ticks: i64| ticks as f64 / 10_000_000.0;
        let end = seconds(timeline.EndTime()?.Duration);
        let start = seconds(timeline.StartTime()?.Duration);

        // Позиция в сессии — снимок на момент `LastUpdatedTime`, а не бегущие
        // часы: браузер обновляет её редко, и без досчёта строка времени стоит
        // на месте всю песню.
        let playing = status == Status::Playing;
        let elapsed = if playing {
            seconds((now_ticks() - timeline.LastUpdatedTime()?.UniversalTime).max(0))
        } else {
            0.0
        };
        let duration = (end - start).max(0.0);
        let position = (seconds(timeline.Position()?.Duration) - start + elapsed).max(0.0);
        let controls = playback.Controls()?;

        Ok(NowPlaying {
            available: true,
            playing,
            title: meta.Title()?.to_string(),
            artist: meta.Artist()?.to_string(),
            source: session.SourceAppUserModelId()?.to_string(),
            // Досчёт не должен убегать за конец дорожки, пока плеер молчит.
            position: if duration > 0.0 {
                position.min(duration)
            } else {
                position
            },
            duration,
            can_pause: controls.IsPauseEnabled().unwrap_or(false)
                || controls.IsPlayEnabled().unwrap_or(false),
            can_next: controls.IsNextEnabled().unwrap_or(false),
            can_previous: controls.IsPreviousEnabled().unwrap_or(false),
        })
    })();

    if let Ok(found) = read {
        // Сессия без названия — это всё равно «что-то играет»: часть плееров
        // метаданных не сообщает вовсе. Подпись для такого случая подберёт
        // интерфейс, а прятать карточку значит соврать, что музыки нет.
        result = found;
    }
    result
}

#[cfg(not(windows))]
fn read_now_playing() -> NowPlaying {
    NowPlaying::default()
}

#[cfg(windows)]
fn control_media(action: &str) -> Result<(), String> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;

    ensure_com();
    (|| -> windows::core::Result<bool> {
        // Та же сессия, что показана в карточке. Раньше здесь был голый
        // `GetCurrentSession`, и когда Windows не назначала текущую сессию,
        // карточка работала, а кнопки молча не делали ничего.
        let session = current_session()?;
        match action {
            "next" => session.TrySkipNextAsync()?.get(),
            "previous" => session.TrySkipPreviousAsync()?.get(),
            // Намерение приходит от интерфейса, а не вычисляется здесь.
            //
            // `TryTogglePlayPauseAsync` и любое «сделай наоборот» опасны тем,
            // что направление выбирается по состоянию, которое могло разойтись
            // с тем, что человек видит на экране. Тогда нажатие «пауза»
            // запускает музыку — ровно то, на что жаловался пользователь.
            // Просьба «поставь на паузу» у уже остановленного плеера не делает
            // ничего, и это безобидно.
            "pause" => session.TryPauseAsync()?.get(),
            "play" => session.TryPlayAsync()?.get(),
            // Запасной путь, если интерфейс не знает состояния.
            _ => {
                let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
                if playing {
                    session.TryPauseAsync()?.get()
                } else {
                    session.TryPlayAsync()?.get()
                }
            }
        }
    })()
    .map_err(|_| "плеер не отвечает".to_string())?
    .then_some(())
    .ok_or_else(|| "плеер отклонил команду".to_string())
}

#[cfg(not(windows))]
fn control_media(_action: &str) -> Result<(), String> {
    Err("управление плеером есть только на Windows".into())
}
