//! Реализация системного слоя для macOS 13+ на Intel и Apple Silicon (ТЗ §2, §30).
//!
//! # Почему здесь AppleScript, а не Cocoa
//!
//! Управление чужими окнами на macOS в любом случае идёт через Accessibility API и
//! требует разрешения Accessibility (ТЗ §21). `System Events` — это тот же AX API,
//! только без привязки к версии `objc2`-биндингов, и он одинаково работает на Intel
//! и Apple Silicon. Прямой AX API нужен там, где важна скорость обхода дерева —
//! это фаза 2 роадмапа (см. `yuki-accessibility`).
//!
//! Крейт намеренно не использует `cfg(target_os = "macos")` на уровне модулей: код
//! состоит только из вызовов `Command`, поэтому он компилируется и проверяется
//! компилятором на любой платформе, а не молча выпадает из сборки на Windows.

use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use sysinfo::{MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use yuki_system::{
    AppInfo, Rect, SystemAdapter, SystemError, SystemInfo, SystemResult, WindowInfo,
};

/// Приложение может стартовать заметно дольше, чем на Windows: `open -a` возвращает
/// управление сразу, поэтому успех подтверждается появлением процесса (ТЗ §5).
const APP_LAUNCH_TIMEOUT: Duration = Duration::from_secs(8);
const APP_LAUNCH_POLL: Duration = Duration::from_millis(200);

pub struct MacOsAdapter;

impl MacOsAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacOsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Выполняет AppleScript и возвращает stdout.
///
/// Ошибка `osascript` про «not allowed assistive access» означает отсутствие
/// разрешения Accessibility — она транслируется в [`SystemError::PermissionDenied`],
/// чтобы UI мог открыть нужную панель System Settings (ТЗ §21), а не показать
/// пользователю сырой текст ошибки.
fn osascript(script: &str) -> SystemResult<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| SystemError::Platform(format!("не удалось запустить osascript: {e}")))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let lowered = stderr.to_lowercase();
    if lowered.contains("assistive access") || lowered.contains("not allowed") {
        return Err(SystemError::PermissionDenied(
            "нужно разрешение Accessibility: System Settings → Privacy & Security → Accessibility"
                .into(),
        ));
    }
    Err(SystemError::Platform(stderr))
}

/// Экранирует строку для подстановки в двойные кавычки AppleScript.
fn as_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn process_snapshot() -> System {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys
}

/// На macOS процесс называется как приложение — «Google Chrome», а не `chrome.exe`,
/// поэтому сравниваем по вхождению, а не по точному совпадению.
fn process_matches(process_name: &str, wanted: &str) -> bool {
    let p = process_name.to_lowercase();
    let w_full = wanted.trim().to_lowercase();
    let w = w_full.rsplit('/').next().unwrap_or(&w_full);
    let w = w.strip_suffix(".app").unwrap_or(w);
    p == w || p.contains(w)
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

/// Разбирает строку TSV, которую отдаёт скрипт перечисления окон.
fn parse_window_row(row: &str, focused_app: &str) -> Option<WindowInfo> {
    let mut parts = row.split('\t');
    let id: u64 = parts.next()?.trim().parse().ok()?;
    let title = parts.next()?.trim().to_string();
    let app_name = parts.next()?.trim().to_string();
    let pid: u32 = parts.next()?.trim().parse().ok()?;
    let x: i32 = parts.next()?.trim().parse().ok()?;
    let y: i32 = parts.next()?.trim().parse().ok()?;
    let width: i32 = parts.next()?.trim().parse().ok()?;
    let height: i32 = parts.next()?.trim().parse().ok()?;
    let is_minimized = matches!(parts.next()?.trim(), "true");

    Some(WindowInfo {
        id,
        title,
        is_focused: app_name == focused_app,
        app_name,
        pid,
        bounds: Rect {
            x,
            y,
            width,
            height,
        },
        is_minimized,
    })
}

const LIST_WINDOWS_SCRIPT: &str = r#"
set out to ""
tell application "System Events"
  repeat with p in (every process whose visible is true)
    set pname to name of p
    set ppid to unix id of p
    repeat with w in (every window of p)
      try
        set {wx, wy} to position of w
        set {ww, wh} to size of w
        set wmin to "false"
        try
          if value of attribute "AXMinimized" of w is true then set wmin to "true"
        end try
        set out to out & (id of w as text) & tab & (name of w) & tab & pname & tab & (ppid as text) & tab & (wx as text) & tab & (wy as text) & tab & (ww as text) & tab & (wh as text) & tab & wmin & linefeed
      end try
    end repeat
  end repeat
end tell
return out
"#;

const FRONT_APP_SCRIPT: &str = r#"
tell application "System Events" to return name of first process whose frontmost is true
"#;

impl SystemAdapter for MacOsAdapter {
    fn open_app(&self, app: &str) -> SystemResult<AppInfo> {
        if app.trim().is_empty() {
            return Err(SystemError::InvalidArgument("пустое имя приложения".into()));
        }

        // `-b` для bundle id вида com.google.Chrome, `-a` для человеческого имени.
        let flag = if app.contains('.') && !app.contains('/') && !app.ends_with(".app") {
            "-b"
        } else {
            "-a"
        };

        let status = Command::new("open")
            .args([flag, app])
            .status()
            .map_err(|e| SystemError::Platform(format!("не удалось выполнить open: {e}")))?;

        if !status.success() {
            return Err(SystemError::NotFound(format!(
                "macOS не нашла приложение {app}"
            )));
        }

        let deadline = Instant::now() + APP_LAUNCH_TIMEOUT;
        loop {
            if let Some(info) = find_process(app) {
                return Ok(info);
            }
            if Instant::now() >= deadline {
                return Err(SystemError::NotFound(format!(
                    "приложение {app} не появилось в списке процессов за {} с",
                    APP_LAUNCH_TIMEOUT.as_secs()
                )));
            }
            sleep(APP_LAUNCH_POLL);
        }
    }

    fn close_app(&self, app: &str, force: bool) -> SystemResult<()> {
        if !force {
            // Штатный quit даёт приложению сохранить данные пользователя.
            let script = format!("tell application \"{}\" to quit", as_literal(app));
            if osascript(&script).is_ok() {
                return Ok(());
            }
        }

        let sys = process_snapshot();
        let targets: Vec<_> = sys
            .processes()
            .values()
            .filter(|p| process_matches(&p.name().to_string_lossy(), app))
            .collect();

        if targets.is_empty() {
            return Err(SystemError::NotFound(format!("{app} не запущено")));
        }

        let killed = targets.into_iter().fold(false, |acc, p| acc | p.kill());
        if killed {
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
        let focused = osascript(FRONT_APP_SCRIPT).unwrap_or_default();
        let raw = osascript(LIST_WINDOWS_SCRIPT)?;
        Ok(raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| parse_window_row(l, &focused))
            .collect())
    }

    fn focus_window(&self, window_id: u64) -> SystemResult<()> {
        let window = self
            .list_windows()?
            .into_iter()
            .find(|w| w.id == window_id)
            .ok_or_else(|| SystemError::NotFound(format!("окно #{window_id}")))?;

        // Сначала поднимаем окно внутри приложения, затем выводим само приложение
        // вперёд: без второго шага окно поднимется, но фокус останется у другого app.
        let script = format!(
            r#"tell application "System Events"
  set p to first process whose name is "{app}"
  set w to first window of p whose id is {id}
  perform action "AXRaise" of w
  set frontmost of p to true
end tell"#,
            app = as_literal(&window.app_name),
            id = window_id
        );
        osascript(&script).map(|_| ())
    }

    fn active_window(&self) -> SystemResult<Option<WindowInfo>> {
        Ok(self.list_windows()?.into_iter().find(|w| w.is_focused))
    }

    fn set_volume(&self, level: f32) -> SystemResult<()> {
        if !(0.0..=1.0).contains(&level) {
            return Err(SystemError::InvalidArgument(format!(
                "громкость должна быть в диапазоне 0.0–1.0, получено {level}"
            )));
        }
        let percent = (level * 100.0).round() as i32;
        osascript(&format!("set volume output volume {percent}")).map(|_| ())
    }

    fn volume(&self) -> SystemResult<f32> {
        let raw = osascript("output volume of (get volume settings)")?;
        raw.trim()
            .parse::<f32>()
            .map(|v| v / 100.0)
            .map_err(|_| SystemError::Platform(format!("неожиданный ответ о громкости: {raw}")))
    }

    fn system_info(&self) -> SystemResult<SystemInfo> {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        sys.refresh_memory();

        Ok(SystemInfo {
            platform: "macos".into(),
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
    use super::{as_literal, parse_window_row, process_matches};

    #[test]
    fn matches_process_by_human_readable_name() {
        assert!(process_matches("Google Chrome", "chrome"));
        assert!(process_matches("Safari", "Safari.app"));
        assert!(process_matches("Notes", "/System/Applications/Notes.app"));
        assert!(!process_matches("Safari", "firefox"));
    }

    #[test]
    fn escapes_quotes_and_backslashes_for_applescript() {
        assert_eq!(as_literal(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn parses_window_row_and_marks_focus_by_app() {
        let row = "42\tInbox\tMail\t501\t10\t20\t800\t600\tfalse";
        let w = parse_window_row(row, "Mail").expect("строка должна разобраться");
        assert_eq!(w.id, 42);
        assert_eq!(w.title, "Inbox");
        assert_eq!(w.bounds.width, 800);
        assert!(w.is_focused);
        assert!(!w.is_minimized);
    }

    #[test]
    fn rejects_malformed_window_row() {
        assert!(parse_window_row("не строка окна", "Mail").is_none());
    }
}
