//! Прогоняет WAV через подавление шума: `cargo run -p yuki-voice --example denoise_wav -- вход.wav выход.wav`.
//!
//! Нужен, чтобы отделить «подавление портит речь» от «микрофон приносит плохой
//! звук»: один и тот же файл до и после можно отдать распознаванию и сравнить.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        return eprintln!("нужно: вход.wav выход.wav");
    };

    let mut reader = hound::WavReader::open(input).expect("файл не открылся");
    let spec = reader.spec();
    println!("вход: {} Гц, каналов {}", spec.sample_rate, spec.channels);

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect(),
    };
    // Сводим в моно.
    let mono: Vec<f32> = samples
        .chunks(spec.channels as usize)
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect();

    // Сеть ждёт 48 кГц; файл на 16 кГц растягиваем втрое линейно, потом обратно.
    let upsampled: Vec<f32> = if spec.sample_rate == 48_000 {
        mono.clone()
    } else {
        let ratio = 48_000.0 / spec.sample_rate as f32;
        (0..(mono.len() as f32 * ratio) as usize)
            .map(|i| mono[((i as f32 / ratio) as usize).min(mono.len() - 1)])
            .collect()
    };

    let mut state = nnnoiseless::DenoiseState::new();
    let frame = nnnoiseless::DenoiseState::FRAME_SIZE;
    let mut out = vec![0.0_f32; frame];
    let mut clean: Vec<f32> = Vec::with_capacity(upsampled.len());
    for chunk in upsampled.chunks(frame) {
        let mut padded = vec![0.0_f32; frame];
        for (slot, value) in padded.iter_mut().zip(chunk) {
            *slot = value * 32_768.0;
        }
        state.process_frame(&mut out, &padded);
        clean.extend(out.iter().map(|v| v / 32_768.0));
    }

    // Возвращаемся к исходной частоте.
    let back: Vec<f32> = if spec.sample_rate == 48_000 {
        clean
    } else {
        let ratio = 48_000.0 / spec.sample_rate as f32;
        (0..mono.len())
            .map(|i| clean[((i as f32 * ratio) as usize).min(clean.len() - 1)])
            .collect()
    };

    let mut writer = hound::WavWriter::create(
        output,
        hound::WavSpec {
            channels: 1,
            sample_rate: spec.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .expect("не записать");
    for value in &back {
        writer
            .write_sample((value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .unwrap();
    }
    writer.finalize().unwrap();

    let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len().max(1) as f32).sqrt();
    println!("RMS до: {:.4}, после: {:.4}", rms(&mono), rms(&back));
}
