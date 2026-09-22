//! Windows speech synthesis to PCM, with the meter tied to audio playback.
use crate::{TextToSpeech, VoiceError, VoiceResult};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use windows::{
    core::HSTRING, Media::SpeechSynthesis::SpeechSynthesizer, Storage::Streams::DataReader,
};

#[derive(Default)]
struct Playback {
    cancelled: Arc<AtomicBool>,
    playing: AtomicBool,
    level: Arc<AtomicU32>,
}
pub struct SystemTts {
    voice: Mutex<String>,
    rate: Mutex<f64>,
    playback: Mutex<Arc<Playback>>,
}
fn error(e: impl std::fmt::Display) -> VoiceError {
    VoiceError::Tts(e.to_string())
}
impl SystemTts {
    pub fn new() -> VoiceResult<Self> {
        SpeechSynthesizer::new().map_err(error)?;
        Ok(Self {
            voice: Mutex::new(String::new()),
            rate: Mutex::new(1.0),
            playback: Mutex::new(Arc::default()),
        })
    }
}
impl TextToSpeech for SystemTts {
    fn speak(&self, text: &str) -> VoiceResult<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        self.stop()?;
        let synth = SpeechSynthesizer::new().map_err(error)?;
        let selected = self.voice.lock().map_err(error)?.clone();
        for voice in SpeechSynthesizer::AllVoices().map_err(error)? {
            if voice.DisplayName().map_err(error)?.to_string() == selected {
                synth.SetVoice(&voice).map_err(error)?;
                break;
            }
        }
        synth
            .Options()
            .map_err(error)?
            .SetSpeakingRate(*self.rate.lock().map_err(error)?)
            .map_err(error)?;
        let stream = synth
            .SynthesizeTextToStreamAsync(&HSTRING::from(text))
            .map_err(error)?
            .get()
            .map_err(error)?;
        let size = stream.Size().map_err(error)?;
        if size > 64 * 1024 * 1024 {
            return Err(error("Слишком длинная фраза для озвучивания"));
        }
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(error)?)
            .map_err(error)?;
        reader
            .LoadAsync(size as u32)
            .map_err(error)?
            .get()
            .map_err(error)?;
        let mut bytes = vec![0; size as usize];
        reader.ReadBytes(&mut bytes).map_err(error)?;
        let (samples, rate) = crate::tts_http::decode_wav(&bytes)?;
        let playback = Arc::new(Playback::default());
        playback.playing.store(true, Ordering::Relaxed);
        *self.playback.lock().map_err(error)? = playback.clone();
        std::thread::spawn(move || {
            if let Err(e) =
                crate::tts_http::play(samples, rate, &playback.cancelled, &playback.level)
            {
                tracing::warn!("{e}");
            }
            playback.level.store(0, Ordering::Relaxed);
            playback.playing.store(false, Ordering::Relaxed);
        });
        Ok(())
    }
    fn stop(&self) -> VoiceResult<()> {
        let p = self.playback.lock().map_err(error)?;
        p.cancelled.store(true, Ordering::Relaxed);
        p.playing.store(false, Ordering::Relaxed);
        p.level.store(0, Ordering::Relaxed);
        Ok(())
    }
    fn is_speaking(&self) -> bool {
        self.playback
            .lock()
            .map(|p| p.playing.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
    fn level(&self) -> Option<f32> {
        Some(
            self.playback
                .lock()
                .map(|p| f32::from_bits(p.level.load(Ordering::Relaxed)))
                .unwrap_or(0.0),
        )
    }
    fn voices(&self) -> Vec<String> {
        SpeechSynthesizer::AllVoices()
            .map(|list| {
                list.into_iter()
                    .filter_map(|v| v.DisplayName().ok().map(|n| n.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn set_voice(&self, name: &str) -> VoiceResult<()> {
        if !self.voices().iter().any(|n| n == name) {
            return Err(error("Системный голос не найден"));
        }
        *self.voice.lock().map_err(error)? = name.to_owned();
        Ok(())
    }
    fn set_rate(&self, rate: f32) -> VoiceResult<()> {
        *self.rate.lock().map_err(error)? = 0.5 + rate.clamp(0.0, 1.0) as f64;
        Ok(())
    }
}
impl Drop for SystemTts {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
