//! Голосовая сессия: микрофон → VAD → STT (ТЗ §10).
//!
//! Здесь склеиваются части конвейера и живёт единственное состояние: говорит
//! сейчас человек или нет. Дальше по цепочке (агент → TTS → динамик) сессия не
//! идёт намеренно — это уже оркестрация, и она в приложении.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::capture::{self, CaptureHandle, FRAME_MS, TARGET_RATE};
use crate::error::{VoiceError, VoiceResult};
use crate::vad::{rms, EnergyVad, SpeechDetector, VadConfig, VadEvent};

/// Максимальная длина одной фразы.
///
/// Ограничение защищает не от болтливости, а от залипшего VAD: если детектор
/// по какой-то причине не увидит конца, без потолка буфер будет расти, пока не
/// съест память, и в распознавание уйдёт получасовая запись.
const MAX_UTTERANCE_SECONDS: usize = 30;
const MAX_SAMPLES: usize = TARGET_RATE as usize * MAX_UTTERANCE_SECONDS;

/// Сколько звука перед началом речи попадает в запись.
///
/// Детектор объявляет речь на первом кадре, который уже громче порога, а порог
/// вчетверо выше фона комнаты. Начало слова нарастает тише порога, и без
/// упреждающего буфера оно просто не попадает в запись: распознавание получает
/// фразу без первого звука и выдаёт похожую бессмыслицу вместо команды.
///
/// Триста миллисекунд с запасом покрывают нарастание любого слога и стоят
/// десять килобайт памяти.
const PREROLL_MS: usize = 300;
const PREROLL_FRAMES: usize = PREROLL_MS / FRAME_MS as usize;

/// Как слушать (ТЗ §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenMode {
    /// Пользователь держит кнопку или нажал «говорить» — слушаем до его команды.
    PushToTalk,
    /// Постоянное прослушивание со словом пробуждения.
    WakeWord,
}

/// Что происходит в сессии.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// Текущая громкость 0…1 — по ней Orb дышит в такт голосу (ТЗ §13).
    Level(f32),
    SpeechStarted,
    /// Фраза закончилась, идёт распознавание.
    SpeechEnded,
    /// Услышано обращение (ТЗ §37). Приходит в момент произнесения слова,
    /// а не после конца фразы — на этом и держится бюджет 300 мс.
    WakeWord,
    /// Распознанный текст.
    ///
    /// `addressed` показывает, обращались ли к Yuki: в режиме слова пробуждения
    /// фразы без обращения приходят сюда же, но с `false`. Решение, что с ними
    /// делать, принимает приложение, а не конвейер.
    Transcribed {
        text: String,
        addressed: bool,
    },
    Error(String),
}

/// Сегмент речи, ушедший на распознавание.
pub struct Utterance {
    pub samples: Vec<f32>,
    pub started_at: std::time::Instant,
    pub ended_at: std::time::Instant,
    /// Обращались ли к Yuki.
    ///
    /// В режиме слова пробуждения с записанными образцами фраза без обращения
    /// не уходит на распознавание вовсе: это и деньги, и приватность — фон
    /// комнаты незачем отправлять в чужой сервис.
    pub addressed: bool,
    /// Записано, пока говорила сама Yuki.
    ///
    /// Отмечается в момент захвата, а не при распознавании: к моменту, когда до
    /// записи дойдёт очередь, синтезатор уже замолчит, и проверка «говорит ли
    /// он сейчас» ничего не поймает. Именно так Yuki слышала собственный ответ
    /// и отвечала сама себе.
    pub self_voice: bool,
}

/// Варианты обращения к Yuki (ТЗ §10).
const WAKE_PHRASES: &[&str] = &[
    "hey yuki",
    "hey youki",
    "эй юки",
    "хей юки",
    "yuki",
    "юки",
    "юкки",
    "юке",
];

/// Убирает слово пробуждения и возвращает саму команду.
///
/// `None` означает, что к Yuki не обращались. Отдельно разбирается случай, когда
/// после обращения ничего нет: «Юки» — это обращение, на которое нужно отозваться,
/// а не пустая команда, поэтому возвращается пустая строка, а не `None`.
pub fn strip_wake_phrase(text: &str) -> Option<String> {
    let text = text.trim().trim_start_matches(['«', '"', '\'']);
    for phrase in WAKE_PHRASES {
        let Some(prefix) = text.get(..phrase.len()) else {
            continue;
        };
        if prefix.to_lowercase() != *phrase {
            continue;
        }
        let rest = &text[phrase.len()..];
        if rest
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        return Some(
            rest.trim_start_matches(|c: char| {
                c.is_whitespace()
                    || [',', '.', ':', '!', '?', '—', '-', '»', '"', '\''].contains(&c)
            })
            .trim()
            .to_owned(),
        );
    }
    None
}

/// Allows exactly one follow-up after a standalone wake word. Uses capture time,
/// so slow transcription cannot expire a command that was spoken in time.
#[derive(Default)]
pub struct WakeTextGate {
    until: Option<std::time::Instant>,
}

impl WakeTextGate {
    pub fn route(
        &mut self,
        text: &str,
        start: std::time::Instant,
        end: std::time::Instant,
    ) -> (String, bool) {
        if let Some(command) = strip_wake_phrase(text) {
            self.until = command.is_empty().then_some(end + ADDRESS_WINDOW);
            return (command, true);
        }
        if text.trim().is_empty() {
            return (String::new(), false);
        }
        let addressed = self.until.take().is_some_and(|until| start <= until);
        (text.to_owned(), addressed)
    }
}

/// Окно продолжения разговора.
///
/// Без него разговор рвался на каждом ответе: окно после «Юки» отсчитывается
/// от обращения, а ответ модели приходит через несколько секунд — к моменту,
/// когда Yuki договорила, окно уже закрыто, и на «а завтра?» она молчала, пока
/// её снова не позовут по имени.
///
/// Открывает его приложение — когда Yuki закончила говорить. Хранится как
/// срок в миллисекундах от эпохи Unix в атомарной ячейке: читает её
/// аудиопоток на каждой фразе, и ждать замок там нельзя.
#[derive(Clone, Default)]
pub struct ConversationWindow(Arc<AtomicU64>);

impl ConversationWindow {
    /// Сколько ждать продолжения после ответа.
    ///
    /// Семь секунд: хватает, чтобы подумать и задать уточнение, и мало, чтобы
    /// Yuki приняла на свой счёт разговор, начавшийся после.
    pub const FOLLOW_UP: std::time::Duration = std::time::Duration::from_secs(7);

    /// Открывает окно на `window` от текущего момента.
    pub fn open(&self, window: std::time::Duration) {
        self.0
            .store(unix_ms() + window.as_millis() as u64, Ordering::Relaxed);
    }

    /// Закрывает окно: человек сказал «стоп» или голосовой режим выключен.
    pub fn close(&self) {
        self.0.store(0, Ordering::Relaxed);
    }

    /// Началась ли фраза, пока окно было открыто.
    ///
    /// Сравнивается момент начала фразы, а не распознавания: медленное
    /// распознавание не должно закрыть окно для вовремя сказанного.
    pub fn covers(&self, started_at: std::time::Instant) -> bool {
        let until = self.0.load(Ordering::Relaxed);
        if until == 0 {
            return false;
        }
        let ago = started_at.elapsed().as_millis() as u64;
        unix_ms().saturating_sub(ago) <= until
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Работающая голосовая сессия.
/// Сколько времени после обращения фразы считаются адресованными.
///
/// Восемь секунд: человек говорит «Юки» и продолжает мысль, иногда с паузой на
/// раздумье. Короче — ассистент перестаёт слышать собственное имя; дольше —
/// начинает подслушивать разговор, случившийся после.
const ADDRESS_WINDOW: std::time::Duration = std::time::Duration::from_secs(8);

pub struct VoiceSession {
    mode: ListenMode,
    capture: Option<CaptureHandle>,
    /// Сигнал «хватит слушать» для push-to-talk.
    finish: Arc<AtomicBool>,
    utterances: Option<Receiver<Utterance>>,
}

impl VoiceSession {
    /// Запускает прослушивание.
    ///
    /// `on_event` вызывается из аудиопотока: он должен быть быстрым. Готовые
    /// сегменты речи забираются через [`VoiceSession::utterances`] — распознавание
    /// асинхронно и в аудиопотоке ему делать нечего.
    /// `wake` — образцы слова пробуждения. Без них режим слова пробуждения
    /// работает по прежнему пути: фраза распознаётся целиком, и обращение
    /// ищется в тексте. Это медленнее бюджета ТЗ §37, но не требует настройки.
    /// `speaking` — общий признак «динамик занят собственной речью». Читается
    /// из аудиопотока на каждом кадре, поэтому это атомарный флаг, а не замок.
    /// `follow_up` — окно продолжения разговора после ответа Yuki.
    pub fn start<F>(
        mode: ListenMode,
        wake: Option<crate::wake::WakeModel>,
        speaking: Arc<AtomicBool>,
        follow_up: ConversationWindow,
        mut on_event: F,
    ) -> VoiceResult<Self>
    where
        F: FnMut(VoiceEvent) + Send + 'static,
    {
        let (tx, rx): (Sender<Utterance>, Receiver<Utterance>) = mpsc::channel();
        let finish = Arc::new(AtomicBool::new(false));
        let finish_in_stream = finish.clone();

        let mut vad = EnergyVad::new(VadConfig {
            frame_ms: FRAME_MS,
            ..VadConfig::default()
        });
        let mut buffer: Vec<f32> = Vec::with_capacity(TARGET_RATE as usize * 3);
        // Кольцо последних кадров: что бы ни случилось дальше, начало фразы уже
        // сохранено.
        let mut preroll: std::collections::VecDeque<Vec<f32>> =
            std::collections::VecDeque::with_capacity(PREROLL_FRAMES + 1);

        // Детектор работает только в режиме слова пробуждения: в push-to-talk
        // обращение и есть нажатая кнопка.
        let mut detector = match (mode, wake) {
            // `None` здесь — это «слово записано, но детектор из него не
            // собрался»: тогда режим работает прежним путём, по тексту.
            (ListenMode::WakeWord, Some(model)) => crate::wake::WakeDetector::new(model),
            _ => None,
        };
        let mut addressed_until: Option<std::time::Instant> = None;

        let mut started_at = std::time::Instant::now();
        let mut utterance_addressed = false;
        // Любой кадр фразы, пришедшийся на собственную речь, помечает её целиком.
        let mut heard_self = false;

        let capture = capture::start(move |frame| {
            on_event(VoiceEvent::Level(level_of(frame)));
            let talking = speaking.load(Ordering::Relaxed);

            if let Some(detector) = detector.as_mut() {
                if detector.push(frame) {
                    // Отзыв в момент произнесения, до конца фразы.
                    addressed_until = Some(std::time::Instant::now() + ADDRESS_WINDOW);
                    utterance_addressed = true;
                    on_event(VoiceEvent::WakeWord);
                }
            }

            let forced = finish_in_stream.swap(false, Ordering::Relaxed);
            let event = vad.push_frame(frame);

            // Кольцо пополняется всегда: к моменту, когда детектор объявит
            // речь, начало слова уже должно быть в нём.
            if preroll.len() == PREROLL_FRAMES {
                preroll.pop_front();
            }
            preroll.push_back(frame.to_vec());

            match event {
                VadEvent::SpeechStart => {
                    // Фраза началась раньше, чем детектор это понял: отсчёт
                    // ведём от первого кадра упреждающего буфера.
                    started_at = std::time::Instant::now()
                        - std::time::Duration::from_millis(
                            (preroll.len() * FRAME_MS as usize) as u64,
                        );
                    utterance_addressed = detector.is_none()
                        || addressed_until.is_some_and(|until| started_at <= until)
                        || follow_up.covers(started_at);
                    buffer.clear();
                    for earlier in &preroll {
                        buffer.extend_from_slice(earlier);
                    }
                    buffer.extend_from_slice(frame);
                    heard_self = talking;
                    on_event(VoiceEvent::SpeechStarted);
                }
                VadEvent::Speech => {
                    heard_self |= talking;
                    if buffer.len() < MAX_SAMPLES {
                        buffer.extend_from_slice(frame);
                    }
                }
                VadEvent::SpeechEnd => {
                    on_event(VoiceEvent::SpeechEnded);

                    // Без детектора адресованность решается позже, по тексту;
                    // с детектором — здесь, и неадресованное дальше не идёт.
                    let addressed = utterance_addressed;

                    if tx
                        .send(Utterance {
                            samples: std::mem::take(&mut buffer),
                            addressed,
                            self_voice: std::mem::take(&mut heard_self),
                            started_at,
                            ended_at: std::time::Instant::now(),
                        })
                        .is_err()
                    {
                        // Приёмник уничтожен — сессия закрывается.
                        return;
                    }
                }
                VadEvent::Silence => {}
            }

            // Push-to-talk отпускают в любой момент, в том числе посреди слова:
            // отдаём накопленное, не дожидаясь паузы.
            if forced && !buffer.is_empty() {
                vad.reset();
                on_event(VoiceEvent::SpeechEnded);
                let _ = tx.send(Utterance {
                    samples: std::mem::take(&mut buffer),
                    // Кнопку нажал человек — это и есть обращение.
                    addressed: true,
                    self_voice: std::mem::take(&mut heard_self),
                    started_at,
                    ended_at: std::time::Instant::now(),
                });
            }
        })?;

        Ok(Self {
            mode,
            capture: Some(capture),
            finish,
            utterances: Some(rx),
        })
    }

    pub fn mode(&self) -> ListenMode {
        self.mode
    }

    /// Просит отдать накопленное прямо сейчас — для отпущенной кнопки push-to-talk.
    pub fn finish_utterance(&self) {
        self.finish.store(true, Ordering::Relaxed);
    }

    /// Забирает очередь готовых сегментов речи.
    ///
    /// Отдаётся один раз и во владение: распознавание блокирует поток, и держать
    /// его рядом с сессией — значит держать блокирующий вызов там, где живёт
    /// состояние UI. Второй вызов вернёт `None`.
    pub fn take_utterances(&mut self) -> Option<Receiver<Utterance>> {
        self.utterances.take()
    }

    /// Останавливает захват.
    pub fn stop(mut self) -> VoiceResult<()> {
        match self.capture.take() {
            Some(capture) => {
                capture.stop();
                Ok(())
            }
            None => Err(VoiceError::NotRunning),
        }
    }
}

/// Приводит громкость кадра к 0…1 для анимации Orb.
///
/// Масштаб подобран под речь: RMS около 0.2 — это уже уверенный разговорный
/// уровень, и дальше расти анимации некуда.
fn level_of(frame: &[f32]) -> f32 {
    (rms(frame) / 0.2).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_prefix_preserves_unicode_and_punctuation() {
        assert_eq!(
            strip_wake_phrase("  Юки! Открой Chrome?  ").as_deref(),
            Some("Открой Chrome?")
        );
        assert_eq!(strip_wake_phrase("Юки, İ ❤️?").as_deref(), Some("İ ❤️?"));
        assert_eq!(strip_wake_phrase("🌸🌸🌸"), None);
        assert_eq!(strip_wake_phrase("«Юки?»").as_deref(), Some(""));
    }

    #[test]
    fn standalone_wake_accepts_one_followup_using_capture_time() {
        let mut gate = WakeTextGate::default();
        let now = std::time::Instant::now();
        let seconds = std::time::Duration::from_secs;
        assert_eq!(gate.route("Юки", now, now), (String::new(), true));
        // Starts inside the window, ends much later; STT latency is irrelevant.
        assert!(
            gate.route("Напомни мне завтра", now + seconds(4), now + seconds(25))
                .1
        );
        assert!(
            !gate
                .route("Фоновый разговор", now + seconds(5), now + seconds(26))
                .1
        );
    }

    #[test]
    fn expired_wake_and_completed_command_do_not_accept_background_speech() {
        let mut gate = WakeTextGate::default();
        let now = std::time::Instant::now();
        gate.route("Юки", now, now);
        assert!(
            !gate
                .route("Фон", now + std::time::Duration::from_secs(9), now)
                .1
        );
        assert_eq!(
            gate.route("Юки, открой Chrome", now, now),
            ("открой Chrome".into(), true)
        );
        assert!(!gate.route("Фон", now, now).1);
    }

    #[test]
    fn recognises_the_wake_phrase_and_returns_the_command() {
        assert_eq!(
            strip_wake_phrase("Юки, открой браузер").as_deref(),
            Some("открой браузер")
        );
        assert_eq!(
            strip_wake_phrase("Hey Yuki what is the time").as_deref(),
            Some("what is the time")
        );
    }

    #[test]
    fn a_bare_address_is_still_an_address() {
        // «Юки?» — это оклик, на который надо отозваться, а не тишина.
        assert_eq!(strip_wake_phrase("Юки?").as_deref(), Some(""));
    }

    #[test]
    fn ignores_speech_that_is_not_addressed_to_yuki() {
        assert_eq!(strip_wake_phrase("открой браузер"), None);
        assert_eq!(strip_wake_phrase("надо бы позвонить"), None);
    }

    #[test]
    fn does_not_trigger_on_a_word_that_merely_starts_the_same() {
        assert_eq!(strip_wake_phrase("юкитерий это порода"), None);
    }

    #[test]
    fn preserves_case_of_the_command_itself() {
        // Регистр значим: модель получит текст как есть.
        assert_eq!(
            strip_wake_phrase("Юки, открой Chrome").as_deref(),
            Some("открой Chrome")
        );
    }

    #[test]
    fn level_saturates_instead_of_exceeding_one() {
        assert_eq!(level_of(&[1.0; 320]), 1.0);
        assert_eq!(level_of(&[0.0; 320]), 0.0);
        assert!((level_of(&[0.1; 320]) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn the_preroll_covers_the_attack_of_a_word() {
        // Детектор объявляет речь на кадре, который уже громче порога, а порог
        // вчетверо выше фона. Без запаса перед этим кадром распознавание
        // получает фразу без первого звука.
        assert_eq!(PREROLL_FRAMES, 15);
        assert_eq!(PREROLL_FRAMES * FRAME_MS as usize, PREROLL_MS);
        // Запас должен покрывать нарастание слога, но не тянуть в запись
        // полсекунды фона до неё.
        assert!((200..=400).contains(&PREROLL_MS));
    }

    #[test]
    fn utterance_cap_matches_thirty_seconds() {
        assert_eq!(MAX_SAMPLES, 16_000 * 30);
    }
}
