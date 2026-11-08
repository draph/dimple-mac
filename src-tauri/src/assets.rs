use crate::localization::AppError;
use base64::{Engine, engine::general_purpose::STANDARD};
use resvg::{tiny_skia, usvg};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{Cursor, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    str::FromStr,
};

const FILE_LIMIT: usize = 10 * 1024 * 1024;
const NODE_LIMIT: usize = 20_000;
const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mask {
    pub png: String,
    pub width: u32,
    pub height: u32,
    pub original_width: u32,
    pub original_height: u32,
    pub bounds: [u32; 4],
    pub opaque: bool,
}
fn error(reason: &str) -> AppError {
    AppError::reason("asset_error", reason)
}
fn dimensions(width: u32, height: u32) -> Result<(), AppError> {
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 16_000_000
    {
        Err(error("dimensions"))
    } else {
        Ok(())
    }
}
pub fn load(path: &Path) -> Result<Mask, AppError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| error("unavailable"))?;
    let meta = file.metadata().map_err(|_| error("unavailable"))?;
    if !meta.is_file() {
        return Err(error("type"));
    }
    if meta.len() > FILE_LIMIT as u64 {
        return Err(error("size"));
    }
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("unavailable"))?;
    if bytes.len() > FILE_LIMIT {
        return Err(error("size"));
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => png_mask(&bytes),
        Some("svg") => svg_mask(&bytes),
        _ => Err(error("type")),
    }
}
fn png_mask(bytes: &[u8]) -> Result<Mask, AppError> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 128 * 1024 * 1024,
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| error("damaged"))?;
    let (w, h) = (reader.info().width, reader.info().height);
    dimensions(w, h)?;
    if reader.info().animation_control.is_some() {
        return Err(error("unsupported"));
    }
    let len = reader
        .output_buffer_size()
        .ok_or_else(|| error("dimensions"))?;
    if len > 64_000_000 {
        return Err(error("dimensions"));
    }
    let mut pixels = vec![0; len];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|_| error("damaged"))?;
    if frame.bit_depth != png::BitDepth::Eight {
        return Err(error("unsupported"));
    }
    let alpha = match frame.color_type {
        png::ColorType::Rgba => pixels[..frame.buffer_size()]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels[..frame.buffer_size()]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| p[1])
            .collect(),
        png::ColorType::Rgb | png::ColorType::Grayscale => vec![255; (w * h) as usize],
        _ => return Err(error("unsupported")),
    };
    reader.finish().map_err(|_| error("damaged"))?;
    normalize(alpha, w, h, true)
}
fn normalize(alpha: Vec<u8>, w: u32, h: u32, is_png: bool) -> Result<Mask, AppError> {
    let mut left = w;
    let mut top = h;
    let mut right = 0;
    let mut bottom = 0;
    for (i, &a) in alpha.iter().enumerate() {
        if a != 0 {
            let x = i as u32 % w;
            let y = i as u32 / w;
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right == 0 {
        return Err(error("empty"));
    }
    let bw = right - left;
    let bh = bottom - top;
    let factor = (1024.0 / f64::from(bw.max(bh))).min(1.0);
    let ow = (f64::from(bw) * factor).round().max(1.0) as u32;
    let oh = (f64::from(bh) * factor).round().max(1.0) as u32;
    let mut out = Vec::with_capacity((ow * oh * 2) as usize);
    for y in 0..oh {
        for x in 0..ow {
            // Area sampling preserves thin Alpha features while reducing large inputs.
            let x0 = f64::from(x) * f64::from(bw) / f64::from(ow);
            let x1 = f64::from(x + 1) * f64::from(bw) / f64::from(ow);
            let y0 = f64::from(y) * f64::from(bh) / f64::from(oh);
            let y1 = f64::from(y + 1) * f64::from(bh) / f64::from(oh);
            let mut sum = 0.0;
            for sy in y0.floor() as u32..(y1.ceil() as u32).min(bh) {
                for sx in x0.floor() as u32..(x1.ceil() as u32).min(bw) {
                    let weight = (x1.min(f64::from(sx + 1)) - x0.max(f64::from(sx)))
                        * (y1.min(f64::from(sy + 1)) - y0.max(f64::from(sy)));
                    sum += f64::from(alpha[((sy + top) * w + sx + left) as usize]) * weight;
                }
            }
            out.extend_from_slice(&[255, (sum / ((x1 - x0) * (y1 - y0))).round() as u8]);
        }
    }
    if !out.as_chunks::<2>().0.iter().any(|p| p[1] != 0) {
        return Err(error("empty"));
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, ow, oh);
        encoder.set_color(png::ColorType::GrayscaleAlpha);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|_| error("damaged"))?;
        writer
            .write_image_data(&out)
            .map_err(|_| error("damaged"))?;
        writer.finish().map_err(|_| error("damaged"))?;
    }
    Ok(Mask {
        png: STANDARD.encode(encoded),
        width: ow,
        height: oh,
        original_width: w,
        original_height: h,
        bounds: [left, top, bw, bh],
        opaque: is_png && alpha.iter().all(|&a| a == 255),
    })
}

fn svg_mask(bytes: &[u8]) -> Result<Mask, AppError> {
    let text = std::str::from_utf8(bytes).map_err(|_| error("damaged"))?;
    let doc = roxmltree::Document::parse(text).map_err(|_| error("damaged"))?;
    let (width, height) = validate_svg(&doc)?;
    let options = usvg::Options {
        resources_dir: None,
        default_size: usvg::Size::from_wh(width as f32, height as f32)
            .ok_or_else(|| error("dimensions"))?,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(text, &options).map_err(|_| error("damaged"))?;
    let size = tree.size();
    // Crop before shrinking so transparent margins cannot erase narrow features.
    let (rw, rh) = (width, height);
    let transform =
        tiny_skia::Transform::from_scale(rw as f32 / size.width(), rh as f32 / size.height());
    check_layers(
        tree.root(),
        transform,
        &mut (u64::from(rw) * u64::from(rh)),
        (rw as f64 * 5.0, rh as f64 * 5.0),
        None,
    )?;
    let mut pixmap = tiny_skia::Pixmap::new(rw, rh).ok_or_else(|| error("dimensions"))?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let alpha = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[3])
        .collect();
    drop(pixmap);
    normalize(alpha, rw, rh, false)
}

fn check_layers(
    group: &usvg::Group,
    parent: tiny_skia::Transform,
    pixels: &mut u64,
    max: (f64, f64),
    clip_layer: Option<u64>,
) -> Result<(), AppError> {
    let transform = parent.pre_concat(group.transform());
    let mut layer = 0;
    if group.should_isolate() {
        let bbox = group
            .layer_bounding_box()
            .to_rect()
            .transform(transform)
            .ok_or_else(|| error("complexity"))?;
        let width = (f64::from(bbox.width()).ceil() + 4.0).min(max.0);
        let height = (f64::from(bbox.height()).ceil() + 4.0).min(max.1);
        layer = ((width * height) as u64).max(clip_layer.unwrap_or(0));
        *pixels += layer;
    }
    if *pixels > 64_000_000 {
        return Err(error("complexity"));
    }
    if let Some(mask) = group.mask() {
        let mut current = Some(mask);
        while let Some(mask) = current {
            *pixels += layer * 3;
            if *pixels > 64_000_000 {
                return Err(error("complexity"));
            }
            check_layers(mask.root(), transform, pixels, max, None)?;
            current = mask.mask();
        }
    }
    if let Some(clip) = group.clip_path() {
        let mut current = Some(clip);
        while let Some(clip) = current {
            *pixels += layer * 2;
            if *pixels > 64_000_000 {
                return Err(error("complexity"));
            }
            check_layers(clip.root(), transform, pixels, max, Some(layer))?;
            current = clip.clip_path();
        }
    }
    for child in group.children() {
        if let usvg::Node::Group(child) = child {
            check_layers(child, transform, pixels, max, clip_layer)?;
        }
    }
    Ok(())
}

fn validate_svg(doc: &roxmltree::Document<'_>) -> Result<(u32, u32), AppError> {
    let root = doc.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NS) {
        return Err(error("unsupported"));
    }
    let elements: Vec<_> = doc.descendants().filter(|n| n.is_element()).collect();
    if elements.len() > NODE_LIMIT {
        return Err(error("complexity"));
    }
    let mut ids = HashMap::new();
    let mut refs = HashMap::<roxmltree::NodeId, Vec<(String, &'static str)>>::new();
    let mut paths = 0usize;
    let mut weights = HashMap::new();
    for node in doc.descendants() {
        if node.is_pi() {
            return Err(error("unsupported"));
        }
        if node.is_text()
            && !node.text().unwrap_or("").trim().is_empty()
            && !node
                .parent()
                .is_some_and(|p| matches!(p.tag_name().name(), "title" | "desc"))
        {
            return Err(error("unsupported"));
        }
        if !node.is_element() {
            continue;
        }
        if node.ancestors().filter(|n| n.is_element()).count() > 64 {
            return Err(error("complexity"));
        }
        let tag = node.tag_name();
        if tag.namespace() != Some(SVG_NS)
            || !matches!(
                tag.name(),
                "svg"
                    | "g"
                    | "defs"
                    | "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "linearGradient"
                    | "radialGradient"
                    | "stop"
                    | "clipPath"
                    | "mask"
                    | "use"
                    | "symbol"
                    | "title"
                    | "desc"
            )
        {
            return Err(error("unsupported"));
        }
        for attr in node.attributes() {
            if attr
                .namespace()
                .is_some_and(|ns| ns != XLINK_NS || attr.name() != "href")
            {
                return Err(error("unsupported"));
            }
            let value = attr.value().trim();
            match attr.name() {
                "id" => {
                    if value.is_empty() || ids.insert(value.to_owned(), node).is_some() {
                        return Err(error("reference"));
                    }
                }
                "version" if node == root && matches!(value, "1.0" | "1.1" | "2.0") => {}
                "style" => {
                    if value.contains(['\\', '/', '@', '(', ')']) {
                        return Err(error("unsupported"));
                    }
                    for declaration in value.split(';').filter(|d| !d.trim().is_empty()) {
                        let (key, v) = declaration
                            .split_once(':')
                            .ok_or_else(|| error("unsupported"))?;
                        property(key.trim(), v.trim(), node.id(), &mut refs)?;
                    }
                }
                "d" if tag.name() == "path" => {
                    let before = paths;
                    for segment in svgtypes::PathParser::from(value) {
                        let segment = segment.map_err(|_| error("damaged"))?;
                        if !finite_segment(segment) {
                            return Err(error("damaged"));
                        }
                        paths += 1;
                        if paths > 200_000 {
                            return Err(error("complexity"));
                        }
                    }
                    weights.insert(node.id(), paths - before);
                }
                "points" if matches!(tag.name(), "polyline" | "polygon") => {
                    let mut count = 0;
                    for number in svgtypes::NumberListParser::from(value) {
                        let n = number.map_err(|_| error("damaged"))?;
                        if !n.is_finite() || n.abs() > 1e12 {
                            return Err(error("damaged"));
                        }
                        count += 1;
                        if count > 400_000 {
                            return Err(error("complexity"));
                        }
                    }
                    if count % 2 != 0 {
                        return Err(error("damaged"));
                    }
                    weights.insert(node.id(), count / 2);
                }
                "href" if matches!(tag.name(), "use" | "linearGradient" | "radialGradient") => {
                    refs.entry(node.id()).or_default().push((
                        fragment(value)?.to_owned(),
                        if tag.name() == "use" {
                            "use"
                        } else {
                            "gradient"
                        },
                    ));
                }
                "viewBox" if matches!(tag.name(), "svg" | "symbol") => {
                    let v = svgtypes::ViewBox::from_str(value).map_err(|_| error("dimensions"))?;
                    if ![v.x, v.y, v.w, v.h]
                        .iter()
                        .all(|n| n.is_finite() && n.abs() <= 1e12)
                    {
                        return Err(error("dimensions"));
                    }
                }
                "preserveAspectRatio" if matches!(tag.name(), "svg" | "symbol") => {
                    svgtypes::AspectRatio::from_str(value).map_err(|_| error("unsupported"))?;
                }
                "x" | "y" | "x1" | "x2" | "y1" | "y2" | "cx" | "cy" | "r" | "rx" | "ry" | "fx"
                | "fy" | "width" | "height" => {
                    let l = svgtypes::Length::from_str(value).map_err(|_| error("damaged"))?;
                    if !l.number.is_finite()
                        || l.number.abs() > 1e12
                        || matches!(l.unit, svgtypes::LengthUnit::Em | svgtypes::LengthUnit::Ex)
                    {
                        return Err(error("unsupported"));
                    }
                    if matches!(attr.name(), "r" | "rx" | "ry" | "width" | "height")
                        && l.number < 0.0
                    {
                        return Err(error("damaged"));
                    }
                }
                "gradientUnits" | "clipPathUnits" | "maskUnits" | "maskContentUnits"
                    if matches!(value, "userSpaceOnUse" | "objectBoundingBox") => {}
                "spreadMethod" if matches!(value, "pad" | "reflect" | "repeat") => {}
                "gradientTransform" | "transform" => {
                    let t = svgtypes::Transform::from_str(value).map_err(|_| error("damaged"))?;
                    if ![t.a, t.b, t.c, t.d, t.e, t.f]
                        .iter()
                        .all(|n| n.is_finite() && n.abs() <= 1e12)
                    {
                        return Err(error("damaged"));
                    }
                }
                "offset" if tag.name() == "stop" => {
                    number(value, true)?;
                }
                key => property(key, value, node.id(), &mut refs)?,
            }
        }
    }
    for links in refs.values() {
        for (link, kind) in links {
            let target = ids.get(link).ok_or_else(|| error("reference"))?;
            let target_tag = target.tag_name().name();
            let valid = match *kind {
                "use" => matches!(
                    target_tag,
                    "g" | "path"
                        | "rect"
                        | "circle"
                        | "ellipse"
                        | "line"
                        | "polyline"
                        | "polygon"
                        | "use"
                        | "symbol"
                        | "svg"
                ),
                "paint" | "gradient" => matches!(target_tag, "linearGradient" | "radialGradient"),
                "mask" => target_tag == "mask",
                "clip" => target_tag == "clipPath",
                _ => false,
            };
            if !valid {
                return Err(error("reference"));
            }
        }
    }
    expansion(
        root,
        &ids,
        &refs,
        &mut HashSet::new(),
        &weights,
        &mut (0, 0),
        svgtypes::Transform::default(),
    )?;
    svg_dimensions(root)
}
fn finite_segment(segment: svgtypes::PathSegment) -> bool {
    use svgtypes::PathSegment::*;
    match segment {
        MoveTo { x, y, .. } | LineTo { x, y, .. } | SmoothQuadratic { x, y, .. } => {
            [x, y].iter().all(|n| n.is_finite() && n.abs() <= 1e12)
        }
        HorizontalLineTo { x, .. } => x.is_finite() && x.abs() <= 1e12,
        VerticalLineTo { y, .. } => y.is_finite() && y.abs() <= 1e12,
        CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
            ..
        } => [x1, y1, x2, y2, x, y]
            .iter()
            .all(|n| n.is_finite() && n.abs() <= 1e12),
        SmoothCurveTo { x2, y2, x, y, .. }
        | Quadratic {
            x1: x2,
            y1: y2,
            x,
            y,
            ..
        } => [x2, y2, x, y]
            .iter()
            .all(|n| n.is_finite() && n.abs() <= 1e12),
        EllipticalArc {
            rx,
            ry,
            x_axis_rotation,
            x,
            y,
            ..
        } => [rx, ry, x_axis_rotation, x, y]
            .iter()
            .all(|n| n.is_finite() && n.abs() <= 1e12),
        ClosePath { .. } => true,
    }
}
fn fragment(value: &str) -> Result<&str, AppError> {
    value
        .strip_prefix('#')
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_whitespace))
        .ok_or_else(|| error("reference"))
}
fn number(value: &str, percent: bool) -> Result<f64, AppError> {
    let value = if percent {
        value.strip_suffix('%').unwrap_or(value)
    } else {
        value
    };
    value
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && n.abs() <= 1e12)
        .ok_or_else(|| error("damaged"))
}
fn property(
    key: &str,
    value: &str,
    id: roxmltree::NodeId,
    refs: &mut HashMap<roxmltree::NodeId, Vec<(String, &'static str)>>,
) -> Result<(), AppError> {
    match key {
        "fill" | "stroke" => {
            match svgtypes::Paint::from_str(value).map_err(|_| error("unsupported"))? {
                svgtypes::Paint::FuncIRI(link, _) => {
                    refs.entry(id).or_default().push((link.to_owned(), "paint"));
                }
                svgtypes::Paint::ContextFill | svgtypes::Paint::ContextStroke => {
                    return Err(error("unsupported"));
                }
                _ => {}
            }
        }
        "clip-path" | "mask" => {
            if value != "none" {
                let inner = value
                    .strip_prefix("url(")
                    .and_then(|v| v.strip_suffix(')'))
                    .ok_or_else(|| error("unsupported"))?
                    .trim()
                    .trim_matches(['\'', '"']);
                refs.entry(id).or_default().push((
                    fragment(inner)?.to_owned(),
                    if key == "mask" { "mask" } else { "clip" },
                ));
            }
        }
        "color" | "stop-color" => {
            if value != "currentColor" {
                svgtypes::Color::from_str(value).map_err(|_| error("unsupported"))?;
            }
        }
        "opacity" | "fill-opacity" | "stroke-opacity" | "stop-opacity" => {
            number(value, true)?;
        }
        "stroke-width" | "stroke-dashoffset" => {
            let n = svgtypes::Length::from_str(value).map_err(|_| error("unsupported"))?;
            if !n.number.is_finite()
                || n.number.abs() > 1e12
                || matches!(n.unit, svgtypes::LengthUnit::Em | svgtypes::LengthUnit::Ex)
            {
                return Err(error("unsupported"));
            }
        }
        "stroke-miterlimit" => {
            if number(value, false)? < 1.0 {
                return Err(error("damaged"));
            }
        }
        "stroke-dasharray" => {
            if value != "none" {
                return Err(error("unsupported"));
            }
        }
        "fill-rule" | "clip-rule" if matches!(value, "nonzero" | "evenodd") => {}
        "stroke-linecap" if matches!(value, "butt" | "round" | "square") => {}
        "stroke-linejoin" if matches!(value, "miter" | "round" | "bevel") => {}
        "display" if matches!(value, "none" | "inline") => {}
        "visibility" if matches!(value, "visible" | "hidden" | "collapse") => {}
        "shape-rendering"
            if matches!(
                value,
                "auto" | "optimizeSpeed" | "crispEdges" | "geometricPrecision"
            ) => {}
        "mask-type" if matches!(value, "luminance" | "alpha") => {}
        "color-interpolation" if value == "sRGB" => {}
        _ => return Err(error("unsupported")),
    }
    Ok(())
}
fn expansion<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    ids: &HashMap<String, roxmltree::Node<'a, 'input>>,
    refs: &HashMap<roxmltree::NodeId, Vec<(String, &'static str)>>,
    active: &mut HashSet<roxmltree::NodeId>,
    weights: &HashMap<roxmltree::NodeId, usize>,
    total: &mut (usize, usize),
    parent: svgtypes::Transform,
) -> Result<(), AppError> {
    total.0 += 1;
    total.1 += weights.get(&node.id()).copied().unwrap_or(0);
    if total.0 > NODE_LIMIT || total.1 > 200_000 {
        return Err(error("complexity"));
    }
    if active.len() >= 64 || !active.insert(node.id()) {
        return Err(error("reference"));
    }
    let local = node
        .attribute("transform")
        .map(svgtypes::Transform::from_str)
        .transpose()
        .map_err(|_| error("damaged"))?
        .unwrap_or_default();
    let combined = svgtypes::Transform {
        a: parent.a * local.a + parent.c * local.b,
        b: parent.b * local.a + parent.d * local.b,
        c: parent.a * local.c + parent.c * local.d,
        d: parent.b * local.c + parent.d * local.d,
        e: parent.a * local.e + parent.c * local.f + parent.e,
        f: parent.b * local.e + parent.d * local.f + parent.f,
    };
    if ![
        combined.a, combined.b, combined.c, combined.d, combined.e, combined.f,
    ]
    .iter()
    .all(|v| v.is_finite() && v.abs() <= 1e12)
    {
        return Err(error("complexity"));
    }
    for child in node.children().filter(|n| n.is_element()) {
        expansion(child, ids, refs, active, weights, total, combined)?;
    }
    if let Some(links) = refs.get(&node.id()) {
        for (link, _) in links {
            expansion(
                *ids.get(link).ok_or_else(|| error("reference"))?,
                ids,
                refs,
                active,
                weights,
                total,
                combined,
            )?;
        }
    }
    active.remove(&node.id());
    Ok(())
}
fn svg_dimensions(root: roxmltree::Node<'_, '_>) -> Result<(u32, u32), AppError> {
    fn pixels(value: &str) -> Result<f64, AppError> {
        let l = svgtypes::Length::from_str(value).map_err(|_| error("dimensions"))?;
        let n = l.number
            * match l.unit {
                svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px => 1.0,
                svgtypes::LengthUnit::In => 96.0,
                svgtypes::LengthUnit::Cm => 96.0 / 2.54,
                svgtypes::LengthUnit::Mm => 96.0 / 25.4,
                svgtypes::LengthUnit::Pt => 96.0 / 72.0,
                svgtypes::LengthUnit::Pc => 16.0,
                _ => return Err(error("unsupported")),
            };
        if !n.is_finite() || n <= 0.0 || n > 4096.0 {
            return Err(error("dimensions"));
        }
        Ok(n)
    }
    let width = root.attribute("width").map(pixels).transpose()?;
    let height = root.attribute("height").map(pixels).transpose()?;
    let vb = root
        .attribute("viewBox")
        .map(svgtypes::ViewBox::from_str)
        .transpose()
        .map_err(|_| error("dimensions"))?;
    let (w, h) = match (width, height, vb) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some(v)) => (w, w * v.h / v.w),
        (None, Some(h), Some(v)) => (h * v.w / v.h, h),
        (None, None, Some(v)) => {
            let f = 1024.0 / v.w.max(v.h);
            (v.w * f, v.h * f)
        }
        _ => return Err(error("dimensions")),
    };
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 || w > 4096.0 || h > 4096.0 {
        return Err(error("dimensions"));
    }
    let (w, h) = (w.ceil() as u32, h.ceil() as u32);
    dimensions(w, h)?;
    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn svg(body: &str) -> Vec<u8> {
        format!("<svg xmlns=\"{SVG_NS}\" width=\"16\" height=\"16\">{body}</svg>").into_bytes()
    }
    fn png_bytes(w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![];
        {
            let mut e = png::Encoder::new(&mut bytes, w, h);
            e.set_color(color);
            e.set_depth(png::BitDepth::Eight);
            let mut out = e.write_header().unwrap();
            out.write_image_data(data).unwrap();
        }
        bytes
    }
    fn reason(result: Result<Mask, AppError>) -> String {
        result.unwrap_err().params["reason"]
            .as_str()
            .unwrap()
            .to_string()
    }
    #[test]
    fn file_budget_and_special_file_reads_are_bounded() {
        let path = std::env::temp_dir()
            .join(crate::targets::next_id("asset-test"))
            .with_extension("png");
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(FILE_LIMIT as u64).unwrap();
        assert_eq!(reason(load(&path)), "damaged");
        file.set_len(FILE_LIMIT as u64 + 1).unwrap();
        assert_eq!(reason(load(&path)), "size");
        std::fs::remove_file(&path).unwrap();
        let cpath = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
        assert_eq!(reason(load(&path)), "type");
    }
    #[test]
    fn palette_transparency_and_animation_are_checked() {
        let mut bytes = vec![];
        {
            let mut e = png::Encoder::new(&mut bytes, 2, 1);
            e.set_color(png::ColorType::Indexed);
            e.set_depth(png::BitDepth::Eight);
            e.set_palette(vec![255, 255, 255, 0, 0, 0]);
            e.set_trns(vec![0, 128]);
            e.write_header().unwrap().write_image_data(&[0, 1]).unwrap();
        }
        let m = png_mask(&bytes).unwrap();
        assert_eq!(m.bounds, [1, 0, 1, 1]);
        assert!(!m.opaque);
        let mut bytes = vec![];
        {
            let mut e = png::Encoder::new(&mut bytes, 1, 1);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.set_animated(1, 0).unwrap();
            e.write_header()
                .unwrap()
                .write_image_data(&[0, 0, 0, 255])
                .unwrap();
        }
        assert_eq!(reason(png_mask(&bytes)), "unsupported");
    }
    #[test]
    fn png_alpha_is_cropped_without_color_keying() {
        let mut rgba = vec![255; 4 * 4 * 4];
        for p in rgba.as_chunks_mut::<4>().0 {
            p[3] = 0;
        }
        rgba[(1 + 4) * 4 + 3] = 128;
        rgba[(2 + 8) * 4 + 3] = 255;
        let m = png_mask(&png_bytes(4, 4, png::ColorType::Rgba, &rgba)).unwrap();
        assert_eq!(m.bounds, [1, 1, 2, 2]);
        assert_eq!((m.width, m.height), (2, 2));
        assert!(!m.opaque);
        let encoded = STANDARD.decode(m.png).unwrap();
        let mut reader = png::Decoder::new(Cursor::new(encoded)).read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut data).unwrap();
        assert_eq!(data, [255, 128, 255, 0, 255, 0, 255, 255]);
        assert!(
            png_mask(&png_bytes(2, 2, png::ColorType::Rgb, &[255; 12]))
                .unwrap()
                .opaque
        );
        assert_eq!(
            reason(png_mask(&png_bytes(
                1,
                1,
                png::ColorType::GrayscaleAlpha,
                &[255, 0]
            ))),
            "empty"
        );
    }
    #[test]
    fn dimensions_and_downsampling_are_bounded() {
        assert!(dimensions(4096, 3906).is_ok());
        assert!(dimensions(4096, 3907).is_err());
        assert!(dimensions(4097, 1).is_err());
        let m = normalize(vec![128; 2048 * 2], 2048, 2, true).unwrap();
        assert_eq!((m.width, m.height), (1024, 1));
        assert_eq!(m.bounds, [0, 0, 2048, 2]);
        assert_eq!(reason(png_mask(b"broken")), "damaged");
    }
    #[test]
    fn static_svg_supports_gradients_references_and_luminance_masks() {
        let m=svg_mask(&svg("<defs><linearGradient id=\"g\"><stop offset=\"0\" stop-color=\"white\"/><stop offset=\"1\" stop-color=\"black\"/></linearGradient><mask id=\"m\"><rect width=\"16\" height=\"16\" fill=\"url(#g)\"/></mask><path id=\"p\" d=\"M0 0H16V16H0Z\"/></defs><use href=\"#p\" mask=\"url(#m)\"/>" )).unwrap();
        assert!(!m.opaque);
        assert_eq!(m.width, 16);
        let m=svg_mask(b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 20 10\"><rect width=\"20\" height=\"10\"/></svg>").unwrap();
        assert_eq!((m.width, m.height), (1024, 512));
        assert!(
            svg_mask(&svg(
                "<rect width=\"16\" height=\"16\" style=\"fill: red; opacity: .5\"/>"
            ))
            .is_ok()
        );
    }
    #[test]
    fn dangerous_or_unsupported_svg_is_rejected_atomically() {
        for body in [
            "<path id=\"p\" d=\"M0 0H1V1Z\"/><rect width=\"16\" height=\"16\"/><rect width=\"1\" height=\"1\" fill=\"url(#p)\"/>",
            "<script/>",
            "<image href=\"file:///tmp/private\"/>",
            "<image href=\"data:image/png;base64,AA==\"/>",
            "<foreignObject/>",
            "<animate/>",
            "<text>text</text>",
            "<filter/>",
            "<style>rect{fill:red}</style>",
            "<rect width=\"16\" height=\"16\" onclick=\"run()\"/>",
            "<rect width=\"16\" height=\"16\" fill=\"url(https://example.com/x)\"/>",
            "<rect width=\"16\" height=\"16\" style=\"filter: blur(1px)\"/>",
            "<rect width=\"16\" height=\"16\" unknown=\"1\"/>",
            "<path d=\"M 0 bad\"/>",
            "<path d=\"M0 0L16 16\" stroke=\"black\" stroke-dasharray=\".001 .001\"/>",
            "<use href=\"#missing\"/>",
            "<use id=\"loop\" href=\"#loop\"/>",
            "<g id=\"a\"><use href=\"#a\"/></g>",
            "<g id=\"a\"/><g id=\"a\"/>",
        ] {
            assert!(svg_mask(&svg(body)).is_err(), "accepted {body}");
        }
        assert!(svg_mask(b"<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///tmp/private'>]><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\">&x;</svg>").is_err());
        assert!(svg_mask(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"10\" height=\"10\"/></svg>").is_err());
        assert!(
            svg_mask(b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4097\" height=\"1\"/>")
                .is_err()
        );
    }
    #[test]
    fn svg_crops_transparent_margins_before_shrinking_narrow_shapes() {
        let m = svg_mask(br#"<svg xmlns="http://www.w3.org/2000/svg" width="4096" height="1024"><rect x="100" width="1" height="512"/><rect x="102" width="1" height="512"/></svg>"#).unwrap();
        assert_eq!(m.bounds, [100, 0, 3, 512]);
        assert_eq!((m.width, m.height), (3, 512));
        let encoded = STANDARD.decode(m.png).unwrap();
        let mut reader = png::Decoder::new(Cursor::new(encoded)).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        assert_eq!(&pixels[..6], &[255, 255, 255, 0, 255, 255]);
    }
    #[test]
    fn svg_expanded_geometry_and_intermediate_layers_are_bounded() {
        let path = format!("M0 0{}", "l1 0".repeat(50_000));
        let body = format!(
            "<defs><path id=\"p\" d=\"{path}\"/></defs>{}",
            "<use href=\"#p\"/>".repeat(4)
        );
        assert_eq!(reason(svg_mask(&svg(&body))), "complexity");
        let polygon = format!("<polygon points=\"{}\"/>", "0,0 ".repeat(70_000));
        assert_eq!(reason(svg_mask(&svg(&polygon.repeat(3)))), "complexity");
        let layers = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4096" height="1024"><g opacity=".5"><rect width="100000" height="100000"/></g></svg>"#;
        assert_eq!(reason(svg_mask(layers)), "complexity");
        let wide_layer = br#"<svg xmlns="http://www.w3.org/2000/svg" width="3000" height="1000"><g opacity=".5"><rect x="-6000" y="-2000" width="15000" height="5000"/></g></svg>"#;
        assert_eq!(reason(svg_mask(wide_layer)), "complexity");
    }
    #[test]
    fn svg_complexity_budgets_bound_expansion_and_depth() {
        assert_eq!(
            reason(svg_mask(&svg(
                "<rect width=\"1\" height=\"1\"/><g transform=\"scale(1e12)\"><g transform=\"scale(1e12)\"><rect width=\"1\" height=\"1\"/></g></g>"
            ))),
            "complexity"
        );
        assert_eq!(
            reason(svg_mask(&svg(&"<g/>".repeat(NODE_LIMIT)))),
            "complexity"
        );
        let deep = format!(
            "{}<rect width=\"1\" height=\"1\"/>{}",
            "<g>".repeat(64),
            "</g>".repeat(64)
        );
        assert!(svg_mask(&svg(&deep)).is_err());
        let mut many = String::from("<defs><g id=\"g0\"><rect width=\"1\" height=\"1\"/></g>");
        for n in 1..16 {
            many += &format!(
                "<g id=\"g{n}\"><use href=\"#g{}\"/><use href=\"#g{}\"/></g>",
                n - 1,
                n - 1
            );
        }
        many += "</defs><use href=\"#g15\"/>";
        assert_eq!(reason(svg_mask(&svg(&many))), "complexity");
    }
}
