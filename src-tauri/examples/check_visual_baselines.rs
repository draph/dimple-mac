use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::{collections::HashSet, error::Error, fs, io::Cursor, path::PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
type RgbaPixels = Vec<[u8; 4]>;
const THEMES: [(&str, &str); 7] = [
    ("native", "native"),
    ("#EF4444", "red"),
    ("#F97316", "orange"),
    ("#EAB308", "yellow"),
    ("#22C55E", "green"),
    ("#A855F7", "purple"),
    ("#808080", "gray"),
];

fn decode(bytes: &[u8]) -> Result<(u32, u32, RgbaPixels), Box<dyn Error>> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut rgba = vec![0; reader.output_buffer_size().ok_or("E_PNG_SIZE")?];
    let info = reader.next_frame(&mut rgba)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("E_PNG_RGBA".into());
    }
    let (pixels, remainder) = rgba[..info.buffer_size()].as_chunks::<4>();
    if !remainder.is_empty() {
        return Err("E_PNG_RGBA".into());
    }
    Ok((info.width, info.height, pixels.to_vec()))
}

fn run() -> Result<(), Box<dyn Error>> {
    let input = std::env::args_os()
        .nth(1)
        .ok_or("usage: check_visual_baselines <export.json>")?;
    let current: Value = serde_json::from_slice(&fs::read(input)?)?;
    let manifest: Value = serde_json::from_slice(&fs::read(
        PathBuf::from(ROOT).join("../tests/visual-baselines/manifest.json"),
    )?)?;
    for key in ["userAgent", "platform", "devicePixelRatio"] {
        let current_key = if key == "devicePixelRatio" {
            "dpr"
        } else {
            key
        };
        if current["environment"][current_key] != manifest["environment"][key] {
            return Err(format!("E_ENVIRONMENT {key}").into());
        }
    }
    let current_images = current["all"].as_array().ok_or("E_CURRENT_IMAGES")?;
    let expected = manifest["images"].as_array().ok_or("E_MANIFEST_IMAGES")?;
    if current_images.len() != expected.len() || expected.len() != 70 {
        return Err("E_IMAGE_COUNT".into());
    }

    let mut seen = HashSet::new();
    let mut max_delta = 0u8;
    let mut different_pixels = 0u64;
    for image in current_images {
        let sample = image["sample"].as_str().ok_or("E_SAMPLE")?;
        let theme = image["theme"].as_str().ok_or("E_THEME")?;
        let size = image["size"].as_u64().ok_or("E_SIZE")? as u32;
        let theme_name = THEMES
            .iter()
            .find(|(value, _)| *value == theme)
            .ok_or("E_UNKNOWN_THEME")?
            .1;
        let file = format!("{sample}-{theme_name}-{size}.png");
        if !seen.insert(file.clone()) {
            return Err("E_DUPLICATE_IMAGE".into());
        }
        let metadata = expected
            .iter()
            .find(|item| item["file"].as_str() == Some(&file))
            .ok_or("E_UNEXPECTED_IMAGE")?;
        let golden = fs::read(
            PathBuf::from(ROOT)
                .join("../tests/visual-baselines")
                .join(&file),
        )?;
        let actual = STANDARD.decode(image["png"].as_str().ok_or("E_PNG_BASE64")?)?;
        let (width, height, reference) = decode(&golden)?;
        let (actual_width, actual_height, pixels) = decode(&actual)?;
        if (width, height) != (size, size) || (actual_width, actual_height) != (size, size) {
            return Err(format!("E_DIMENSIONS {file}").into());
        }
        if !reference.iter().any(|pixel| pixel[3] == 0)
            || !reference.iter().any(|pixel| pixel[3] > 0)
        {
            return Err(format!("E_ALPHA {file}").into());
        }
        let mut file_delta = 0u8;
        let mut file_different = 0u64;
        for (expected, actual) in reference.iter().zip(&pixels) {
            let delta = expected
                .iter()
                .zip(actual)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            file_delta = file_delta.max(delta);
            file_different += u64::from(delta > 0);
        }
        max_delta = max_delta.max(file_delta);
        different_pixels += file_different;
        let threshold = &manifest["pixelThresholds"][size.to_string()];
        if file_delta > threshold["maxChannelDelta"].as_u64().ok_or("E_THRESHOLD")? as u8
            || file_different > threshold["differentPixels"].as_u64().ok_or("E_THRESHOLD")?
        {
            return Err(
                format!("E_PIXEL_DIFF {file}: max={file_delta}, pixels={file_different}").into(),
            );
        }
        if metadata["size"].as_u64() != Some(size as u64) {
            return Err(format!("E_MANIFEST_SIZE {file}").into());
        }
    }
    println!(
        "PASS images={} max_channel_delta={max_delta} different_pixels={different_pixels}",
        seen.len()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("VISUAL_BASELINE_FAILED {error}");
        std::process::exit(1);
    }
}
