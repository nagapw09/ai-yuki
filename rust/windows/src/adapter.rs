use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use sysinfo::{MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use windows::core::GUID;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, TRUE};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
    SW_RESTORE,
};
use yuki_system::{
    AppInfo, Rect, SystemAdapter, SystemError, SystemInfo, SystemResult, WindowInfo,
};

/// Сколько ждём появления процесса, запущенного через оболочку.
///
/// `start` не возвращает PID, а ТЗ §5 запрещает рапортовать об успехе без подтверждения,
/// поэтому запуск считается успешным только после того, как процесс реально найден.
const APP_LAUNCH_TIMEOUT: Duration = Duration::from_secs(5);
const APP_LAUNCH_POLL: Duration = Duration::from_millis(150);

pub struct WindowsAdapter;

impl WindowsAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn platform_err(e: impl std::fmt::Display) -> SystemError {
    SystemError::Platform(e.to_string())
}

fn process_snapshot() -> System {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys
}

/// Совпадает ли имя процесса с тем, что попросил пользователь.
///
/// Пользователь говорит «открой Chrome», а процесс называется `chrome.exe`, поэтому
/// сравнение идёт без учёта регистра, расширения и пути.
fn process_matches(process_name: &str, wanted: &str) -> bool {
    let p = process_name.to_lowercase();
    let w_full = wanted.trim().to_lowercase();
    let w = w_full.rsplit(['\\', '/']).next().unwrap_or(&w_full);
    let p_stem = p.strip_suffix(".exe").unwrap_or(&p);
    let w_stem = w.strip_suffix(".exe").unwrap_or(w);
    p_stem == w_stem
}

fn find_process(app: &str) -> Option<AppInfo> {
    let sys = process_snapshot();
    sys.processes().iter().find_map(|(pid, proc_)| {
        let name = proc_.name().to_string_lossy().into_owned();
        process_matches(&name, app).then(|| AppInfo {
            pid: pid.as_u32(),
            name,
            path: proc_.exe().map(|p| p.to_string_lossy().into_owned()),
        })
    })
}

/// Колбэк `EnumWindows`: собирает видимые окна с заголовком.
///
/// # Safety
/// Вызывается только из [`SystemAdapter::list_windows`], которая передаёт в `lparam`
/// валидный указатель на `Vec<WindowInfo>`, живущий всё время обхода.
unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<WindowInfo>);

    if !IsWindowVisible(hwnd).as_bool() {
        return TRUE;
    }

    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        // Окна без заголовка — служебные; для пользователя они не существуют.
        return TRUE;
    }
    let mut buf = vec![0u16; len as usize + 1];
    let written = GetWindowTextW(hwnd, &mut buf);
    let title = String::from_utf16_lossy(&buf[..written as usize]);

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));

    let mut rect = RECT::default();
    let bounds = if GetWindowRect(hwnd, &mut rect).is_ok() {
        Rect {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        }
    } else {
        Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    };

    windows.push(WindowInfo {
        id: hwnd.0 as u64,
        title,
        // Имя приложения заполняется одним снимком процессов после обхода.
        app_name: String::new(),
        pid,
        bounds,
        is_focused: hwnd == GetForegroundWindow(),
        is_minimized: IsIconic(hwnd).as_bool(),
    });

    TRUE
}

fn audio_endpoint() -> SystemResult<IAudioEndpointVolume> {
    unsafe {
        // Повторная инициализация в уже инициализированном потоке возвращает
        // S_FALSE / RPC_E_CHANGED_MODE — для нашего сценария это не ошибка.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(platform_err)?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(platform_err)?;
        device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .map_err(platform_err)
    }
}

impl SystemAdapter for WindowsAdapter {
    fn open_app(&self, app: &str) -> SystemResult<AppInfo> {
        if app.trim().is_empty() {
            return Err(SystemError::InvalidArgument("пустое имя приложения".into()));
        }

        // Прямой запуск даёт PID сразу — самый надёжный путь.
        if let Ok(child) = Command::new(app).spawn() {
            return Ok(AppInfo {
                pid: child.id(),
                name: app.to_string(),
                path: None,
            });
        }

        // Иначе — через оболочку: она резолвит ярлыки, UWP и App Paths.
        // Пустые кавычки после start — это заголовок окна: без них start принимает
        // за заголовок первый аргумент в кавычках и ничего не запускает.
        Command::new("cmd")
            .args(["/C", "start", "", app])
            .spawn()
            .map_err(|e| SystemError::NotFound(format!("не удалось запустить {app}: {e}")))?;

        let deadline = Instant::now() + APP_LAUNCH_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(info) = find_process(app) {
                return Ok(info);
            }
            sleep(APP_LAUNCH_POLL);
        }

        Err(SystemError::NotFound(format!(
            "приложение {app} не появилось в списке процессов за {} с",
            APP_LAUNCH_TIMEOUT.as_secs()
        )))
    }

    fn close_app(&self, app: &str, force: bool) -> SystemResult<()> {
        let sys = process_snapshot();
        let targets: Vec<_> = sys
            .processes()
            .values()
            .filter(|p| process_matches(&p.name().to_string_lossy(), app))
            .collect();

        if targets.is_empty() {
            return Err(SystemError::NotFound(format!("{app} не запущено")));
        }

        let mut closed = false;
        for proc_ in targets {
            let ok = if force {
                proc_.kill()
            } else {
                // Без force просим завершиться штатно, чтобы приложение успело
                // сохранить данные пользователя.
                proc_.kill_with(sysinfo::Signal::Term).unwrap_or(false)
            };
            closed |= ok;
        }

        if closed {
            Ok(())
        } else {
            Err(SystemError::Platform(format!("не удалось завершить {app}")))
        }
    }

    fn list_apps(&self) -> SystemResult<Vec<AppInfo>> {
        let sys = process_snapshot();
        let mut apps: Vec<AppInfo> = sys
            .processes()
            .iter()
            .map(|(pid, p)| AppInfo {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                path: p.exe().map(|p| p.to_string_lossy().into_owned()),
            })
            .collect();
        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        apps.dedup_by(|a, b| a.name == b.name);
        Ok(apps)
    }

    fn list_windows(&self) -> SystemResult<Vec<WindowInfo>> {
        let mut windows: Vec<WindowInfo> = Vec::new();
        unsafe {
            EnumWindows(
                Some(collect_window),
                LPARAM(&mut windows as *mut Vec<WindowInfo> as isize),
            )
            .map_err(platform_err)?;
        }

        // Имя приложения подставляем одним снимком процессов, а не по разу на окно.
        let sys = process_snapshot();
        for w in &mut windows {
            if let Some(p) = sys.process(sysinfo::Pid::from_u32(w.pid)) {
                w.app_name = p.name().to_string_lossy().into_owned();
            }
        }
        Ok(windows)
    }

    fn focus_window(&self, window_id: u64) -> SystemResult<()> {
        let hwnd = HWND(window_id as usize as *mut std::ffi::c_void);
        unsafe {
            if IsIconic(hwnd).as_bool() {
                // Свёрнутое окно не примет фокус, пока не будет восстановлено.
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            if SetForegroundWindow(hwnd).as_bool() {
                Ok(())
            } else {
                Err(SystemError::Platform(format!(
                    "Windows отказала в переводе фокуса на окно #{window_id}"
                )))
            }
        }
    }

    fn active_window(&self) -> SystemResult<Option<WindowInfo>> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return Ok(None);
        }
        let id = hwnd.0 as u64;
        Ok(self.list_windows()?.into_iter().find(|w| w.id == id))
    }

    fn set_volume(&self, level: f32) -> SystemResult<()> {
        if !(0.0..=1.0).contains(&level) {
            return Err(SystemError::InvalidArgument(format!(
                "громкость должна быть в диапазоне 0.0–1.0, получено {level}"
            )));
        }
        let endpoint = audio_endpoint()?;
        unsafe {
            endpoint
                .SetMasterVolumeLevelScalar(level, std::ptr::null::<GUID>())
                .map_err(platform_err)
        }
    }

    fn volume(&self) -> SystemResult<f32> {
        let endpoint = audio_endpoint()?;
        unsafe { endpoint.GetMasterVolumeLevelScalar().map_err(platform_err) }
    }

    fn system_info(&self) -> SystemResult<SystemInfo> {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        sys.refresh_memory();

        Ok(SystemInfo {
            platform: "windows".into(),
            os_version: System::os_version().unwrap_or_else(|| "unknown".into()),
            arch: std::env::consts::ARCH.into(),
            hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
            cpu_count: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            total_memory_bytes: sys.total_memory(),
            available_memory_bytes: sys.available_memory(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::process_matches;

    #[test]
    fn matches_process_regardless_of_case_extension_and_path() {
        assert!(process_matches("chrome.exe", "Chrome"));
        assert!(process_matches("Code.exe", "code.exe"));
        assert!(process_matches(
            "notepad.exe",
            r"C:\Windows\System32\notepad.exe"
        ));
        assert!(!process_matches("chrome.exe", "firefox"));
    }
}
