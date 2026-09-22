//! Локальное распознавание речи без ключей и без облака (ТЗ §10, §29).
//!
//! # Почему отдельным процессом
//!
//! Whisper — это C++ и математика на сотни мегабайт весов. Вкомпилировать его в
//! Yuki значит привязать сборку приложения к LLVM и cmake и таскать модель в
//! установщике. Поэтому движок живёт рядом: официальная сборка `whisper.cpp`
//! поднимает HTTP-сервер на localhost, а Yuki ходит в него тем же кодом, каким
//! ходила бы в OpenAI. «Локально» здесь означает `http://127.0.0.1`, а не
//! другую реализацию распознавания.
//!
//! # Почему скачивание, а не поставка в установщике
//!
//! Модель весит больше самого приложения, и нужна она только тем, кто включил
//! голос. Человек нажимает кнопку и видит, что именно и откуда качается, —
//! это честнее, чем полугигабайтный установщик у всех.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Mutex;
#[cfg(windows)]
use std::sync::OnceLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

/// Порт, на котором слушает движок.
///
/// Фиксированный, а не случайный: адрес попадает в настройку `voice.stt.url`,
/// и при случайном порте она протухала бы на каждом перезапуске.
const PORT: u16 = 8756;

/// Сборка `whisper.cpp` для Windows x64.
///
/// Версия и контрольная сумма закреплены: скачанный и запущенный исполняемый
/// файл — это доверие, и оно не должно зависеть от того, что лежит по ссылке
/// сегодня.
const RUNTIME_URL: &str =
    "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-bin-x64.zip";
const RUNTIME_SHA256: &str = "49dcc16de826f20bd53d44f947a1ae49dfa81f86cad67a64d80820cb192d674a";

/// Ключи настроек.
const SETTING_ENABLED: &str = "voice.stt.local";
const SETTING_MODEL: &str = "voice.stt.local.model";
const SETTING_URL: &str = "voice.stt.url";

/// Ход загрузки — интерфейс показывает мегабайты, а не «подождите».
const EVENT_PROGRESS: &str = "yuki://speech-download";

/// Что можно скачать.
///
/// Две модели, а не десять: разница между ними — это выбор «быстро» или
/// «точнее», и он понятен без таблицы. `base` ошибается на числах и именах,
/// `small` на тех же фразах не ошибается, но думает втрое дольше.
struct ModelSpec {
    id: &'static str,
    file: &'static str,
    sha256: &'static str,
    bytes: u64,
}

const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "base",
        file: "ggml-base.bin",
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        bytes: 147_951_465,
    },
    ModelSpec {
        id: "small",
        file: "ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        bytes: 487_601_967,
    },
];

/// Модель по умолчанию: правильно понятая команда важнее двух секунд.
const DEFAULT_MODEL: &str = "small";

fn model_spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

fn model_url(spec: &ModelSpec) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        spec.file
    )
}

/// Запущенный движок.
#[derive(Default)]
pub struct SpeechState {
    server: Mutex<Option<Child>>,
    /// Идёт ли скачивание: две одновременные загрузки писали бы в один файл.
    busy: Mutex<bool>,
    /// Job-объект Windows, к которому привязан движок.
    ///
    /// Обычное закрытие Yuki движок останавливает явно. Но при аварии или
    /// убийстве процесса обработчик не выполнится, и сервер с моделью остался
    /// бы висеть на полгигабайта и держать порт. Windows закрывает job вместе
    /// с нашим процессом, а job уносит с собой всё, что в него помещено.
    #[cfg(windows)]
    job: OnceLock<isize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSpeechStatus {
    /// Поддерживается ли локальный движок на этой системе.
    pub supported: bool,
    /// Скачан ли сам движок.
    pub runtime_ready: bool,
    /// Какие модели уже лежат на диске.
    pub models: Vec<ModelStatus>,
    /// Выбранная модель.
    pub model: String,
    /// Включено ли локальное распознавание.
    pub enabled: bool,
    /// Работает ли процесс прямо сейчас.
    pub running: bool,
    pub port: u16,
    pub busy: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub bytes: u64,
    pub downloaded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    /// `runtime` или идентификатор модели.
    what: String,
    downloaded: u64,
    total: u64,
    done: bool,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(err)?.join("speech"))
}

fn runtime_exe(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(root(app)?.join("runtime").join("whisper-server.exe"))
}

fn model_path(app: &AppHandle, spec: &ModelSpec) -> Result<PathBuf, String> {
    Ok(root(app)?.join("models").join(spec.file))
}

/// Поддерживается ли локальный движок здесь.
///
/// Официальная сборка есть только под Windows x64. Обещать её на macOS, где
/// сборки нет, значило бы показать кнопку, которая не работает.
const fn supported() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
}

pub fn selected_model(state: &AppState) -> String {
    crate::avatar::setting(state, SETTING_MODEL)
        .filter(|id| model_spec(id).is_some())
        .unwrap_or_else(|| DEFAULT_MODEL.into())
}

#[tauri::command]
pub fn local_speech_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LocalSpeechStatus, String> {
    let model = selected_model(&state);
    let running = state
        .speech
        .server
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);

    Ok(LocalSpeechStatus {
        supported: supported(),
        runtime_ready: runtime_exe(&app).map(|p| p.exists()).unwrap_or(false),
        models: MODELS
            .iter()
            .map(|spec| ModelStatus {
                id: spec.id.into(),
                bytes: spec.bytes,
                downloaded: model_path(&app, spec).map(|p| p.exists()).unwrap_or(false),
            })
            .collect(),
        model,
        enabled: crate::avatar::setting(&state, SETTING_ENABLED).as_deref() == Some("on"),
        running,
        port: PORT,
        busy: state.speech.busy.lock().map(|b| *b).unwrap_or(false),
    })
}

/// Скачивает движок и модель, затем включает локальное распознавание.
///
/// Всё в одной команде, потому что для человека это одно действие: «пусть
/// понимает меня без ключей». Разбивать его на «скачать движок», «скачать
/// модель», «прописать адрес» значит переложить на него нашу внутреннюю
/// раскладку файлов.
#[tauri::command]
pub async fn local_speech_install(app: AppHandle, model: Option<String>) -> Result<(), String> {
    if !supported() {
        return Err("Локальный движок распознавания есть только для Windows x64.".into());
    }
    let state = app.state::<AppState>();
    let id = model.unwrap_or_else(|| selected_model(&state));
    let spec = model_spec(&id).ok_or("Неизвестная модель распознавания")?;

    {
        let mut busy = state
            .speech
            .busy
            .lock()
            .map_err(|_| "состояние загрузки повреждено".to_string())?;
        if *busy {
            return Err("Загрузка уже идёт".into());
        }
        *busy = true;
    }

    let result = install(&app, spec).await;

    if let Ok(mut busy) = state.speech.busy.lock() {
        *busy = false;
    }
    result?;

    let state = app.state::<AppState>();
    crate::avatar::set_setting(&state, SETTING_MODEL, spec.id)?;
    crate::avatar::set_setting(&state, SETTING_ENABLED, "on")?;
    crate::avatar::set_setting(&state, SETTING_URL, &endpoint())?;
    start(&app)
}

async fn install(app: &AppHandle, spec: &'static ModelSpec) -> Result<(), String> {
    let root = root(app)?;
    std::fs::create_dir_all(root.join("models")).map_err(err)?;

    let exe = runtime_exe(app)?;
    if !exe.exists() {
        let archive = root.join("runtime.zip");
        download(app, RUNTIME_URL, &archive, "runtime", RUNTIME_SHA256, 0).await?;
        unpack(&archive, &root.join("runtime"))?;
        let _ = std::fs::remove_file(&archive);
        if !exe.exists() {
            return Err("В архиве движка нет whisper-server.exe".into());
        }
    }

    let target = model_path(app, spec)?;
    if !target.exists() {
        download(
            app,
            &model_url(spec),
            &target,
            spec.id,
            spec.sha256,
            spec.bytes,
        )
        .await?;
    }
    Ok(())
}

/// Качает файл, показывая прогресс и сверяя контрольную сумму.
///
/// Во временный файл: оборванная загрузка не должна оставить на диске огрызок,
/// который в следующий раз примут за готовую модель.
async fn download(
    app: &AppHandle,
    url: &str,
    target: &Path,
    what: &str,
    sha256: &str,
    expected: u64,
) -> Result<(), String> {
    // Свой клиент, а не общий: у общего стоит потолок в десять минут на запрос,
    // рассчитанный на ответ модели. Полгигабайта модели на медленном канале в
    // него не укладываются, и загрузка обрывалась бы у самого конца.
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("Yuki/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(err)?;
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|e| format!("не удалось скачать: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{} вернул {}", url, response.status()));
    }

    let total = response.content_length().unwrap_or(expected);
    let temporary = target.with_extension("part");
    let mut file = std::fs::File::create(&temporary).map_err(err)?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut reported = 0u64;
    let mut response = response;

    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(e) => {
                let _ = std::fs::remove_file(&temporary);
                return Err(format!("загрузка оборвалась: {e}"));
            }
        };
        use std::io::Write;
        file.write_all(&chunk).map_err(err)?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;

        // Событие на каждый мегабайт: чаще — это сотни сообщений в секунду
        // в интерфейс ради цифры, которая всё равно меняется плавно.
        if downloaded - reported >= 1_048_576 {
            reported = downloaded;
            let _ = app.emit(
                EVENT_PROGRESS,
                Progress {
                    what: what.into(),
                    downloaded,
                    total,
                    done: false,
                },
            );
        }
    }
    drop(file);

    let actual = hasher.finish();
    if !actual.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "контрольная сумма не совпала: ожидали {sha256}, получили {actual}"
        ));
    }

    std::fs::rename(&temporary, target).map_err(err)?;
    let _ = app.emit(
        EVENT_PROGRESS,
        Progress {
            what: what.into(),
            downloaded,
            total,
            done: true,
        },
    );
    Ok(())
}

/// Распаковывает движок, складывая всё в один каталог.
///
/// Пути внутри архива игнорируются намеренно: имя записи в ZIP задаёт тот, кто
/// архив собрал, и `..` в нём вывел бы запись за пределы каталога.
///
/// Берётся только сервер и библиотеки. В архиве лежит ещё десяток программ —
/// тесты, бенчмарки, голосовые шахматы, — и раскладывать чужие исполняемые
/// файлы по пользовательской папке за то, что они оказались в одном ZIP с
/// нужным, незачем.
fn unpack(archive: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(err)?;
    let file = std::fs::File::open(archive).map_err(err)?;
    let mut zip = zip::ZipArchive::new(file).map_err(err)?;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(err)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let Some(name) = Path::new(&name).file_name().map(|n| n.to_owned()) else {
            continue;
        };
        let extension = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let wanted = extension == "dll"
            || Path::new(&name)
                .file_name()
                .is_some_and(|file| file.eq_ignore_ascii_case("whisper-server.exe"));
        if !wanted {
            continue;
        }
        if entry.size() > 200 * 1024 * 1024 {
            return Err("Файл в архиве движка неправдоподобно большой".into());
        }
        let mut out = std::fs::File::create(target.join(&name)).map_err(err)?;
        std::io::copy(&mut entry, &mut out).map_err(err)?;
    }
    Ok(())
}

/// Адрес, по которому Yuki обращается к движку.
fn endpoint() -> String {
    format!("http://127.0.0.1:{PORT}/inference")
}

/// Поднимает движок, если он скачан и включён.
///
/// Вызывается и при старте приложения, и после установки: повторный вызов при
/// уже работающем процессе ничего не делает.
pub fn start(app: &AppHandle) -> Result<(), String> {
    if !supported() {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let mut guard = state
        .speech
        .server
        .lock()
        .map_err(|_| "состояние движка повреждено".to_string())?;

    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            // Живой процесс — второй не нужен.
            Ok(None) => return Ok(()),
            _ => *guard = None,
        }
    }

    let exe = runtime_exe(app)?;
    let model = model_path(
        app,
        model_spec(&selected_model(&state)).ok_or("нет модели")?,
    )?;
    if !exe.exists() || !model.exists() {
        return Err("Локальное распознавание ещё не скачано".into());
    }

    let language = crate::avatar::setting(&state, "voice.language").unwrap_or_else(|| "ru".into());
    let mut command = std::process::Command::new(&exe);
    command
        .arg("--model")
        .arg(&model)
        .arg("--language")
        .arg(&language)
        .arg("--port")
        .arg(PORT.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--no-timestamps")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Без этого у фонового движка на секунду мигает окно консоли.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command
        .spawn()
        .map_err(|e| format!("не удалось запустить локальное распознавание: {e}"))?;

    #[cfg(windows)]
    attach_to_job(&state.speech, &child);

    *guard = Some(child);
    Ok(())
}

/// Привязывает движок к job-объекту, который Windows закроет вместе с Yuki.
///
/// Неудача здесь не повод не запускать распознавание: движок и так снимается
/// при обычном закрытии, а job — страховка на случай аварии.
#[cfg(windows)]
fn attach_to_job(state: &SpeechState, child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::*;

    let job = *state.job.get_or_init(|| unsafe {
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
        tracing::warn!("job-объект недоступен: движок может пережить аварию Yuki");
        return;
    }
    unsafe {
        let _ = AssignProcessToJobObject(HANDLE(job as _), HANDLE(child.as_raw_handle() as _));
    }
}

#[tauri::command]
pub fn local_speech_start(app: AppHandle) -> Result<(), String> {
    start(&app)
}

/// Останавливает движок и выключает локальный режим.
#[tauri::command]
pub fn local_speech_stop(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    stop(&app);
    crate::avatar::set_setting(&state, SETTING_ENABLED, "off")?;
    // Адрес указывал на наш же процесс: оставлять его после выключения значит
    // оставить распознавание, которое молча не отвечает.
    if crate::avatar::setting(&state, SETTING_URL).as_deref() == Some(endpoint().as_str()) {
        state
            .storage
            .with_conn(|conn| conn.execute("DELETE FROM settings WHERE key = ?1", [SETTING_URL]))
            .map_err(err)?;
    }
    Ok(())
}

/// Убивает процесс движка.
pub fn stop(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Ok(mut guard) = state.speech.server.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    };
}

/// Поднимает движок при старте приложения, если человек его включил.
pub fn restore(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if crate::avatar::setting(&state, SETTING_ENABLED).as_deref() != Some("on") {
        return;
    }
    if let Err(error) = start(app) {
        tracing::warn!("локальное распознавание не поднялось: {error}");
    }
}

// ── SHA-256 ─────────────────────────────────────────────────────────────────────
//
// Своя реализация, а не крейт: она нужна ровно в одном месте — сверить два
// скачанных файла, — и тянуть ради этого зависимость с транзитивным хвостом
// в приложение, которое и так собирается семь минут, незачем.

struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    filled: usize,
    length: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            filled: 0,
            length: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        while !data.is_empty() {
            let take = (64 - self.filled).min(data.len());
            self.buffer[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.filled = 0;
            }
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (index, chunk) in block.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        for (slot, value) in self
            .state
            .iter_mut()
            .zip([a, b, c, d, e, f, g, h].into_iter())
        {
            *slot = slot.wrapping_add(value);
        }
    }

    fn finish(mut self) -> String {
        let bits = self.length.wrapping_mul(8);
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());

        let mut out = String::with_capacity(64);
        for word in self.state {
            out.push_str(&format!("{word:08x}"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finish()
    }

    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_survives_being_fed_in_odd_pieces() {
        // Загрузка приходит кусками произвольного размера, и сумма обязана
        // совпасть с посчитанной за один вызов — иначе проверка бесполезна.
        let data: Vec<u8> = (0..1000u32).map(|v| (v % 251) as u8).collect();
        let whole = digest(&data);

        let mut hasher = Sha256::new();
        for piece in data.chunks(7) {
            hasher.update(piece);
        }
        assert_eq!(hasher.finish(), whole);
    }

    #[test]
    fn every_model_is_pinned_to_a_checksum() {
        for spec in MODELS {
            assert_eq!(spec.sha256.len(), 64, "модель {} без суммы", spec.id);
            assert!(spec.bytes > 0);
        }
        assert!(model_spec(DEFAULT_MODEL).is_some());
        assert!(model_spec("нет такой").is_none());
    }

    #[test]
    fn the_endpoint_is_local_and_is_the_whisper_path() {
        // Адрес уходит в общую настройку распознавания: если он перестанет
        // быть локальным, звук начнёт уходить наружу молча.
        assert_eq!(endpoint(), "http://127.0.0.1:8756/inference");
        assert!(crate::privacy::is_local_url(&endpoint()));
    }
}
