//! Команды голосового режима (ТЗ §10).
//!
//! Конвейер живёт в крейте `yuki-voice`, здесь — только его подключение к
//! приложению: разрешение на микрофон (ТЗ §21), настройки распознавания и
//! события для интерфейса.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use yuki_voice::{
    strip_wake_phrase, HttpStt, HttpTts, HttpTtsConfig, ListenMode, SpeechToText, SystemTts,
    TextToSpeech, VoiceEvent, VoiceSession, WakeModel, ENROLL_SAMPLES,
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
    /// Чем говорит: `system` или `http`.
    pub engine: String,
    /// Отдаёт ли движок громкость — от этого зависит, настоящий ли lip-sync.
    pub has_level: bool,
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
        return Err("нет системного разрешения на микрофон — выдайте его в настройках ОС".into());
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
    if let Some(url) = setting(state, "voice.stt.url").filter(|u| !u.trim().is_empty()) {
        validate_stt_url(&url)?;
        crate::privacy::ensure_allowed(&state.storage, &url, "распознавание речи")?;
        let api_key = crate::secrets::get(&stt_secret_ref(&url)?).map_err(err)?;
        if !crate::privacy::is_local_url(&url) && api_key.is_none() {
            return Err("Добавьте ключ сервиса распознавания речи в настройках голоса.".into());
        }
        return Ok(SttConfig {
            base_url: url,
            api_key,
            model: setting(state, "voice.stt.model").unwrap_or_else(|| "whisper-1".into()),
            language: setting(state, "voice.language").or_else(|| Some("ru".into())),
        });
    }
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
                     WHERE enabled = 1 AND kind IN ('openai', 'custom', 'openai_compatible')
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
        "Распознавание речи не настроено. Откройте настройки голоса и подключите \
         OpenAI, Groq или локальный сервер распознавания."
            .to_string()
    })?;

    if !matches!(kind.as_str(), "openai" | "custom" | "openai_compatible") {
        return Err(format!(
            "у провайдера «{kind}» нет распознавания речи — выберите другой в настройках голоса"
        ));
    }

    // Распознавание увозит звук голоса целиком — в режиме Local Only (ТЗ §29) это
    // точно такая же утечка, как и отправка реплики в облачную модель.
    crate::privacy::ensure_allowed(&state.storage, &base_url, "распознавание речи")?;

    let api_key = secret_ref
        .as_deref()
        .and_then(|r| crate::secrets::get(r).ok().flatten());
    if yuki_ai::requires_key(&kind) && api_key.is_none() {
        return Err("Не задан ключ OpenAI для распознавания речи".into());
    }
    Ok(SttConfig {
        base_url,
        api_key,
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
    tts: Mutex<Option<TtsEngine>>,
    /// Занят ли динамик собственной речью Yuki.
    ///
    /// Флаг поднимается на всё время озвучивания и опускается с запасом после
    /// него. Читает его аудиопоток на каждом кадре, поэтому это атомарный
    /// признак, а не замок: ждать чужую блокировку в захвате звука нельзя.
    speaking: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Окно продолжения разговора: после ответа Yuki слушает уточнение без
    /// повторного «Юки».
    follow_up: yuki_voice::ConversationWindow,
}

/// Чем говорить.
///
/// Перечисление, а не `Box<dyn TextToSpeech>`, потому что у сервиса есть то,
/// чего нет у системного движка: громкость звука прямо сейчас. Через типаж её
/// пришлось бы или добавлять всем, или достукиваться приведением типа — и то,
/// и другое хуже двух явных вариантов.
enum TtsEngine {
    /// Синтез средствами ОС: работает всегда, звука не отдаёт.
    System(SystemTts),
    /// Чужой сервис по HTTP: отдаёт WAV, поэтому у нас есть громкость.
    Http(HttpTts),
}

impl TtsEngine {
    fn as_speech(&self) -> &dyn TextToSpeech {
        match self {
            Self::System(engine) => engine,
            Self::Http(engine) => engine,
        }
    }

    /// Громкость речи 0…1; у системного синтеза её нет.
    fn level(&self) -> Option<f32> {
        match self {
            Self::System(engine) => engine.level(),
            Self::Http(engine) => Some(engine.level()),
        }
    }
}

/// Ключи настроек синтеза.
const SETTING_TTS_ENGINE: &str = "voice.tts.engine";
const SETTING_TTS_URL: &str = "voice.tts.url";
const SETTING_TTS_DIR: &str = "voice.tts.samples";
const SETTING_TTS_SAMPLE: &str = "voice.tts.sample";
const SETTING_TTS_PROMPT: &str = "voice.tts.prompt";
const SETTING_TTS_LANG: &str = "voice.tts.lang";

impl VoiceState {
    pub(crate) fn reset_tts(&self) {
        if let Ok(mut guard) = self.tts.lock() {
            if let Some(engine) = guard.take() {
                let _ = engine.as_speech().stop();
            }
        }
    }
    /// Создаёт синтезатор при первом обращении.
    ///
    /// Лениво, потому что движок ОС инициализируется небыстро, а пользователь
    /// может вообще не включать голос — платить за это стартом приложения
    /// (бюджет ТЗ §37) незачем.
    fn with_tts<T>(
        &self,
        state: &AppState,
        f: impl FnOnce(&TtsEngine) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .tts
            .lock()
            .map_err(|_| "состояние синтезатора повреждено".to_string())?;

        let wanted_http = setting(state, SETTING_TTS_ENGINE).as_deref() == Some("http");

        // Движок пересоздаётся при смене выбора: менять его на месте значило бы
        // держать внутри оба и решать, кто из них говорит, на каждом вызове.
        let mismatch = match guard.as_ref() {
            Some(TtsEngine::Http(_)) => !wanted_http,
            Some(TtsEngine::System(_)) => wanted_http,
            None => true,
        };

        if mismatch {
            // Прежний движок обязан замолчать: иначе две фразы наложатся.
            if let Some(previous) = guard.as_ref() {
                let _ = previous.as_speech().stop();
            }

            *guard = Some(if wanted_http {
                TtsEngine::Http(HttpTts::new(http_tts_config(state)))
            } else {
                let engine = SystemTts::new().map_err(err)?;
                if let Some(name) = setting(state, "voice.tts.voice") {
                    // A removed OS voice should fall back to the system default.
                    let _ = engine.set_voice(&name);
                }
                TtsEngine::System(engine)
            });
        } else if wanted_http {
            // Настройки сервиса могли измениться, пока движок уже жил.
            if let Some(TtsEngine::Http(engine)) = guard.as_ref() {
                engine.configure(http_tts_config(state));
            }
        }

        f(guard.as_ref().expect("синтезатор только что создан"))
    }
}

/// Настройки сервиса синтеза из базы.
fn http_tts_config(state: &AppState) -> HttpTtsConfig {
    let language = setting(state, SETTING_TTS_LANG).unwrap_or_else(|| "ru".into());

    HttpTtsConfig {
        base_url: setting(state, SETTING_TTS_URL)
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:9880".into()),
        reference_dir: setting(state, SETTING_TTS_DIR).unwrap_or_default(),
        reference: setting(state, SETTING_TTS_SAMPLE).unwrap_or_default(),
        prompt_text: setting(state, SETTING_TTS_PROMPT).unwrap_or_default(),
        text_lang: language.clone(),
        prompt_lang: language,
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
        speaking: state
            .voice
            .with_tts(&state, |t| Ok(t.as_speech().is_speaking()))
            .unwrap_or(false),
        input_device: yuki_voice::default_input_name(),
        devices: yuki_voice::input_devices(),
        voices: state
            .voice
            .with_tts(&state, |t| Ok(t.as_speech().voices()))
            .unwrap_or_default(),
        stt_ready: stt_config(&state).is_ok(),
        engine: if setting(&state, SETTING_TTS_ENGINE).as_deref() == Some("http") {
            "http".into()
        } else {
            "system".into()
        },
        has_level: state
            .voice
            .with_tts(&state, |t| Ok(t.level().is_some()))
            .unwrap_or(false),
    })
}

/// Имя микрофона по умолчанию.
///
/// Отдельной лёгкой командой: интерфейс спрашивает её раз в несколько секунд,
/// чтобы заметить подключённые наушники. Полный статус для этого слишком тяжёл
/// — он перечисляет все устройства и голоса.
#[tauri::command]
pub fn voice_input_name() -> Option<String> {
    yuki_voice::default_input_name()
}

/// Громкость речи прямо сейчас, 0…1.
///
/// Отдельной командой, а не полем статуса: её спрашивают десятки раз в секунду,
/// чтобы рот аватара шёл за звуком, а статус попутно читает настройки из базы —
/// шестнадцать запросов в секунду ради одного числа.
///
/// `None` означает «движок звука не отдаёт» — у системного синтеза буфера нет,
/// и рот в этом случае работает по ритму слогов.
#[tauri::command]
pub fn voice_speaking_level(state: State<'_, AppState>) -> Option<f32> {
    state.voice.tts.lock().ok()?.as_ref()?.level()
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
    if let Some(running) = guard.as_ref() {
        // Тот же режим уже слушает — это не ошибка: так бывает, когда окно
        // перезагрузилось и интерфейс заново включает голос. Раньше человек
        // видел «голосовой режим уже запущен», хотя всё работало.
        if running.mode() == mode {
            return Ok(());
        }
        return Err("голосовой режим уже запущен в другом режиме".into());
    }

    let events = app.clone();
    // Образцы обращения читаются при старте сессии: их могли записать только что.
    let wake = wake_model(&state);
    let fast_wake = mode == ListenMode::WakeWord && wake.is_some();

    let mut session =
        VoiceSession::start(
            mode,
            wake,
            state.voice.speaking.clone(),
            state.voice.follow_up.clone(),
            move |event| match event {
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
            },
        )
        .map_err(err)?;

    let utterances = session
        .take_utterances()
        .ok_or_else(|| "очередь речи уже занята".to_string())?;

    let http = state.http.clone();
    let worker = app.clone();
    let wake_word = mode == ListenMode::WakeWord;
    let follow_up = state.voice.follow_up.clone();

    // Распознавание блокирует поток и ходит в сеть — ему нужен собственный
    // поток, а не аудиопоток и не поток UI.
    std::thread::Builder::new()
        .name("yuki-stt".into())
        .spawn(move || {
            let stt = HttpStt::new(config.base_url, config.api_key, config.model, http);

            // Канал закрывается вместе с сессией — это и есть условие выхода.
            let mut wake_gate = yuki_voice::session::WakeTextGate::default();
            while let Ok(utterance) = utterances.recv() {
                // Постоянно открытый микрофон слышит и саму Yuki. Её ответ,
                // распознанный обратно, — это в лучшем случае мусор в истории,
                // а в худшем команда самой себе.
                //
                // Признак ставится при захвате: проверять «говорит ли она
                // сейчас» здесь бесполезно — пока запись дойдёт до очереди,
                // синтезатор успевает замолчать, и эхо проходит насквозь.
                if wake_word && utterance.self_voice {
                    continue;
                }
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
                let result =
                    tauri::async_runtime::block_on(stt.transcribe(&utterance.samples, language));

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
                            let (command, addressed) =
                                wake_gate.route(&text, utterance.started_at, utterance.ended_at);
                            (
                                command,
                                addressed || follow_up.covers(utterance.started_at),
                            )
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

    state.voice.follow_up.close();
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

/// Сколько микрофон остаётся заглушённым после того, как Yuki договорила.
///
/// Звук ещё живёт в комнате и в буфере устройства; без запаса последний слог
/// возвращается в распознавание и Yuki отвечает сама себе.
const ECHO_TAIL: std::time::Duration = std::time::Duration::from_millis(700);

#[tauri::command]
pub async fn voice_speak(app: AppHandle, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let speaking = state.voice.speaking.clone();
        speaking.store(true, std::sync::atomic::Ordering::Relaxed);

        let result = state
            .voice
            .with_tts(&state, |tts| tts.as_speech().speak(&text).map_err(err));

        // Синтез возвращается сразу, а звук ещё идёт: ждём тишины, потом ещё
        // немного. Потолок нужен, чтобы зависший движок не оставил микрофон
        // выключенным навсегда.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let talking = state
                .voice
                .tts
                .try_lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|e| e.as_speech().is_speaking()))
                .unwrap_or(false);
            if !talking {
                break;
            }
        }
        std::thread::sleep(ECHO_TAIL);
        speaking.store(false, std::sync::atomic::Ordering::Relaxed);
        // Договорила — теперь можно ответить ей без имени. Только при постоянном
        // прослушивании: в push-to-talk обращение и так нажатая кнопка.
        let listening_for_name = state
            .voice
            .session
            .lock()
            .ok()
            .and_then(|s| s.as_ref().map(|s| s.mode() == ListenMode::WakeWord))
            .unwrap_or(false);
        if listening_for_name && result.is_ok() {
            state
                .voice
                .follow_up
                .open(yuki_voice::ConversationWindow::FOLLOW_UP);
        }
        result
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn voice_playback(state: State<'_, AppState>) -> serde_json::Value {
    let guard = state.voice.tts.try_lock().ok();
    let engine = guard.as_deref().and_then(|v| v.as_ref());
    serde_json::json!({"speaking": engine.is_some_and(|v| v.as_speech().is_speaking()), "level": engine.and_then(|v| v.level())})
}

/// Замолчать немедленно — перебивание из ТЗ §10.
#[tauri::command]
pub fn voice_stop_speaking(state: State<'_, AppState>) -> Result<(), String> {
    state
        .voice
        .with_tts(&state, |tts| tts.as_speech().stop().map_err(err))
}

#[tauri::command]
pub fn voice_set_voice(state: State<'_, AppState>, name: String) -> Result<(), String> {
    if name.is_empty() {
        state.voice.reset_tts();
    } else {
        state
            .voice
            .with_tts(&state, |tts| tts.as_speech().set_voice(&name).map_err(err))?;
    }
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
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

fn validate_stt_url(value: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Некорректный адрес распознавания речи")?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !(url.scheme() == "https"
            || (url.scheme() == "http" && crate::privacy::is_local_url(value)))
    {
        return Err(
            "Используйте HTTPS или HTTP на localhost. Ключ вводится в отдельное поле.".into(),
        );
    }
    Ok(())
}

fn stt_secret_ref(url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Некорректный адрес распознавания")?;
    Ok(format!(
        "voice:stt:{}",
        parsed.origin().ascii_serialization()
    ))
}

#[tauri::command]
pub fn voice_configure_stt(
    state: State<'_, AppState>,
    settings: SttSettings,
) -> Result<(), String> {
    if let Some(url) = &settings.base_url {
        if !url.trim().is_empty() {
            validate_stt_url(url.trim())?;
        }
    }
    if let Some(key) = settings.api_key {
        let url = settings
            .base_url
            .clone()
            .or_else(|| setting(&state, "voice.stt.url"))
            .ok_or("Сначала укажите адрес распознавания")?;
        let secret_ref = stt_secret_ref(&url)?;
        if key.is_empty() {
            crate::secrets::delete(&secret_ref).map_err(err)?;
        } else {
            crate::secrets::set(&secret_ref, key.trim()).map_err(err)?;
        }
    }
    let pairs = [
        ("voice.stt.provider", settings.provider_id),
        ("voice.stt.model", settings.model),
        ("voice.language", settings.language),
        ("voice.stt.url", settings.base_url),
    ];

    state
        .storage
        .with_conn(|conn| {
            for (key, value) in pairs {
                let Some(value) = value else { continue; };
                // Пустое значение означает «вернуть умолчание», а не «записать пустоту».
                match Some(value).filter(|v| !v.trim().is_empty()) {
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
    let _ = state
        .voice
        .with_tts(&state, |tts| tts.as_speech().stop().map_err(err));
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
    /// Слово записано прежней версией и работает старым детектором.
    ///
    /// Перезаписывать никто не обязан — оно и так работает. Но молчать об этом
    /// нельзя: человек останется на старом пути, не узнав, что новый ошибается
    /// реже, и решит, что обновление ничего не изменило.
    pub legacy: bool,
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
        legacy: model
            .as_ref()
            .is_some_and(|model| model.reference.is_none()),
        threshold: model.map(|model| model.threshold),
    }
}

/// Записывает один образец обращения.
///
/// Пишет ровно две секунды и возвращает состояние: человек говорит «Юки» и
/// видит, что образец принят. Молчаливая запись не даёт понять, услышал ли
/// микрофон вообще что-нибудь.
///
/// Голосовой режим на это время выключается и включается обратно. Раньше здесь
/// был отказ «сначала выключите голосовой режим» — а режим включается сам при
/// запуске, и человек упирался в этот отказ ровно в тот момент, когда делал
/// то, о чём его просили. Микрофон один, и разобраться с этим должна программа,
/// а не пользователь.
#[tauri::command]
pub fn wake_enroll_record(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WakeStatus, String> {
    ensure_microphone_allowed(&state)?;

    // Микрофон один: пока идёт голосовой режим, запись образца получила бы
    // пустой поток.
    let resume = {
        let mut guard = state
            .voice
            .session
            .lock()
            .map_err(|_| "состояние голоса повреждено".to_string())?;
        match guard.take() {
            Some(session) => {
                let mode = session.mode();
                session.stop().map_err(err)?;
                Some(mode)
            }
            None => None,
        }
    };

    let recorded = record_wake_sample(&state);

    // Режим возвращается при любом исходе: иначе тихий образец молча отнимал бы
    // голосовой режим до перезапуска.
    if let Some(mode) = resume {
        if let Err(error) = voice_start(app, state.clone(), mode) {
            tracing::warn!(%error, "голосовой режим не вернулся после записи образца");
        }
    }

    recorded?;
    Ok(wake_status(state))
}

/// Пишет две секунды с микрофона и кладёт их в набор образцов.
fn record_wake_sample(state: &AppState) -> Result<(), String> {
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

    Ok(())
}

/// Собирает модель из записанных образцов и сохраняет её.
///
/// Голосовой режим после этого перезапускается: образцы читаются один раз, при
/// старте сессии. Без перезапуска человек записал бы обращение и не увидел
/// никакой разницы до перезапуска приложения — и решил бы, что запись не
/// работает.
#[tauri::command]
pub fn wake_enroll_finish(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WakeStatus, String> {
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
    restart_listening(&app, &state);

    Ok(wake_status(state))
}

/// Перезапускает голосовой режим, если он идёт: чтобы сессия перечитала слово.
///
/// Молча ничего не делает, когда режим выключен, — включать его за человека
/// незачем.
fn restart_listening(app: &AppHandle, state: &State<'_, AppState>) {
    let resume = {
        let Ok(mut guard) = state.voice.session.lock() else {
            return;
        };
        match guard.take() {
            Some(session) => {
                let mode = session.mode();
                let _ = session.stop();
                Some(mode)
            }
            None => None,
        }
    };

    if let Some(mode) = resume {
        if let Err(error) = voice_start(app.clone(), state.clone(), mode) {
            tracing::warn!(%error, "голосовой режим не вернулся после записи обращения");
        }
    }
}

/// Забывает записанное обращение.
///
/// Режим тоже перезапускается: иначе сессия продолжила бы слушать по уже
/// забытому слову.
#[tauri::command]
pub fn wake_forget(app: AppHandle, state: State<'_, AppState>) -> Result<WakeStatus, String> {
    if let Ok(mut enrollment) = state.voice.enrollment.lock() {
        enrollment.clear();
    }

    state
        .storage
        .with_conn(|conn| conn.execute("DELETE FROM settings WHERE key = ?1", [SETTING_WAKE]))
        .map_err(err)?;
    restart_listening(&app, &state);

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
