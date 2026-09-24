//! Сервис голоса, который Yuki поднимает сама.
//!
//! Голос по образцу (XTTS, Silero) — отдельная программа на Python. Раньше её
//! надо было запускать руками перед каждым разговором, и без этого Yuki молча
//! не говорила. Теперь достаточно указать, чем и что запускать: при старте Yuki
//! поднимает сервис скрытым процессом и снимает его вместе с собой.
//!
//! Команда задаётся человеком в настройках — никакой загрузки и запуска
//! чужого кода по собственной инициативе.

use std::process::Child;
use std::sync::{Mutex, OnceLock};

use tauri::{AppHandle, Manager};

use crate::AppState;

/// Интерпретатор, например `D:\yuki-voice\xtts\.venv\Scripts\python.exe`.
pub const SETTING_PYTHON: &str = "voice.tts.server.python";
/// Скрипт сервиса, например `tools\xtts\server.py`.
pub const SETTING_SCRIPT: &str = "voice.tts.server.script";
/// Папка модели, скачанной установщиком: сервис берёт её без сети.
pub const SETTING_MODEL: &str = "voice.tts.server.model";

static SERVER: Mutex<Option<Child>> = Mutex::new(None);
#[cfg(windows)]
static JOB: OnceLock<isize> = OnceLock::new();

/// Порт из адреса сервиса: скрипт должен слушать там, куда ходит Yuki.
fn port_of(url: &str) -> u16 {
    url.rsplit(':')
        .next()
        .and_then(|tail| tail.trim_end_matches('/').parse().ok())
        .unwrap_or(9880)
}

/// Поднимает сервис, если голос идёт через него и команда задана.
pub fn start(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let setting = |key: &str| crate::avatar::setting(&state, key).filter(|v| !v.trim().is_empty());

    if setting("voice.tts.engine").as_deref() != Some("http") {
        return Ok(());
    }
    let (Some(python), Some(script)) = (setting(SETTING_PYTHON), setting(SETTING_SCRIPT)) else {
        return Ok(());
    };

    let mut guard = SERVER.lock().map_err(|_| "состояние сервиса голоса повреждено")?;
    if let Some(child) = guard.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return Ok(());
        }
    }

    let port = port_of(&setting("voice.tts.url").unwrap_or_default());
    let mut command = std::process::Command::new(&python);
    command.arg(&script).arg("--port").arg(port.to_string());
    // Модель, скачанная установщиком: путь к ней и работа без сети — чтобы
    // сервис не лез за обновлениями и не зависел от интернета при старте.
    if let Some(model) = setting(SETTING_MODEL).filter(|m| std::path::Path::new(m).is_dir()) {
        command.arg("--model").arg(&model).env("HF_HUB_OFFLINE", "1");
        if let Some(hf) = std::path::Path::new(&model).ancestors().find(|p| p.file_name().is_some_and(|n| n == "hf")) {
            command.env("HF_HOME", hf);
        }
    }
    // Образец по умолчанию — выбранный в настройках.
    if let (Some(dir), Some(sample)) = (setting("voice.tts.samples"), setting("voice.tts.sample")) {
        let reference = std::path::Path::new(&dir).join(sample);
        if reference.is_file() {
            command.arg("--ref").arg(reference);
        }
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(dir) = std::path::Path::new(&script).parent() {
        command.current_dir(dir);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command
        .spawn()
        .map_err(|e| format!("не удалось запустить сервис голоса: {e}"))?;

    #[cfg(windows)]
    attach_to_job(&child);

    *guard = Some(child);
    Ok(())
}

/// Поднимает сервис при старте приложения.
pub fn restore(app: &AppHandle) {
    if let Err(error) = start(app) {
        tracing::warn!("сервис голоса не поднялся: {error}");
    }
}

/// Снимает сервис при выходе.
pub fn stop() {
    if let Ok(mut guard) = SERVER.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Перезапуск после смены настроек в интерфейсе.
#[tauri::command]
pub fn tts_server_restart(app: AppHandle) -> Result<(), String> {
    stop();
    start(&app)
}

/// Привязка к job-объекту: при аварии Yuki Windows снимет и сервис, иначе он
/// остался бы держать гигабайты видеопамяти и порт.
#[cfg(windows)]
fn attach_to_job(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::*;

    let job = *JOB.get_or_init(|| unsafe {
        let Ok(job) = CreateJobObjectW(None, None) else {
            return 0;
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .is_err()
        {
            return 0;
        }
        job.0 as isize
    });
    if job == 0 {
        tracing::warn!("job-объект недоступен: сервис голоса может пережить аварию Yuki");
        return;
    }
    unsafe {
        let _ = AssignProcessToJobObject(HANDLE(job as _), HANDLE(child.as_raw_handle() as _));
    }
}

#[cfg(test)]
mod tests {
    use super::port_of;

    #[test]
    fn the_service_listens_where_yuki_calls_it() {
        assert_eq!(port_of("http://127.0.0.1:9880"), 9880);
        assert_eq!(port_of("http://127.0.0.1:9881/"), 9881);
        assert_eq!(port_of(""), 9880);
    }
}
