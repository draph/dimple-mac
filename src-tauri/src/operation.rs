use crate::{
    localization::AppError,
    targets::{self, Target},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io::Cursor, sync::Arc};
const SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];
const ICNS_TYPES: [(&str, u32); 10] = [
    ("ic04", 16),
    ("ic11", 32),
    ("ic05", 32),
    ("ic12", 64),
    ("ic07", 128),
    ("ic13", 256),
    ("ic08", 256),
    ("ic14", 512),
    ("ic09", 512),
    ("ic10", 1024),
];
fn invalid(reason: &str) -> AppError {
    AppError::reason("invalid_input", reason)
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub revision: u64,
    pub target_ids: Vec<String>,

    pub action: Action,
}
#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Apply {
        #[serde(rename = "templateId")]
        template_id: String,
        #[serde(rename = "badgeId")]
        badge_id: String,
        params: Params,
        images: Vec<Image>,
    },
    Restore {},
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    #[default]
    Engraved,
    Original,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    pub theme: String,
    #[serde(default)]
    pub effect: Effect,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            theme: "native".into(),
            effect: Effect::Engraved,
        }
    }
}
impl Params {
    pub fn validate(&self) -> Result<(), AppError> {
        if !self.scale.is_finite()
            || !(0.25..=2.).contains(&self.scale)
            || [self.offset_x, self.offset_y]
                .into_iter()
                .any(|v| !v.is_finite() || !(-100. ..=100.).contains(&v))
            || !(self.theme == "native"
                || (self.theme.len() == 7
                    && self.theme.starts_with('#')
                    && self.theme.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)))
        {
            return Err(invalid("params"));
        }
        Ok(())
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    size: u32,
    png: String,
}
pub fn parse(text: &str) -> Result<Request, AppError> {
    if text.len() > 12 * 1024 * 1024 {
        return Err(invalid("payload"));
    }
    serde_json::from_str(text).map_err(|_| invalid("payload"))
}
pub fn resolve(ids: &[String], registered: &[Target]) -> Result<Vec<Target>, AppError> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(invalid("limit"));
    }
    let mut seen = HashSet::new();
    ids.iter()
        .map(|id| {
            if !seen.insert(id) {
                return Err(invalid("duplicate"));
            }
            registered
                .iter()
                .find(|t| &t.id == id)
                .cloned()
                .ok_or_else(|| AppError::reason("target_changed", "id").target(id))
        })
        .collect()
}
pub fn validate(input: &Request) -> Result<Vec<Vec<u8>>, AppError> {
    let Action::Apply {
        template_id,
        badge_id,
        params,
        images,
    } = &input.action
    else {
        return Ok(vec![]);
    };
    if template_id != "big-sur-light-blue" || badge_id.is_empty() {
        return Err(invalid("asset"));
    }
    params.validate()?;
    if images.len() != SIZES.len() {
        return Err(invalid("images"));
    }
    let mut decoded_size = 0usize;
    for image in images {
        let len = image.png.len();
        if len % 4 != 0 {
            return Err(invalid("images"));
        }
        let padding = if image.png.ends_with("==") {
            2
        } else if image.png.ends_with('=') {
            1
        } else {
            0
        };
        let size = (len / 4 * 3)
            .checked_sub(padding)
            .ok_or_else(|| invalid("images"))?;
        decoded_size = decoded_size
            .checked_add(size)
            .ok_or_else(|| invalid("payload"))?;
        if decoded_size > 8 * 1024 * 1024 {
            return Err(invalid("payload"));
        }
    }
    let mut sizes = HashSet::new();
    let mut total = 0;
    let mut output = Vec::new();
    for image in images {
        if !SIZES.contains(&image.size) || !sizes.insert(image.size) {
            return Err(invalid("images"));
        }
        let bytes = STANDARD.decode(&image.png).map_err(|_| invalid("images"))?;
        total += bytes.len();
        if total > 8 * 1024 * 1024 {
            return Err(invalid("payload"));
        }
        let mut decoder = png::Decoder::new_with_limits(
            Cursor::new(&bytes),
            png::Limits {
                bytes: 8 * 1024 * 1024,
            },
        );
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|_| invalid("images"))?;
        let info = reader.info();
        if info.width != image.size || info.height != image.size || info.animation_control.is_some()
        {
            return Err(invalid("images"));
        }
        let len = reader
            .output_buffer_size()
            .ok_or_else(|| invalid("images"))?;
        if len > 1024 * 1024 * 4 {
            return Err(invalid("images"));
        }
        let mut rgba = vec![0; len];
        let frame = reader
            .next_frame(&mut rgba)
            .map_err(|_| invalid("images"))?;
        if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
            return Err(invalid("images"));
        }
        reader.finish().map_err(|_| invalid("images"))?;
        output.push(bytes);
    }
    Ok(output)
}
#[derive(Clone)]
pub struct Prepared {
    pub id: String,
    pub targets: Vec<Target>,
    pub icon: Option<Arc<[u8]>>,
    pub fingerprints: Vec<Result<crate::icon::Fingerprint, AppError>>,
    pub token: String,
    pub confirmation_ids: Option<Vec<String>>,
    pub previous: Option<Progress>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub operation_id: String,
    target_ids: Vec<String>,
    linked_target_ids: Vec<String>,
    overwrite_target_ids: Vec<String>,
    restore: bool,
    confirmation_token: Option<String>,
    commit_available: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub target_id: String,
    pub status: String,
    pub error: Option<AppError>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub operation_id: String,
    pub sequence: u64,
    pub phase: String,
    pub items: Vec<ItemResult>,
    pub slow: bool,
    pub stopped: bool,
}
impl Progress {
    pub fn new(p: &Prepared) -> Self {
        Self {
            operation_id: p.id.clone(),
            sequence: 0,
            phase: "checking".into(),
            items: p
                .targets
                .iter()
                .map(|t| ItemResult {
                    target_id: t.id.clone(),
                    status: "pending".into(),
                    error: None,
                })
                .collect(),
            slow: false,
            stopped: false,
        }
    }
    pub fn busy(&self) -> bool {
        // An uncertain result may still have written and keeps edits blocked until restart.
        self.phase != "completed"
    }
}
impl Prepared {
    pub(crate) fn new(icon: Option<Arc<[u8]>>, targets: Vec<Target>) -> Self {
        Self {
            id: targets::next_id("operation"),
            targets,
            icon,
            fingerprints: vec![],
            token: targets::next_id("confirmation"),
            confirmation_ids: None,
            previous: None,
        }
    }
    pub fn build(request: Request, targets: Vec<Target>) -> Result<Self, AppError> {
        let icon = match &request.action {
            Action::Apply { images, .. } => Some(encode_icns(images, &validate(&request)?)?.into()),
            Action::Restore {} => None,
        };
        drop(request);
        let mut p = Self::new(icon, targets);
        p.refresh()?;
        Ok(p)
    }
    pub fn refresh(&mut self) -> Result<(), AppError> {
        self.fingerprints =
            targets::check(targets::Request::Fingerprint(self.targets.clone()))?.fingerprints;
        self.token = targets::next_id("confirmation");
        Ok(())
    }
    pub fn needs_confirmation(&self) -> bool {
        if let Some(ids) = &self.confirmation_ids {
            return !ids.is_empty();
        }
        self.targets.iter().zip(&self.fingerprints).any(|(t, f)| {
            f.as_ref()
                .is_ok_and(|f| self.icon.is_none() || t.linked || f.has_icon())
        })
    }
    pub fn summary(&self) -> Summary {
        let selected: Vec<_> = self
            .targets
            .iter()
            .zip(&self.fingerprints)
            .filter(|(t, f)| {
                f.is_ok()
                    && self
                        .confirmation_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&t.id))
            })
            .collect();
        Summary {
            operation_id: self.id.clone(),
            target_ids: selected.iter().map(|(t, _)| t.id.clone()).collect(),
            linked_target_ids: selected
                .iter()
                .filter(|(t, _)| t.linked)
                .map(|(t, _)| t.id.clone())
                .collect(),
            overwrite_target_ids: selected
                .iter()
                .filter(|(_, f)| f.as_ref().is_ok_and(|f| f.has_icon()))
                .map(|(t, _)| t.id.clone())
                .collect(),
            restore: self.icon.is_none(),
            confirmation_token: self.needs_confirmation().then(|| self.token.clone()),
            commit_available: true,
        }
    }
    pub fn changed_consent(&self, old: &Self) -> Vec<String> {
        self.targets
            .iter()
            .zip(&self.fingerprints)
            .zip(&old.fingerprints)
            .filter(|((_, current), previous)| current.is_ok() && *current != *previous)
            .map(|((t, _), _)| t.id.clone())
            .collect()
    }
    pub fn refreshed(&self) -> Result<Self, AppError> {
        let mut p = self.clone();
        p.fingerprints.clear();
        p.token.clear();
        p.refresh()?;
        Ok(p)
    }
}
fn encode_icns(supplied: &[Image], images: &[Vec<u8>]) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::from(&b"icns\0\0\0\0"[..]);
    for (kind, size) in ICNS_TYPES {
        let index = supplied
            .iter()
            .position(|image| image.size == size)
            .ok_or_else(|| invalid("images"))?;
        let png = images.get(index).ok_or_else(|| invalid("images"))?;
        let payload = if matches!(kind, "ic04" | "ic05") {
            legacy_argb(png)?
        } else {
            png.clone()
        };
        bytes.extend(kind.as_bytes());
        bytes.extend(((payload.len() + 8) as u32).to_be_bytes());
        bytes.extend(payload);
    }
    let len = bytes.len() as u32;
    bytes[4..8].copy_from_slice(&len.to_be_bytes());
    Ok(bytes)
}
fn legacy_argb(png: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| invalid("images"))?;
    let mut rgba = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| invalid("images"))?
    ];
    let frame = reader
        .next_frame(&mut rgba)
        .map_err(|_| invalid("images"))?;
    if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
        return Err(invalid("images"));
    }
    reader.finish().map_err(|_| invalid("images"))?;

    let (pixels, remainder) = rgba[..frame.buffer_size()].as_chunks::<4>();
    if !remainder.is_empty() {
        return Err(invalid("images"));
    }
    let mut encoded = Vec::from(&b"ARGB"[..]);
    for channel in [3, 0, 1, 2] {
        let plane: Vec<_> = pixels
            .iter()
            .map(|pixel| {
                if channel == 3 {
                    pixel[channel]
                } else {
                    ((u16::from(pixel[channel]) * u16::from(pixel[3]) + 127) / 255) as u8
                }
            })
            .collect();
        for packet in plane.chunks(128) {
            encoded.push((packet.len() - 1) as u8);
            encoded.extend(packet);
        }
    }
    // Apple Silicon drops the final compressed blue byte unless the payload ends in zero.
    encoded.push(0);
    Ok(encoded)
}
#[cfg(test)]
struct DropDir(std::path::PathBuf);
#[cfg(test)]
impl Drop for DropDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn icns_probe_pixel(index: usize) -> [u8; 4] {
        let alpha = [0, 1, 2, 31, 64, 127, 128, 192, 254, 255][index % 10];
        [
            (index * 37 % 256) as u8,
            (index * 67 + 13) as u8,
            (index * 97 + 77) as u8,
            alpha,
        ]
    }
    fn request() -> Request {
        parse(r#"{"revision":1,"targetIds":["x"],"action":{"type":"restore"}}"#).unwrap()
    }
    #[test]
    fn effects_default_to_engraved_and_reject_unknown_values() {
        let input = serde_json::json!({"scale":1,"offsetX":0,"offsetY":0,"theme":"native"});
        let params: Params = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(params.effect, Effect::Engraved);
        for effect in ["original", "unknown"] {
            let mut input = input.clone();
            input["effect"] = effect.into();
            assert_eq!(
                serde_json::from_value::<Params>(input).is_ok(),
                effect == "original"
            );
        }
    }
    #[test]
    fn render_params_validate_themes_and_numeric_boundaries() {
        for color in ["native", "#EF4444", "#000000", "#ffffff"] {
            for scale in [0.25, 2.] {
                let p: Params = serde_json::from_value(
                    serde_json::json!({"scale":scale,"offsetX":-100,"offsetY":100,"theme":color}),
                )
                .unwrap();
                assert!(p.validate().is_ok());
            }
        }
        for color in ["", "red", "#FFF", "#12345G", "#１２３４５６"] {
            let p = Params {
                theme: color.into(),
                ..Default::default()
            };
            assert!(p.validate().is_err());
        }
        for scale in [f64::NAN, f64::INFINITY, 0.24, 2.01] {
            assert!(
                Params {
                    scale,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
    }
    #[test]
    fn rejects_unregistered_duplicate_and_arbitrary_path_inputs() {
        assert!(resolve(&["/tmp/arbitrary".into()], &[]).is_err());
        assert!(resolve(&[], &[]).is_err());
        assert!(
            parse(r#"{"revision":1,"targetIds":["x"],"action":{"type":"restore"},"path":"/tmp/arbitrary"}"#)
                .is_err()
        );
        assert!(validate(&request()).unwrap().is_empty());
    }
    #[test]
    fn wire_budget_and_restore_payload_are_strict() {
        assert!(parse(&" ".repeat(12 * 1024 * 1024 + 1)).is_err());
        assert!(
            parse(r#"{"revision":1,"targetIds":["x"],"action":{"type":"restore","images":[]}}"#)
                .is_err()
        );
        assert!(parse(r#"{"revision":1,"targetIds":["x"],"action":{"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":null,"offsetX":0,"offsetY":0,"theme":"native"},"images":[]}}"#).is_err());
    }
    #[test]
    fn validates_real_png_sizes_corruption_and_parameter_bounds() {
        let images = SIZES
            .into_iter()
            .map(|size| {
                let mut b = Vec::new();
                {
                    let mut e = png::Encoder::new(&mut b, size, size);
                    e.set_color(png::ColorType::Rgba);
                    e.set_depth(png::BitDepth::Eight);
                    let mut w = e.write_header().unwrap();
                    w.write_image_data(&vec![128; size as usize * size as usize * 4])
                        .unwrap();
                }
                Image {
                    size,
                    png: STANDARD.encode(b),
                }
            })
            .collect();
        let mut r = Request {
            revision: 1,
            target_ids: vec!["x".into()],
            action: Action::Apply {
                template_id: "big-sur-light-blue".into(),
                badge_id: "code".into(),
                params: Params {
                    effect: Effect::Engraved,
                    scale: 1.,
                    offset_x: 0.,
                    offset_y: 0.,
                    theme: "native".into(),
                },
                images,
            },
        };
        assert_eq!(validate(&r).unwrap().len(), 7);
        if let Action::Apply { images, .. } = &mut r.action {
            images[0].size = 32;
        }
        assert!(validate(&r).is_err());
        if let Action::Apply { images, params, .. } = &mut r.action {
            images[0].size = 16;
            params.offset_x = 101.;
        }
        assert!(validate(&r).is_err());
        if let Action::Apply { params, .. } = &mut r.action {
            params.offset_x = 0.;
            params.scale = f64::NAN;
        }
        assert!(validate(&r).is_err());
        if let Action::Apply { params, images, .. } = &mut r.action {
            params.scale = 1.;
            images[0].png = "invalid".into();
        }
        assert!(validate(&r).is_err());
    }

    #[test]
    fn icns_round_trips_all_standard_and_retina_png_types() {
        let pngs: Vec<_> = SIZES
            .into_iter()
            .enumerate()
            .map(|(index, size)| {
                let mut bytes = Vec::new();
                {
                    let mut encoder = png::Encoder::new(&mut bytes, size, size);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder.write_header().unwrap();
                    let color = [index as u8 * 30, 80, 160, 255];
                    let pixels: Vec<_> = color
                        .into_iter()
                        .cycle()
                        .take(size as usize * size as usize * 4)
                        .collect();
                    writer.write_image_data(&pixels).unwrap();
                }
                bytes
            })
            .collect();
        let supplied: Vec<_> = SIZES
            .into_iter()
            .map(|size| Image {
                size,
                png: String::new(),
            })
            .collect();
        let bytes = encode_icns(&supplied, &pngs).unwrap();
        assert_eq!(&bytes[..4], b"icns");
        assert_eq!(
            u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len()
        );

        let mut offset = 8;
        for (kind, size) in ICNS_TYPES {
            assert_eq!(&bytes[offset..offset + 4], kind.as_bytes());
            let length =
                u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let payload = &bytes[offset + 8..offset + length];
            let image_index = SIZES
                .iter()
                .position(|candidate| *candidate == size)
                .unwrap();
            let expected = if matches!(kind, "ic04" | "ic05") {
                legacy_argb(&pngs[image_index]).unwrap()
            } else {
                pngs[image_index].clone()
            };
            assert_eq!(payload, expected);
            if matches!(kind, "ic04" | "ic05") {
                assert!(payload.starts_with(b"ARGB"));
            } else {
                let reader = png::Decoder::new(Cursor::new(payload)).read_info().unwrap();
                assert_eq!((reader.info().width, reader.info().height), (size, size));
            }
            offset += length;
        }
        assert_eq!(offset, bytes.len());
    }

    #[test]
    fn system_iconutil_reads_every_icns_representation_without_inventing_48px() {
        use std::{fs, process::Command, time::SystemTime};

        let pngs: Vec<_> = SIZES
            .into_iter()
            .map(|size| {
                let mut bytes = Vec::new();
                {
                    let mut encoder = png::Encoder::new(&mut bytes, size, size);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder.write_header().unwrap();
                    let pixels: Vec<_> = (0..size as usize * size as usize)
                        .flat_map(icns_probe_pixel)
                        .collect();
                    writer.write_image_data(&pixels).unwrap();
                }
                bytes
            })
            .collect();
        let supplied: Vec<_> = SIZES
            .into_iter()
            .map(|size| Image {
                size,
                png: String::new(),
            })
            .collect();
        let bytes = encode_icns(&supplied, &pngs).unwrap();
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("dimple-t14-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let _cleanup = DropDir(root.clone());
        let icns = root.join("visual.icns");
        fs::write(&icns, bytes).unwrap();
        let output = Command::new("/usr/bin/iconutil")
            .args(["-c", "iconset"])
            .arg(&icns)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let iconset = root.join("visual.iconset");
        let mut dimensions: Vec<_> = fs::read_dir(&iconset)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let filename = entry.file_name();
                let bytes = fs::read(entry.path()).unwrap();
                let mut decoder = png::Decoder::new(Cursor::new(bytes));
                decoder.set_transformations(png::Transformations::normalize_to_color8());
                let mut reader = decoder.read_info().unwrap();
                let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut pixels).unwrap();
                let rgba = &pixels[..info.buffer_size()];
                let (rgba, remainder) = rgba.as_chunks::<4>();
                assert!(remainder.is_empty());
                for (index, pixel) in rgba.iter().enumerate() {
                    let source = icns_probe_pixel(index);
                    assert_eq!(
                        pixel[3], source[3],
                        "alpha changed at {}x{}",
                        info.width, info.height
                    );
                    if matches!(info.width, 16 | 32) {
                        for channel in 0..3 {
                            let expected =
                                (u16::from(source[channel]) * u16::from(source[3]) + 127) / 255;
                            let actual =
                                (u16::from(pixel[channel]) * u16::from(pixel[3]) + 127) / 255;
                            assert!(
                                expected.abs_diff(actual) <= 1,
                                "premultiplied RGB changed at {}x{} {}: {pixel:?}",
                                info.width,
                                info.height,
                                filename.to_string_lossy(),
                            );
                        }
                    } else {
                        assert_eq!(
                            *pixel, source,
                            "PNG changed at {}x{}",
                            info.width, info.height
                        );
                    }
                }
                (info.width, info.height)
            })
            .collect();
        dimensions.sort_unstable();
        assert_eq!(
            dimensions,
            [
                (16, 16),
                (32, 32),
                (32, 32),
                (64, 64),
                (128, 128),
                (256, 256),
                (256, 256),
                (512, 512),
                (512, 512),
                (1024, 1024)
            ]
        );
    }
}
