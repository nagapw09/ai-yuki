fn main() -> Result<(), String> {
    let input = std::env::args().nth(1).ok_or("VRM path required")?;
    let started = std::time::Instant::now();
    let bytes = yuki_desktop_lib::avatar_assets::model_bytes(
        std::path::Path::new(&input),
        std::path::Path::new("tmp/vrm-cache"),
    )?;
    println!(
        "Optimized bytes: {}, elapsed: {:?}",
        bytes.len(),
        started.elapsed()
    );
    Ok(())
}
