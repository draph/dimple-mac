use crate::{assets::Mask, localization::AppError};
use objc2_core_foundation::{CFAttributedString, CFDictionary, CFRange, CFString};
use objc2_core_graphics::{CGBitmapContextCreate, CGContext, CGImageAlphaInfo};
use objc2_core_text::{CTFont, CTFontUIFontType, CTLine, CTRun, kCTFontAttributeName};
use objc2_foundation::NSString;
use std::ptr::NonNull;

pub const BYTE_LIMIT: usize = 4096;
fn error(reason: &str) -> AppError {
    AppError::reason("text_error", reason)
}

pub fn count(text: &str) -> Result<usize, AppError> {
    if text.len() > BYTE_LIMIT {
        return Err(error("budget"));
    }
    let string = NSString::from_str(text);
    let mut index = 0;
    let mut count = 0;
    while index < string.length() {
        let range = string.rangeOfComposedCharacterSequenceAtIndex(index);
        index = range.location + range.length;
        count += 1;
    }
    Ok(count)
}

pub fn validate(text: &str) -> Result<usize, AppError> {
    let count = count(text)?;
    if text
        .chars()
        .any(|c| matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'))
    {
        return Err(error("line"));
    }
    if text.trim().is_empty() {
        return Err(error("empty"));
    }
    if !(1..=4).contains(&count) {
        return Err(error("length"));
    }
    if text.chars().any(char::is_control) {
        return Err(error("control"));
    }
    Ok(count)
}

pub fn render(text: &str) -> Result<Mask, AppError> {
    validate(text)?;
    // Core Text owns fallback and shaping; inspect the final runs, not a character-to-glyph ratio.
    unsafe {
        let font = CTFont::new_ui_font_for_language(CTFontUIFontType::System, 256.0, None)
            .ok_or_else(|| error("render"))?;
        let attributes = CFDictionary::from_slices(&[kCTFontAttributeName], &[&*font]);
        let string = CFString::from_str(text);
        let attributed = CFAttributedString::new(None, Some(&string), Some(attributes.as_opaque()))
            .ok_or_else(|| error("render"))?;
        let line = CTLine::with_attributed_string(&attributed);
        let runs = line.glyph_runs();
        for i in 0..runs.count() {
            let run = &*runs.value_at_index(i).cast::<CTRun>();
            let attributes = run.attributes();
            let font = attributes
                .value((kCTFontAttributeName as *const CFString).cast())
                .cast::<CTFont>();
            if font.is_null()
                || (*font)
                    .post_script_name()
                    .to_string()
                    .contains("LastResort")
            {
                return Err(error("missing"));
            }
            let n = run.glyph_count();
            if !(0..=4096).contains(&n) {
                return Err(error("budget"));
            }
            if n > 0 {
                let mut glyphs = vec![0; n as usize];
                run.glyphs(
                    CFRange::new(0, n),
                    NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                );
                if glyphs.contains(&0) {
                    return Err(error("missing"));
                }
            }
        }
        let bounds = line.image_bounds(None);
        let width = bounds.size.width;
        let height = bounds.size.height;
        if ![width, height, bounds.origin.x, bounds.origin.y]
            .iter()
            .all(|n| n.is_finite())
            || width <= 0.0
            || height <= 0.0
        {
            return Err(error("empty"));
        }
        let scale = 992.0 / width.max(height);
        let w = (width * scale).ceil() as usize + 32;
        let h = (height * scale).ceil() as usize + 32;
        let mut alpha = vec![0u8; w * h];
        let context = CGBitmapContextCreate(
            alpha.as_mut_ptr().cast(),
            w,
            h,
            8,
            w,
            None,
            CGImageAlphaInfo::Only.0,
        )
        .ok_or_else(|| error("render"))?;
        CGContext::translate_ctm(
            Some(&context),
            16.0 - bounds.origin.x * scale,
            16.0 - bounds.origin.y * scale,
        );
        CGContext::scale_ctm(Some(&context), scale, scale);
        CGContext::set_text_position(Some(&context), 0.0, 0.0);
        line.draw(&context);
        drop(context);
        if alpha[..w]
            .iter()
            .chain(&alpha[(h - 1) * w..])
            .any(|&a| a != 0)
            || alpha
                .chunks_exact(w)
                .any(|row| row[0] != 0 || row[w - 1] != 0)
        {
            return Err(error("render"));
        }
        crate::assets::normalize(alpha, w as u32, h as u32, false).map_err(|_| error("empty"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_composed_characters_and_validates_without_truncating() {
        for (text, expected) in [
            ("中文", 2),
            ("a\u{301}", 1),
            ("👨‍👩‍👧‍👦", 1),
            ("🇨🇳", 1),
            ("A中12", 4),
        ] {
            assert_eq!(validate(text).unwrap(), expected);
        }
        for (text, reason) in [
            ("", "empty"),
            (" \u{3000}", "empty"),
            ("12345", "length"),
            ("a\nb", "line"),
            ("a\r", "line"),
            ("a\u{2028}", "line"),
            ("a\tb", "control"),
        ] {
            assert_eq!(validate(text).unwrap_err().params["reason"], reason);
        }
        assert_eq!(
            validate(&"a".repeat(BYTE_LIMIT + 1)).unwrap_err().params["reason"],
            "budget"
        );
    }
    #[test]
    fn renders_system_fallback_and_rejects_missing_glyphs() {
        for text in ["中文", "ABCD", "1234", "A中12", "e\u{301}", "F", "g"] {
            let mask = objc2::rc::autoreleasepool(|_| render(text)).unwrap();
            assert!((1..=1024).contains(&mask.width));
            assert!((1..=1024).contains(&mask.height));
            assert!(!mask.opaque);
            assert!(
                mask.bounds[0] > 0 && mask.bounds[1] > 0,
                "{text}: {:?} {}x{}",
                mask.bounds,
                mask.original_width,
                mask.original_height
            );
            assert!(mask.bounds[0] + mask.bounds[2] < mask.original_width);
            assert!(mask.bounds[1] + mask.bounds[3] < mask.original_height);
        }
        assert_eq!(
            render("\u{10ffff}")
                .expect_err("missing glyph must fail")
                .params["reason"],
            "missing"
        );
    }
    #[test]
    fn asymmetric_glyph_is_upright_in_the_encoded_mask() {
        use base64::Engine;
        let mask = render("F").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(mask.png)
            .unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        let row = mask.width as usize * 2;
        let coverage = |y: usize| {
            pixels[y * row..(y + 1) * row]
                .as_chunks::<2>()
                .0
                .iter()
                .filter(|p| p[1] > 0)
                .count()
        };
        assert!(coverage(mask.height as usize / 20) > 2 * coverage(mask.height as usize * 9 / 10));
    }
}
