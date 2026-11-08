use crate::{
    localization::AppError,
    targets::{self, Target},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io::Cursor};
const SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];
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
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    scale: f64,
    offset_x: f64,
    offset_y: f64,
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
    if !params.scale.is_finite()
        || !(0.25..=2.).contains(&params.scale)
        || [params.offset_x, params.offset_y]
            .into_iter()
            .any(|v| !v.is_finite() || !(-100. ..=100.).contains(&v))
    {
        return Err(invalid("params"));
    }
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
    pub mask: Option<std::sync::Arc<crate::assets::Mask>>,
    pub id: String,
    pub request: Request,
    pub targets: Vec<Target>,
    pub icon: Option<Vec<u8>>,
    pub fingerprints: Vec<crate::icon::Fingerprint>,
    pub token: String,
    pub force_confirmation: bool,
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
    pub fn new(request: Request, targets: Vec<Target>) -> Self {
        Self {
            mask: None,
            id: targets::next_id("operation"),
            request,
            targets,
            icon: None,
            fingerprints: vec![],
            token: targets::next_id("confirmation"),
            force_confirmation: false,
        }
    }
    pub fn build(
        request: Request,
        targets: Vec<Target>,
        images: Vec<Vec<u8>>,
    ) -> Result<Self, AppError> {
        let mut p = Self::new(request, targets);
        if let Action::Apply {
            images: supplied, ..
        } = &p.request.action
        {
            let mut bytes = Vec::from(&b"icns\0\0\0\0"[..]);
            for (kind, size) in [
                ("icp4", 16),
                ("icp5", 32),
                ("icp6", 64),
                ("ic07", 128),
                ("ic08", 256),
                ("ic09", 512),
                ("ic10", 1024),
                ("ic11", 32),
                ("ic12", 64),
                ("ic13", 256),
                ("ic14", 512),
            ] {
                let index = supplied
                    .iter()
                    .position(|i| i.size == size)
                    .ok_or_else(|| invalid("images"))?;
                let png = images.get(index).ok_or_else(|| invalid("images"))?;
                bytes.extend(kind.as_bytes());
                bytes.extend(((png.len() + 8) as u32).to_be_bytes());
                bytes.extend(png);
            }
            let len = bytes.len() as u32;
            bytes[4..8].copy_from_slice(&len.to_be_bytes());
            p.icon = Some(bytes);
        }
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
        self.force_confirmation
            || matches!(self.request.action, Action::Restore {})
            || self.targets.iter().any(|t| t.linked)
            || self.fingerprints.iter().any(|f| f.has_icon())
    }
    pub fn summary(&self) -> Summary {
        Summary {
            operation_id: self.id.clone(),
            target_ids: self.targets.iter().map(|t| t.id.clone()).collect(),
            linked_target_ids: self
                .targets
                .iter()
                .filter(|t| t.linked)
                .map(|t| t.id.clone())
                .collect(),
            overwrite_target_ids: self
                .targets
                .iter()
                .zip(&self.fingerprints)
                .filter(|(_, f)| f.has_icon())
                .map(|(t, _)| t.id.clone())
                .collect(),
            restore: matches!(self.request.action, Action::Restore {}),
            confirmation_token: self.needs_confirmation().then(|| self.token.clone()),
            commit_available: true,
        }
    }
    pub fn refreshed(&self) -> Result<Self, AppError> {
        let mut p = self.clone();
        p.fingerprints.clear();
        p.token.clear();
        p.refresh()?;
        Ok(p)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        parse(r#"{"revision":1,"targetIds":["x"],"action":{"type":"restore"}}"#).unwrap()
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
        assert!(parse(r#"{"revision":1,"targetIds":["x"],"action":{"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":null,"offsetX":0,"offsetY":0},"images":[]}}"#).is_err());
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
                    scale: 1.,
                    offset_x: 0.,
                    offset_y: 0.,
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
}
