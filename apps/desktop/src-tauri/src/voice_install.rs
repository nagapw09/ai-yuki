//! Установка голоса по образцу одной кнопкой (OmniVoice).
//!
//! Голос Yuki — отдельная программа на Python с моделью на 3 ГБ. Раньше её
//! ставили руками в `D:\yuki-voice`, и на новом компьютере Yuki говорила
//! системным голосом, пока установку не повторят. Теперь кнопка в настройках
//! делает всё сама — так же, как уже сделано для распознавания речи:
//!
//! 1. скачивает `uv` (менеджер Python) — закреплённая версия и SHA-256;
//! 2. ставит свой Python и окружение в папку данных Yuki — системный Python не
//!    нужен и не трогается;
//! 3. ставит torch (с CUDA, если есть видеокарта NVIDIA) и OmniVoice;
//! 4. скачивает модель закреплённой ревизии;
//! 5. включает голос и поднимает сервис.
//!
//! Скачивание и запуск чужого кода — только по нажатию человека, никогда само.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const EVENT: &str = "yuki://voice-install";

const UV_URL: &str =
    "https://github.com/astral-sh/uv/releases/download/0.11.11/uv-x86_64-pc-windows-msvc.zip";
const UV_SHA256: &str = "2f75a0db2c3530b6b3c24434dc38137f61ff1f4e5f2d7b4ddc5bcd142cf58b65";
const UV_BYTES: u64 = 23_338_144;

/// Версии, с которыми голос проверен на машине владельца.
const PYTHON: &str = "3.11";
const TORCH: &[&str] = &["torch==2.11.0", "torchaudio==2.11.0"];
const PACKAGES: &[&str] = &["omnivoice==0.2.1", "soundfile", "num2words"];
const MODEL: &str = "k2-fsa/OmniVoice";
const MODEL_REVISION: &str = "c5fdb5ccb189668d56333f77ba2629f4cd7535f4";

/// Сервис лежит в репозитории и попадает в сборку целиком.
const SERVER: &str = include_str!("../../../../tools/omnivoice/server.py");

static BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Stage {
    /// Шаг по-человечески: «Скачиваю модель (3 ГБ)».
    stage: String,
    /// Номер шага и всего шагов — для полосы хода.
    step: u32,
    steps: u32,
    done: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInstallStatus {
    pub supported: bool,
    pub installed: bool,
    pub busy: bool,
    /// Куда ставится: чтобы человек знал, что удалять, если голос не нужен.
    pub folder: String,
    pub gpu: bool,
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("voice"))
}

fn python_path(root: &Path) -> PathBuf {
    root.join("venv").join("Scripts").join("python.exe")
}

/// Папка образцов голоса в данных Yuki — она переезжает вместе с переносом данных.
pub fn voices_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("voices"))
}

/// Есть ли видеокарта NVIDIA: от этого зависит, какой torch ставить.
fn has_nvidia() -> bool {
    std::process::Command::new("nvidia-smi")
        .arg("-L")
        .output()
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).contains("GPU"))
        .unwrap_or(false)
}

#[tauri::command]
pub fn voice_install_status(app: AppHandle) -> Result<VoiceInstallStatus, String> {
    let root = root(&app)?;
    Ok(VoiceInstallStatus {
        supported: cfg!(windows),
        installed: python_path(&root).exists() && root.join("server.py").exists() && root.join("hf").exists(),
        busy: BUSY.load(Ordering::Relaxed),
        folder: root.display().to_string(),
        gpu: has_nvidia(),
    })
}

/// Запускает установку в фоне; ход приходит событиями.
#[tauri::command]
pub fn voice_install(app: AppHandle) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Установка голоса одной кнопкой пока есть только для Windows".into());
    }
    if BUSY.swap(true, Ordering::Relaxed) {
        return Err("Голос уже устанавливается".into());
    }
    tauri::async_runtime::spawn(async move {
        let result = install(&app).await;
        BUSY.store(false, Ordering::Relaxed);
        let _ = app.emit(
            EVENT,
            Stage {
                stage: match &result {
                    Ok(()) => "Голос установлен".into(),
                    Err(_) => "Установка не удалась".into(),
                },
                step: STEPS,
                steps: STEPS,
                done: true,
                error: result.err(),
            },
        );
    });
    Ok(())
}

const STEPS: u32 = 6;

fn report(app: &AppHandle, step: u32, stage: &str) {
    let _ = app.emit(
        EVENT,
        Stage { stage: stage.into(), step, steps: STEPS, done: false, error: None },
    );
}

async fn install(app: &AppHandle) -> Result<(), String> {
    let root = root(app)?;
    std::fs::create_dir_all(root.join("bin")).map_err(|e| e.to_string())?;

    // 1. uv
    report(app, 1, "Скачиваю установщик Python (uv)");
    let uv = root.join("bin").join("uv.exe");
    if !uv.exists() {
        let archive = root.join("bin").join("uv.zip");
        crate::speech_local::download_with_event(app, EVENT, UV_URL, &archive, "uv", UV_SHA256, UV_BYTES)
            .await?;
        unpack_uv(&archive, &root.join("bin"))?;
        let _ = std::fs::remove_file(&archive);
    }

    // 2. Python и окружение — свои, в папке данных Yuki.
    report(app, 2, "Ставлю Python и окружение");
    if !python_path(&root).exists() {
        run(&uv, &root, &["venv", "--python", PYTHON, "venv"]).await?;
    }

    // 3. torch: с CUDA на NVIDIA, иначе для процессора (медленно, но работает).
    let gpu = has_nvidia();
    report(
        app,
        3,
        if gpu { "Ставлю torch для видеокарты (~3 ГБ)" } else { "Ставлю torch для процессора" },
    );
    let index = if gpu {
        "https://download.pytorch.org/whl/cu128"
    } else {
        "https://download.pytorch.org/whl/cpu"
    };
    let python = python_path(&root).display().to_string();
    let mut args = vec!["pip", "install", "--python", python.as_str(), "--index-url", index];
    args.extend_from_slice(TORCH);
    run(&uv, &root, &args).await?;

    // 4. OmniVoice
    report(app, 4, "Ставлю OmniVoice");
    let mut args = vec!["pip", "install", "--python", python.as_str()];
    args.extend_from_slice(PACKAGES);
    run(&uv, &root, &args).await?;
    std::fs::write(root.join("server.py"), SERVER).map_err(|e| e.to_string())?;

    // 5. Модель закреплённой ревизии — в папку Yuki, а не в общий кэш.
    report(app, 5, "Скачиваю модель голоса (3 ГБ)");
    let script = format!(
        "from huggingface_hub import snapshot_download; print(snapshot_download('{MODEL}', revision='{MODEL_REVISION}'))"
    );
    let model_dir = run_python(&python_path(&root), &root, &["-c", &script]).await?;
    let model_dir = model_dir.lines().last().unwrap_or_default().trim().to_string();
    if !Path::new(&model_dir).is_dir() {
        return Err(format!("модель не нашлась после загрузки: {model_dir}"));
    }

    // Кэш пакетов больше не нужен — это гигабайты.
    let _ = run(&uv, &root, &["cache", "clean"]).await;

    // 6. Включить голос.
    report(app, 6, "Включаю голос");
    let voices = voices_dir(app)?;
    std::fs::create_dir_all(&voices).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    for (key, value) in [
        ("voice.tts.engine", "http".to_string()),
        ("voice.tts.url", "http://127.0.0.1:9880".to_string()),
        (crate::tts_server::SETTING_PYTHON, python_path(&root).display().to_string()),
        (crate::tts_server::SETTING_SCRIPT, root.join("server.py").display().to_string()),
        (crate::tts_server::SETTING_MODEL, model_dir.clone()),
    ] {
        crate::avatar::set_setting(&state, key, &value)?;
    }
    // Папку образцов не переписываем, если человек уже выбрал свою.
    if crate::avatar::setting(&state, "voice.tts.samples").filter(|v| Path::new(v).is_dir()).is_none() {
        crate::avatar::set_setting(&state, "voice.tts.samples", &voices.display().to_string())?;
    }
    crate::tts_server::stop();
    crate::tts_server::start(app)?;
    Ok(())
}

/// Окружение для uv и Python: всё хранится в папке голоса.
fn env(root: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("UV_PYTHON_INSTALL_DIR", root.join("python")),
        ("UV_CACHE_DIR", root.join("cache")),
        ("HF_HOME", root.join("hf")),
    ]
}

async fn run(program: &Path, root: &Path, args: &[&str]) -> Result<(), String> {
    run_command(program, root, args).await.map(|_| ())
}

async fn run_python(python: &Path, root: &Path, args: &[&str]) -> Result<String, String> {
    run_command(python, root, args).await
}

/// Запускает шаг установки и возвращает его вывод.
async fn run_command(program: &Path, root: &Path, args: &[&str]) -> Result<String, String> {
    let program = program.to_path_buf();
    let root = root.to_path_buf();
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = std::process::Command::new(&program);
        command.args(&args).current_dir(&root).envs(env(&root));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let out = command.output().map_err(|e| format!("не удалось запустить {}: {e}", program.display()))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let tail: String = String::from_utf8_lossy(&out.stderr)
                .lines()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" / ");
            Err(format!("шаг не прошёл: {tail}"))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Из архива uv нужен один `uv.exe`; пути внутри архива не доверяем.
fn unpack_uv(archive: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().replace('\\', "/");
        if Path::new(&name).file_name().is_some_and(|n| n.eq_ignore_ascii_case("uv.exe")) {
            let mut out = std::fs::File::create(target.join("uv.exe")).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err("в архиве uv нет uv.exe".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bundled_server_is_the_omnivoice_one() {
        assert!(super::SERVER.contains("OmniVoice"));
        assert!(super::SERVER.contains("/tts"));
    }
}
