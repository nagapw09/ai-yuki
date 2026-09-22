//! Диагностика медиасессии Windows.
//!
//! Запускается вручную, в обычный прогон не попадает:
//! `cargo test -p yuki-desktop --test media_probe -- --ignored --nocapture`
#![cfg(windows)]

use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Mgr;
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

fn now_ticks() -> i64 {
    let ft = unsafe { GetSystemTimeAsFileTime() };
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

#[test]
#[ignore]
fn probe() {
    unsafe { std::mem::forget(CoIncrementMTAUsage()) };
    let manager = match Mgr::RequestAsync().and_then(|op| op.get()) {
        Ok(m) => m,
        Err(e) => return println!("менеджер не получен: {e:?}"),
    };
    let session = match manager.GetCurrentSession() {
        Ok(s) => s,
        Err(e) => return println!("GetCurrentSession упал: {e:?}"),
    };
    println!(
        "источник: {:?}",
        session.SourceAppUserModelId().map(|v| v.to_string())
    );

    // Как ведут себя позиция и отметка обновления: если позиция стоит, её надо
    // досчитывать; если идёт сама, досчёт будет двойным.
    println!("\n  время  |  Position |  с последнего обновления | сумма");
    for _ in 0..8 {
        if let Ok(t) = session.GetTimelineProperties() {
            let pos = t.Position().map(|v| v.Duration).unwrap_or(0) as f64 / 1e7;
            let updated = t.LastUpdatedTime().map(|v| v.UniversalTime).unwrap_or(0);
            let since = (now_ticks() - updated).max(0) as f64 / 1e7;
            println!(
                "  {:6.1} | {:9.2} | {:24.2} | {:5.2}",
                0.0,
                pos,
                since,
                pos + since
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }
}

/// Проверяет только паузу и возврат к воспроизведению: дорожки не переключает,
/// чтобы не сбивать то, что слушает человек.
#[test]
#[ignore]
fn pause_probe() {
    unsafe { std::mem::forget(CoIncrementMTAUsage()) };
    let manager = Mgr::RequestAsync()
        .and_then(|op| op.get())
        .expect("менеджер");
    let session = manager.GetCurrentSession().expect("сессия");

    let status = || {
        session
            .GetPlaybackInfo()
            .and_then(|i| i.PlaybackStatus())
            .map(|s| format!("{s:?}"))
    };
    println!("статус до      : {:?}", status());
    println!(
        "TryPauseAsync  : {:?}",
        session.TryPauseAsync().and_then(|o| o.get())
    );
    std::thread::sleep(std::time::Duration::from_millis(1200));
    println!("статус после   : {:?}", status());
    println!(
        "TryPlayAsync   : {:?}",
        session.TryPlayAsync().and_then(|o| o.get())
    );
    std::thread::sleep(std::time::Duration::from_millis(1200));
    println!("статус вернули : {:?}", status());
}
