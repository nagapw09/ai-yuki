//! Команды голосового режима (ТЗ §10).
//!
//! Конвейер живёт в крейте `yuki-voice`, здесь — только его подключение к
//! приложению: разрешение на микрофон (ТЗ §21), настройки распознавания и
//! события для интерфейса.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use yuki_voice::{
    strip_wake_phrase, HttpStt, ListenMode, SpeechToText, SystemTts, TextToSpeech, VoiceEvent,
    WakeModel, ENROLL_SAMPLES,
    VoiceSession,
};

use crate::state::AppState;

/// Громкость входа 0…1 — по ней Orb реагирует на голос (ТЗ §13).
const EVENT_LEVEL: &str = "yuki://voice-level";
/// Смена состояния: слушает, распознаёт, молчит.
const EVENT_STATE: &str = "yuki://voice-state";
/// Распознанная фраза.
const EVENT_TEXT: &str = "yuki://voice-transcribed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LevelEvent {
    level: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StateEvent {
    /// `listening` · `speech` · `transcribing` · `idle` · `error`
    state: &'static str,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscribedEvent {
    text: String,
    /// Обращались ли к Yuki: в режиме слова пробуждения остальное — не команда.
    addressed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    pub listening: bool,
    pub mode: Option<ListenMode>,
    pub speaking: bool,
    pub input_device: Option<String>,
    pub devices: Vec<String>,
    pub voices: Vec<String>,
    /// Настроено ли распознавание: без него голосовой ввод невозможен.
    pub stt_ready: bool,
}

/// Всё, что нужно, чтобы обратиться к сервису распознавания.
struct SttConfig {
    base_url: String,
    api_key: Option<String>,
    model: String,
    language: Option<String>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .storage
        .with_conn(|conn| {
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .ok()
        .flatten()
}

/// Проверяет, что пользователь разрешил микрофон (ТЗ §21).
///
/// Проверка идёт до открытия устройства: включать микрофон, чтобы потом
/// обнаружить запрет, — ровно то, чего политика разрешений не допускает.
fn ensure_microphone_allowed(state: &AppState) -> Result<(), String> {
    let (granted, os_granted): (bool, bool) = state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT granted, os_granted FROM permissions WHERE category = 'microphone'",
                [],
                |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)? != 0)),
            )
        })
        .map_err(err)?;

    if !os_granted {
        return Err(
            "нет системного разрешения на микрофон — выдайте его в настройках ОС".into(),
        );
    }
    if !granted {
        return Err("микрофон выключен в разрешениях Yuki".into());
    }
    Ok(())
}

/// Собирает настройки распознавания.
///
/// Эндпоинт транскрипции есть только у провайдеров протокола OpenAI, поэтому
/// Anthropic и Gemini сюда не годятся — и об этом надо сказать прямо, а не
/// падать позже с невнятной ошибкой HTTP.
fn stt_config(state: &AppState) -> Result<SttConfig, String> {
    let provider_id = setting(state, "voice.stt.provider");

    let row: Option<(String, String, String, Option<String>)> = state
        .storage
        .with_conn(|conn| {
            let result = match &provider_id {
                Some(id) => conn.query_row(
                    "SELECT id, kind, base_url, secret_ref FROM providers WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                ),
                None => conn.query_row(
                    "SELECT id, kind, base_url, secret_ref FROM providers
                     WHERE enabled = 1 AND kind IN ('openai', 'openrouter', 'ollama', 'lmstudio', 'custom')
                     ORDER BY is_default DESC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                ),
            };
            result.map(Some).or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .map_err(err)?;

    let (_, kind, base_url, secret_ref) = row.ok_or_else(|| {
        "распознавание речи не настроено: включите провайдера протокола OpenAI \
         (OpenAI, OpenRouter, локальный Whisper) в настройках"
            .to_string()
    })?;

    if matches!(kind.as_str(), "anthropic" | "gemini") {
        return Err(format!(
            "у провайдера «{kind}» нет распознавания речи — выберите другой в настройках голоса"
        ));
    }

    // Распознавание увозит звук голоса целиком — в режиме Local Only (ТЗ §29) это
    // точно такая же утечка, как и отправка реплики в облачную модель.
    crate::privacy::ensure_allowed(&state.storage, &base_url, "распознавание речи")?;

    Ok(SttConfig {
        base_url,
        api_key: secret_ref
            .as_deref()
            .and_then(|r| crate::secrets::get(r).ok().flatten()),
        model: setting(state, "voice.stt.model").unwrap_or_else(|| "whisper-1".into()),
        language: setting(state, "voice.language").or_else(|| Some("ru".into())),
    })
}

// ── Состояние ───────────────────────────────────────────────────────────────────

/// Голосовая часть состояния приложения.
#[derive(Default)]
pub struct VoiceState {
    /// Образцы обращения, записанные но ещё не собранные в модель (ТЗ §37).
    ///
    /// В памяти, а не в базе: незаконченная запись не должна переживать
    /// перезапуск — человек и так начнёт заново.
    enrollment: Mutex<Vec<Vec<f32>>>,
    session: Mutex<Option<VoiceSession>>,
    tts: Mutex<Option<SystemTts>>,
}

impl VoiceState {
    /// Создаёт синтезатор при первом обращении.
    ///
    /// Лениво, потому что движок ОС инициализируется небыстро, а пользователь
    /// может вообще не включать голос — платить за это стартом приложения
    /// (бюджет ТЗ §37) незачем.
    fn with_tts<T>(&self, f: impl FnOnce(&SystemTts) -> Result<T, String>) -> Result<T, String> {
        let mut guard = self
            .tts
            .lock()
            .map_err(|_| "состояние синтезатора повреждено".to_string())?;

        if guard.is_none() {
            *guard = Some(SystemTts::new().map_err(err)?);
        }
        f(guard.as_ref().expect("синтезатор только что создан"))
    }
}

// ── Команды ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn voice_status(state: State<'_, AppState>) -> Result<VoiceStatus, String> {
    let session = state
        .voice
        .session
        .lock()
        .map_err(|_| "состояние голоса повреждено".to_string())?;

    Ok(VoiceStatus {
        listening: session.is_some(),
        mode: session.as_ref().map(|s| s.mode()),
        speaking: state.voice.with_tts(|t| Ok(t.is_speaking())).unwrap_or(false),
        input_device: yuki_voice::default_input_name(),
        devices: yuki_voice::input_devices(),
        voices: state.voice.with_tts(|t| Ok(t.voices())).unwrap_or_default(),
        stt_ready: stt_config(&state).is_ok(),
    })
}

/// Начинает слушать.
#[tauri::command]
pub fn voice_start(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: ListenMode,
) -> Result<(), String> {
    ensure_microphone_allowed(&state)?;
    let config = stt_config(&state)?;

    let mut guard = state
        .voice
        .session
        .lock()
        .map_err(|_| "состояние голоса повреждено".to_string())?;
    if guard.is_some() {
        return Err("голосовой режим уже запущен".into());
    }

    let events = app.clone();
    // Образцы обращения читаются при старте сессии: их могли записать только что.
    let wake = wake_model(&state);
    let fast_wake = wake.is_some();

    let mut session = VoiceSession::start(mode, wake, move |event| match event {
        VoiceEvent::Level(level) => {
            let _ = events.emit(EVENT_LEVEL, LevelEvent { level });
        }
        VoiceEvent::SpeechStarted => {
            let _ = events.emit(
                EVENT_STATE,
                StateEvent {
                    state: "speech",
                    message: None,
                },
            );
        }
        VoiceEvent::SpeechEnded => {
            let _ = events.emit(
                EVENT_STATE,
                StateEvent {
                    state: "transcribing",
                    message: None,
                },
            );
        }
        VoiceEvent::WakeWord => {
            // В этот момент и укладывается бюджет ТЗ §37: интерфейс отзывается
            // на своё имя, пока человек ещё говорит фразу.
            let _ = events.emit(
                EVENT_STATE,
                StateEvent {
                    state: "addressed",
                    message: None,
                },
            );
        }
        VoiceEvent::Transcribed { .. } | VoiceEvent::Error(_) => {}
    })
    .map_err(err)?;

    let utterances = session
        .take_utterances()
        .ok_or_else(|| "очередь речи уже занята".to_string())?;

    let http = state.http.clone();
    let worker = app.clone();
    let wake_word = mode == ListenMode::WakeWord;

    // Распознавание блокирует поток и ходит в сеть — ему нужен собственный
    // поток, а не аудиопоток и не поток UI.
    std::thread::Builder::new()
        .name("yuki-stt".into())
        .spawn(move || {
            let stt = HttpStt::new(config.base_url, config.api_key, config.model, http);

            // Канал закрывается вместе с сессией — это и есть условие выхода.
            while let Ok(utterance) = utterances.recv() {
                // С записанным обращением фраза без обращения не уходит на
                // распознавание вовсе: это и деньги, и приватность — фон комнаты
                // незачем отправлять в чужой сервис.
                if fast_wake && !utterance.addressed {
                    let _ = worker.emit(
                        EVENT_STATE,
                        StateEvent {
                            state: "listening",
                            message: None,
                        },
                    );
                    continue;
                }

                let language = config.language.as_deref();
                let result = tauri::async_runtime::block_on(
                    stt.transcribe(&utterance.samples, language),
                );

                match result {
                    Ok(text) if text.trim().is_empty() => {
                        // Тишина, принятая за речь: молча возвращаемся слушать.
                        let _ = worker.emit(
                            EVENT_STATE,
                            StateEvent {
                                state: "listening",
                                message: None,
                            },
                        );
                    }
                    Ok(text) => {
                        // В режиме слова пробуждения командой считается только
                        // то, что адресовано Yuki; остальное — фон комнаты.
                        let (payload, addressed) = if fast_wake {
                            // Кто кому адресовал, решил детектор по звуку. Из текста
                            // само обращение всё равно убираем: в команде слово «Юки» лишнее.
                            (
                                strip_wake_phrase(&text).unwrap_or_else(|| text.clone()),
                                true,
                            )
                        } else if wake_word {
                            match strip_wake_phrase(&text) {
                                Some(command) => (command, true),
                                None => (text.clone(), false),
                            }
                        } else {
                            (text.clone(), true)
                        };

                        let _ = worker.emit(
                            EVENT_TEXT,
                            TranscribedEvent {
                                text: payload,
                                addressed,
                            },
                        );
                    }
                    Err(error) => {
                        let _ = worker.emit(
                            EVENT_STATE,
                            StateEvent {
                                state: "error",
                                message: Some(error.to_string()),
                            },
                        );
                    }
                }
            }
        })
        .map_err(err)?;

    *guard = Some(session);

    let _ = app.emit(
        EVENT_STATE,
        StateEvent {
            state: "listening",
            message: None,
        },
    );
    Ok(())
}

#[tauri::command]
pub fn voice_stop(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state
        .voice
        .session
        .lock()
        .map_err(|_| "состояние голоса повреждено".to_string())?;

    match guard.take() {
        Some(session) => {
            session.stop().map_err(err)?;
            let _ = app.emit(
                EVENT_STATE,
                StateEvent {
                    state: "idle",
                    message: None,
                },
            );
            Ok(())
        }
        None => Ok(()),
    }
}

/// Отпущена кнопка push-to-talk: отдать накопленное, не дожидаясь паузы.
#[tauri::command]
pub fn voice_finish_utterance(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .voice
        .session
        .lock()
        .map_err(|_| "состояние голоса повреждено".to_string())?;

    if let Some(session) = guard.as_ref() {
        session.finish_utterance();
    }
    Ok(())
}

#[tauri::command]
pub fn voice_speak(state: State<'_, AppState>, text: String) -> Result<(), String> {
    state.voice.with_tts(|tts| tts.speak(&text).map_err(err))
}

/// Замолчать немедленно — перебивание из ТЗ §10.
#[tauri::command]
pub fn voice_stop_speaking(state: State<'_, AppState>) -> Result<(), String> {
    state.voice.with_tts(|tts| tts.stop().map_err(err))
}

#[tauri::command]
pub fn voice_set_voice(state: State<'_, AppState>, name: String) -> Result<(), String> {
    state.voice.with_tts(|tts| tts.set_voice(&name).map_err(err))?;
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('voice.tts.voice', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                [&name],
            )
        })
        .map(|_| ())
        .map_err(err)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttSettings {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub language: Option<String>,
}

#[tauri::command]
pub fn voice_configure_stt(
    state: State<'_, AppState>,
    settings: SttSettings,
) -> Result<(), String> {
    let pairs = [
        ("voice.stt.provider", settings.provider_id),
        ("voice.stt.model", settings.model),
        ("voice.language", settings.language),
    ];

    state
        .storage
        .with_conn(|conn| {
            for (key, value) in pairs {
                // Пустое значение означает «вернуть умолчание», а не «записать пустоту».
                match value.filter(|v| !v.trim().is_empty()) {
                    Some(value) => conn.execute(
                        "INSERT INTO settings (key, value) VALUES (?1, ?2)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                        rusqlite::params![key, value.trim()],
                    )?,
                    None => conn.execute("DELETE FROM settings WHERE key = ?1", [key])?,
                };
            }
            Ok(())
        })
        .map_err(err)
}

/// Останавливает голосовой режим при закрытии приложения.
pub fn shutdown(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Ok(mut guard) = state.voice.session.lock() {
        if let Some(session) = guard.take() {
            let _ = session.stop();
        }
    }
    let _ = state.voice.with_tts(|tts| tts.stop().map_err(err));
}

// ── Слово пробуждения (ТЗ §37) ──────────────────────────────────────────────────

/// Ключ настройки с образцами слова пробуждения.
const SETTING_WAKE: &str = "voice.wake.model";

/// Сколько писать один образец.
const ENROLL_SECONDS: u64 = 2;

/// Состояние записи образцов для интерфейса.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeStatus {
    /// Записано ли обращение.
    pub enrolled: bool,
    /// Сколько образцов нужно всего.
    pub needed: usize,
    /// Сколько уже записано в текущем заходе.
    pub recorded: usize,
    /// Порог узнавания; `null`, если модели нет.
    pub threshold: Option<f32>,
}

/// Читает сохранённую модель.
///
/// Повреждённая настройка — это «модели нет», а не отказ запускать голос:
/// формат мог поменяться между версиями, и терять из-за этого голосовой режим
/// незачем.
fn wake_model(state: &AppState) -> Option<WakeModel> {
    let raw = setting(state, SETTING_WAKE)?;
    serde_json::from_str(&raw).ok()
}

#[tauri::command]
pub fn wake_status(state: State<'_, AppState>) -> WakeStatus {
    let model = wake_model(&state);

    WakeStatus {
        enrolled: model.is_some(),
        needed: ENROLL_SAMPLES,
        recorded: state
            .voice
            .enrollment
            .lock()
            .map(|samples| samples.len())
            .unwrap_or(0),
        threshold: model.map(|model| model.threshold),
    }
}

/// Записывает один образец обращения.
///
/// Пишет ровно две секунды и возвращает состояние: человек говорит «Юки» и
/// видит, что образец принят. Молчаливая запись не даёт понять, услышал ли
/// микрофон вообще что-нибудь.
#[tauri::command]
pub fn wake_enroll_record(state: State<'_, AppState>) -> Result<WakeStatus, String> {
    ensure_microphone_allowed(&state)?;

    if state.voice.session.lock().map(|s| s.is_some()).unwrap_or(false) {
        // Один микрофон нельзя слушать дважды: пока идёт голосовой режим,
        // запись образца получила бы пустой поток.
        return Err("сначала выключите голосовой режим".into());
    }

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
    let sink = std::sync::Arc::clone(&buffer);

    let handle = yuki_voice::capture::start(move |frame| {
        if let Ok(mut buffer) = sink.lock() {
            buffer.extend_from_slice(frame);
        }
    })
    .map_err(err)?;

    std::thread::sleep(std::time::Duration::from_secs(ENROLL_SECONDS));
    handle.stop();

    let audio = buffer
        .lock()
        .map(|buffer| buffer.clone())
        .unwrap_or_default();

    let loudness = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len().max(1) as f32).sqrt();
    if loudness < 0.005 {
        // Тихий образец испортит модель: порог посчитается по тишине, и
        // детектор начнёт срабатывать на что угодно.
        return Err("микрофон ничего не услышал — скажите «Юки» ближе к микрофону".into());
    }

    state
        .voice
        .enrollment
        .lock()
        .map_err(|_| "состояние записи занято".to_string())?
        .push(audio);

    Ok(wake_status(state))
}

/// Собирает модель из записанных образцов и сохраняет её.
#[tauri::command]
pub fn wake_enroll_finish(state: State<'_, AppState>) -> Result<WakeStatus, String> {
    let samples = {
        let mut enrollment = state
            .voice
            .enrollment
            .lock()
            .map_err(|_| "состояние записи занято".to_string())?;
        std::mem::take(&mut *enrollment)
    };

    if samples.len() < ENROLL_SAMPLES {
        return Err(format!(
            "нужно {ENROLL_SAMPLES} образца, записано {}",
            samples.len()
        ));
    }

    let model = WakeModel::from_samples(&samples)
        .ok_or("образцы не годятся: слишком короткие или тихие")?;

    let json = serde_json::to_string(&model).map_err(err)?;
    set_setting(&state, SETTING_WAKE, &json)?;

    Ok(wake_status(state))
}

/// Забывает записанное обращение.
#[tauri::command]
pub fn wake_forget(state: State<'_, AppState>) -> Result<WakeStatus, String> {
    if let Ok(mut enrollment) = state.voice.enrollment.lock() {
        enrollment.clear();
    }

    state
        .storage
        .with_conn(|conn| conn.execute("DELETE FROM settings WHERE key = ?1", [SETTING_WAKE]))
        .map_err(err)?;

    Ok(wake_status(state))
}

/// Записывает настройку.
fn set_setting(state: &AppState, key: &str, value: &str) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![key, value],
            )
        })
        .map(|_| ())
        .map_err(err)
}
