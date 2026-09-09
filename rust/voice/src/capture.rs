//! Захват микрофона (ТЗ §10).
//!
//! Наружу выдаются кадры фиксированной длины в моно 16 кГц — ровно то, что ждут
//! и VAD, и любой распознаватель речи. Приведение к этому формату спрятано здесь,
//! потому что реальные устройства отдают что угодно: 44.1 или 48 кГц, два канала,
//! `i16` или `f32`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};

use crate::error::{VoiceError, VoiceResult};

/// Частота дискретизации, в которой работает весь конвейер.
pub const TARGET_RATE: u32 = 16_000;

/// Длина кадра. 20 мс — стандартный шаг для VAD и достаточно мелкий,
/// чтобы задержка реакции на голос оставалась незаметной.
pub const FRAME_MS: u32 = 20;
pub const FRAME_SAMPLES: usize = (TARGET_RATE as usize * FRAME_MS as usize) / 1000;

/// Ручка работающего захвата. Уронив её, поток останавливают.
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CaptureHandle {
    /// Останавливает захват и дожидается закрытия потока.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Имя устройства ввода по умолчанию — для показа в настройках.
pub fn default_input_name() -> Option<String> {
    cpal::default_host()
        .default_input_device()
        .and_then(|d| d.name().ok())
}

/// Список доступных микрофонов.
pub fn input_devices() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Запускает захват. `on_frame` вызывается на каждые 20 мс моно-звука 16 кГц.
///
/// Колбэк выполняется в аудиопотоке: он должен быть быстрым и не блокировать.
/// Тяжёлую работу — распознавание, сеть — выносить в другой поток.
pub fn start<F>(mut on_frame: F) -> VoiceResult<CaptureHandle>
where
    F: FnMut(&[f32]) + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let stop_in_thread = stop.clone();

    // Ошибку открытия устройства нужно вернуть вызывающему, а не потерять
    // в потоке: «микрофон не найден» — это то, что пользователь должен увидеть.
    let (tx, rx) = std::sync::mpsc::channel::<VoiceResult<()>>();

    let thread = std::thread::Builder::new()
        .name("yuki-audio".into())
        .spawn(move || {
            // cpal::Stream не Send на части платформ, поэтому и создаётся, и
            // живёт целиком внутри этого потока.
            let started = (|| -> VoiceResult<cpal::Stream> {
                let device = cpal::default_host()
                    .default_input_device()
                    .ok_or(VoiceError::NoInputDevice)?;

                let supported = device
                    .default_input_config()
                    .map_err(|e| VoiceError::Device(e.to_string()))?;

                let source_rate = supported.sample_rate().0;
                let channels = supported.channels() as usize;
                let format = supported.sample_format();
                let config: cpal::StreamConfig = supported.into();

                let mut resampler = Resampler::new(source_rate, TARGET_RATE, channels);
                let error_handler = |e| tracing::warn!(%e, "сбой аудиопотока");

                let stream = match format {
                    SampleFormat::F32 => device.build_input_stream(
                        &config,
                        move |data: &[f32], _| resampler.push(data, &mut on_frame),
                        error_handler,
                        None,
                    ),
                    SampleFormat::I16 => device.build_input_stream(
                        &config,
                        move |data: &[i16], _| {
                            let converted: Vec<f32> =
                                data.iter().map(|s| s.to_sample::<f32>()).collect();
                            resampler.push(&converted, &mut on_frame)
                        },
                        error_handler,
                        None,
                    ),
                    SampleFormat::U16 => device.build_input_stream(
                        &config,
                        move |data: &[u16], _| {
                            let converted: Vec<f32> =
                                data.iter().map(|s| s.to_sample::<f32>()).collect();
                            resampler.push(&converted, &mut on_frame)
                        },
                        error_handler,
                        None,
                    ),
                    other => return Err(VoiceError::Device(format!(
                        "неподдерживаемый формат сэмплов: {other:?}"
                    ))),
                }
                .map_err(|e| VoiceError::Device(e.to_string()))?;

                stream
                    .play()
                    .map_err(|e| VoiceError::Device(e.to_string()))?;
                Ok(stream)
            })();

            match started {
                Ok(stream) => {
                    let _ = tx.send(Ok(()));
                    while !stop_in_thread.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    drop(stream);
                }
                Err(error) => {
                    let _ = tx.send(Err(error));
                }
            }
        })
        .map_err(|e| VoiceError::Device(e.to_string()))?;

    // Ждём результата открытия устройства, а не факта запуска потока.
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(CaptureHandle {
            stop,
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err(VoiceError::Device("устройство не ответило за 5 секунд".into()))
        }
    }
}

/// Сводит каналы в моно, приводит частоту к целевой и нарезает на кадры.
///
/// Ресемплинг — усреднение по окну, а не выбрасывание лишних отсчётов. Простая
/// децимация даёт алиасинг: высокие частоты заворачиваются в слышимый диапазон
/// и портят именно то, по чему распознаватель различает согласные.
struct Resampler {
    source_rate: u32,
    target_rate: u32,
    channels: usize,
    /// Дробная позиция чтения во входном сигнале.
    position: f64,
    /// Хвост входа, не уложившийся в целое окно.
    pending: Vec<f32>,
    /// Готовые выходные отсчёты, ещё не собранные в кадр.
    out: Vec<f32>,
}

impl Resampler {
    fn new(source_rate: u32, target_rate: u32, channels: usize) -> Self {
        Self {
            source_rate: source_rate.max(1),
            target_rate,
            channels: channels.max(1),
            position: 0.0,
            pending: Vec::new(),
            out: Vec::with_capacity(FRAME_SAMPLES * 2),
        }
    }

    fn push<F: FnMut(&[f32])>(&mut self, input: &[f32], on_frame: &mut F) {
        // Сводим в моно: перед распознаванием стерео не даёт ничего, кроме
        // удвоенного объёма.
        self.pending.reserve(input.len() / self.channels + 1);
        for chunk in input.chunks(self.channels) {
            let sum: f32 = chunk.iter().sum();
            self.pending.push(sum / chunk.len() as f32);
        }

        let step = self.source_rate as f64 / self.target_rate as f64;

        loop {
            let start = self.position;
            let end = start + step;
            if end.ceil() as usize > self.pending.len() {
                break;
            }

            let from = start.floor() as usize;
            let to = (end.ceil() as usize).min(self.pending.len());
            let window = &self.pending[from..to.max(from + 1)];
            let value = window.iter().sum::<f32>() / window.len() as f32;

            self.out.push(value);
            self.position = end;

            if self.out.len() >= FRAME_SAMPLES {
                on_frame(&self.out[..FRAME_SAMPLES]);
                self.out.drain(..FRAME_SAMPLES);
            }
        }

        // Сдвигаем буфер, оставляя нужный для следующего окна хвост.
        let consumed = self.position.floor() as usize;
        if consumed > 0 {
            self.pending.drain(..consumed.min(self.pending.len()));
            self.position -= consumed as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_size_matches_twenty_milliseconds() {
        assert_eq!(FRAME_SAMPLES, 320);
    }

    #[test]
    fn downmixes_stereo_to_mono() {
        let mut resampler = Resampler::new(TARGET_RATE, TARGET_RATE, 2);
        let mut frames: Vec<Vec<f32>> = Vec::new();

        // Левый канал 1.0, правый 0.0 — в моно должно получиться 0.5.
        let input: Vec<f32> = (0..FRAME_SAMPLES * 2)
            .map(|i| if i % 2 == 0 { 1.0 } else { 0.0 })
            .collect();
        resampler.push(&input, &mut |f: &[f32]| frames.push(f.to_vec()));

        assert_eq!(frames.len(), 1);
        assert!(frames[0].iter().all(|s| (*s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resamples_down_to_the_target_rate() {
        let mut resampler = Resampler::new(48_000, TARGET_RATE, 1);
        let mut produced = 0usize;

        // Секунда сигнала на 48 кГц должна дать примерно 16 000 отсчётов.
        let input = vec![0.1_f32; 48_000];
        resampler.push(&input, &mut |f: &[f32]| produced += f.len());

        let expected = TARGET_RATE as usize;
        let error = (produced as i64 - expected as i64).unsigned_abs() as usize;
        assert!(
            error <= FRAME_SAMPLES,
            "получено {produced} отсчётов вместо ~{expected}"
        );
    }

    #[test]
    fn emits_frames_of_exact_length() {
        let mut resampler = Resampler::new(TARGET_RATE, TARGET_RATE, 1);
        let mut sizes: Vec<usize> = Vec::new();

        // Вход не кратен размеру кадра — кадры всё равно обязаны быть ровными.
        resampler.push(&vec![0.05_f32; FRAME_SAMPLES * 3 + 17], &mut |f: &[f32]| {
            sizes.push(f.len())
        });

        assert_eq!(sizes, vec![FRAME_SAMPLES; 3]);
    }

    #[test]
    fn averaging_suppresses_the_signal_that_plain_decimation_would_alias() {
        // Чередование +1/-1 на 48 кГц — это 24 кГц, вчетверо выше половины
        // целевой частоты. Усреднение обязано его погасить; децимация вместо
        // этого превратила бы его в постоянную составляющую.
        let mut resampler = Resampler::new(48_000, TARGET_RATE, 1);
        let mut peak = 0.0_f32;

        let input: Vec<f32> = (0..48_000)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        resampler.push(&input, &mut |f: &[f32]| {
            for s in f {
                peak = peak.max(s.abs());
            }
        });

        assert!(peak < 0.5, "алиасинг не подавлен, пик {peak}");
    }
}
