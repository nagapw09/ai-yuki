//! Голосовые сообщения Telegram: OGG/Opus → моно 16 кГц.
//!
//! Telegram присылает голосовые в OGG с кодеком Opus. FFmpeg для этого не
//! нужен: декодер написан на чистом Rust, а Opus умеет отдавать звук сразу с
//! той частотой, которую ждёт распознавание, — пересчитывать не приходится.

use std::io::Cursor;

use opus_pure::{OggOpusReader, Trim};

use crate::capture::TARGET_RATE;
use crate::{VoiceError, VoiceResult};

/// Самый длинный пакет Opus — 120 мс; на 16 кГц это 1920 отсчётов на канал.
const MAX_FRAME: usize = TARGET_RATE as usize * 120 / 1000;

/// Раскодирует OGG/Opus в моно с частотой [`TARGET_RATE`].
pub fn decode_ogg_opus(bytes: &[u8]) -> VoiceResult<Vec<f32>> {
    let fail = |e: &dyn std::fmt::Display| VoiceError::Stt(format!("не удалось разобрать голосовое: {e}"));

    let mut reader = OggOpusReader::new(Cursor::new(bytes)).map_err(|e| fail(&e))?;
    let head = reader.head().clone();
    let channels = head.channel_count.max(1) as usize;
    let mut decoder = head.decoder(TARGET_RATE as i32).map_err(|e| fail(&e))?;
    let mut trim = Trim::new(&head, TARGET_RATE as i32, channels).map_err(|e| fail(&e))?;

    let mut block = vec![0f32; MAX_FRAME * channels];
    let mut mono = Vec::new();
    for packet in reader.packets() {
        let packet = packet.map_err(|e| fail(&e))?;
        let frames = decoder.decode(&packet.data, MAX_FRAME, &mut block).map_err(|e| fail(&e))?;
        let kept = trim.keep(&packet, &block[..frames * channels]);
        // Каналы сводятся в один: распознаванию стерео не нужно.
        mono.extend(kept.chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32));
    }
    Ok(mono)
}

#[cfg(test)]
mod tests {
    use opus_pure::{Application, OggOpusWriter, OpusEncoder, OpusHead, MAX_PACKET_BYTES};

    use super::*;

    /// Голосовое так, как его пишет Telegram: моно Opus 48 кГц в OGG.
    fn voice_message(seconds: f32) -> Vec<u8> {
        let rate = 48_000usize;
        let tone: Vec<f32> = (0..(rate as f32 * seconds) as usize)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5)
            .collect();
        let mut encoder = OpusEncoder::new(48_000, 1, Application::Voip).unwrap();
        let mut writer = OggOpusWriter::new(Vec::new(), OpusHead::new(1, 48_000).unwrap()).unwrap();
        let mut packet = vec![0u8; MAX_PACKET_BYTES];
        for frame in tone.chunks_exact(960) {
            let n = encoder.encode(frame, 960, &mut packet).unwrap();
            writer.write_packet_with_duration(&packet[..n], 960).unwrap();
        }
        writer.finish().unwrap()
    }

    #[test]
    fn a_voice_message_becomes_16khz_mono_of_the_same_length() {
        let samples = decode_ogg_opus(&voice_message(1.0)).unwrap();
        let seconds = samples.len() as f32 / TARGET_RATE as f32;
        assert!((seconds - 1.0).abs() < 0.05, "длительность {seconds} с");
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        assert!(rms > 0.2, "звук не потерялся: rms {rms}");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(decode_ogg_opus(b"not an ogg file").is_err());
    }
}
