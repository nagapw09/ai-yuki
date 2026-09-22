//! Disk cache for desktop-size VRMs. Source files are never changed.
use serde_json::Value;
use std::{
    hash::{Hash, Hasher},
    io::Cursor,
    path::Path,
};

pub fn model_bytes(path: &Path, cache: &Path) -> Result<Vec<u8>, String> {
    let info = path.metadata().map_err(|e| e.to_string())?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    info.len().hash(&mut hash);
    info.modified().ok().hash(&mut hash);
    "textures-1024-v1".hash(&mut hash);
    let cached = cache.join(format!("{:x}.vrm", hash.finish()));
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(bytes);
    }
    let source = std::fs::read(path).map_err(|e| e.to_string())?;
    let bytes = resize_textures(&source, 1024).unwrap_or_else(|error| {
        tracing::warn!(%error,"VRM texture optimization skipped");
        source
    });
    if std::fs::create_dir_all(cache).is_ok() {
        let staging = cached.with_extension("part");
        if std::fs::write(&staging, &bytes).is_ok() {
            let _ = std::fs::rename(staging, cached);
        }
    }
    Ok(bytes)
}

fn resize_textures(bytes: &[u8], limit: u32) -> Result<Vec<u8>, String> {
    let invalid = || "Некорректный VRM/GLB".to_string();
    if bytes.len() < 28 || &bytes[..4] != b"glTF" || &bytes[16..20] != b"JSON" {
        return Err(invalid());
    }
    let size = u32::from_le_bytes(bytes[12..16].try_into().map_err(|_| invalid())?) as usize;
    let end = 20usize.checked_add(size).ok_or_else(invalid)?;
    let mut json: Value = serde_json::from_slice(bytes.get(20..end).ok_or_else(invalid)?)
        .map_err(|e| e.to_string())?;
    if bytes.get(end + 4..end + 8) != Some(b"BIN\0".as_slice()) {
        return Err(invalid());
    }
    let bin = bytes.get(end + 8..).ok_or_else(invalid)?;
    let mut replacements = std::collections::HashMap::<usize, Vec<u8>>::new();
    if let Some(images) = json["images"].as_array() {
        for img in images {
            let Some(index) = img["bufferView"].as_u64().map(|v| v as usize) else {
                continue;
            };
            if replacements.contains_key(&index) {
                continue;
            }
            let view = &json["bufferViews"][index];
            let start = view["byteOffset"].as_u64().unwrap_or(0) as usize;
            let len = view["byteLength"].as_u64().ok_or_else(invalid)? as usize;
            let data = bin
                .get(start..start.checked_add(len).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            let reader = image::ImageReader::new(Cursor::new(data))
                .with_guessed_format()
                .map_err(|e| e.to_string())?;
            let decoded = reader.decode().map_err(|e| e.to_string())?;
            if decoded.width() <= limit && decoded.height() <= limit {
                continue;
            }
            let resized = decoded.resize(limit, limit, image::imageops::FilterType::Triangle);
            let mut png = Cursor::new(Vec::new());
            resized
                .write_to(&mut png, image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            replacements.insert(index, png.into_inner());
        }
    }
    if replacements.is_empty() {
        return Ok(bytes.to_vec());
    }
    if let Some(images) = json["images"].as_array_mut() {
        for img in images {
            if img["bufferView"]
                .as_u64()
                .is_some_and(|i| replacements.contains_key(&(i as usize)))
            {
                img["mimeType"] = "image/png".into();
            }
        }
    }
    let mut packed = Vec::with_capacity(bin.len());
    for (i, view) in json["bufferViews"]
        .as_array_mut()
        .ok_or_else(invalid)?
        .iter_mut()
        .enumerate()
    {
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = view["byteLength"].as_u64().ok_or_else(invalid)? as usize;
        let data = if let Some(replacement) = replacements.get(&i) {
            replacement.as_slice()
        } else {
            bin.get(start..start.checked_add(len).ok_or_else(invalid)?)
                .ok_or_else(invalid)?
        };
        while packed.len() % 4 != 0 {
            packed.push(0)
        }
        view["byteOffset"] = packed.len().into();
        view["byteLength"] = data.len().into();
        packed.extend_from_slice(data);
    }
    json["buffers"][0]["byteLength"] = packed.len().into();
    let mut text = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    while text.len() % 4 != 0 {
        text.push(b' ')
    }
    while packed.len() % 4 != 0 {
        packed.push(0)
    }
    let mut result = Vec::with_capacity(28 + text.len() + packed.len());
    result.extend(b"glTF");
    result.extend(2u32.to_le_bytes());
    result.extend(((28 + text.len() + packed.len()) as u32).to_le_bytes());
    result.extend((text.len() as u32).to_le_bytes());
    result.extend(b"JSON");
    result.extend(text);
    result.extend((packed.len() as u32).to_le_bytes());
    result.extend(b"BIN\0");
    result.extend(packed);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_glb_is_rejected_without_panicking() {
        for b in [&b""[..], &b"glTF00000000\xff\xff\xff\xffJSON"[..]] {
            assert!(resize_textures(b, 1024).is_err())
        }
    }
    #[test]
    fn texture_cache_resizes_images_and_preserves_geometry_bytes() {
        let image = image::DynamicImage::new_rgba8(2048, 512);
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        let meta = serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":4+png.len()}],"bufferViews":[{"byteOffset":0,"byteLength":4,"buffer":0},{"byteOffset":4,"byteLength":png.len(),"buffer":0}],"images":[{"bufferView":1,"mimeType":"image/png"}]});
        let mut text = serde_json::to_vec(&meta).unwrap();
        while text.len() % 4 != 0 {
            text.push(b' ')
        }
        let mut bin = vec![1, 2, 3, 4];
        bin.extend(png);
        while bin.len() % 4 != 0 {
            bin.push(0)
        }
        let mut input = b"glTF".to_vec();
        input.extend(2u32.to_le_bytes());
        input.extend(((28 + text.len() + bin.len()) as u32).to_le_bytes());
        input.extend((text.len() as u32).to_le_bytes());
        input.extend(b"JSON");
        input.extend(text);
        input.extend((bin.len() as u32).to_le_bytes());
        input.extend(b"BIN\0");
        input.extend(bin);
        let result = resize_textures(&input, 1024).unwrap();
        let size = u32::from_le_bytes(result[12..16].try_into().unwrap()) as usize;
        let meta: Value = serde_json::from_slice(&result[20..20 + size]).unwrap();
        let bin = &result[28 + size..];
        assert_eq!(&bin[..4], &[1, 2, 3, 4]);
        let view = &meta["bufferViews"][1];
        let start = view["byteOffset"].as_u64().unwrap() as usize;
        let len = view["byteLength"].as_u64().unwrap() as usize;
        let decoded = image::load_from_memory(&bin[start..start + len]).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1024, 256));
        assert!(result.len() < input.len());
    }
}
