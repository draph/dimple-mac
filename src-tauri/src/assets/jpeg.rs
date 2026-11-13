use super::{Mask, dimensions, error, normalize_rgba};
use crate::localization::AppError;
use objc2_core_foundation::{
    CFData, CFDictionary, CFNumber, CFNumberType, CFRetained, CFString, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, kCGColorSpaceSRGB,
};
use std::{
    ffi::c_void,
    ptr::{NonNull, null},
};

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageSourceCreateWithData(data: &CFData, options: *const c_void) -> *const c_void;
    fn CGImageSourceGetStatus(source: *const c_void) -> i32;
    fn CGImageSourceGetCount(source: *const c_void) -> usize;
    fn CGImageSourceGetStatusAtIndex(source: *const c_void, index: usize) -> i32;
    fn CGImageSourceCopyPropertiesAtIndex(
        source: *const c_void,
        index: usize,
        options: *const c_void,
    ) -> *mut CFDictionary;
    fn CGImageSourceCreateImageAtIndex(
        source: *const c_void,
        index: usize,
        options: *const c_void,
    ) -> *mut CGImage;
    static kCGImagePropertyColorModel: &'static CFString;
    static kCGImagePropertyPixelWidth: &'static CFString;
    static kCGImagePropertyPixelHeight: &'static CFString;
    static kCGImagePropertyOrientation: &'static CFString;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}
struct Source(*const c_void);
impl Drop for Source {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<Mask, AppError> {
    if !complete_jpeg(bytes) {
        return Err(error("damaged"));
    }
    // Read metadata and enforce budgets before asking ImageIO to decode pixels.
    unsafe {
        let data = CFData::from_bytes(bytes);
        let source = CGImageSourceCreateWithData(&data, null());
        if source.is_null() {
            return Err(error("damaged"));
        }
        let source = Source(source);
        if CGImageSourceGetStatus(source.0) != 0 {
            return Err(error("damaged"));
        }
        if CGImageSourceGetCount(source.0) != 1 {
            return Err(error("unsupported"));
        }
        let properties = NonNull::new(CGImageSourceCopyPropertiesAtIndex(source.0, 0, null()))
            .map(|p| CFRetained::from_raw(p))
            .ok_or_else(|| error("damaged"))?;
        let model = properties
            .value((kCGImagePropertyColorModel as *const CFString).cast())
            .cast::<CFString>();
        if model.is_null() || !matches!((*model).to_string().as_str(), "RGB" | "Gray") {
            return Err(error("unsupported"));
        }
        let number = |key: &CFString| -> Result<Option<i64>, AppError> {
            let p = properties
                .value((key as *const CFString).cast())
                .cast::<CFNumber>();
            if p.is_null() {
                return Ok(None);
            }
            let mut n = 0i64;
            if !(*p).value(CFNumberType::SInt64Type, (&mut n as *mut i64).cast()) {
                return Err(error("damaged"));
            }
            Ok(Some(n))
        };
        let width = number(kCGImagePropertyPixelWidth)?.ok_or_else(|| error("damaged"))?;
        let height = number(kCGImagePropertyPixelHeight)?.ok_or_else(|| error("damaged"))?;
        let (w, h) = (
            u32::try_from(width).map_err(|_| error("dimensions"))?,
            u32::try_from(height).map_err(|_| error("dimensions"))?,
        );
        dimensions(w, h)?;
        let orientation = number(kCGImagePropertyOrientation)?.unwrap_or(1);
        if !(1..=8).contains(&orientation) {
            return Err(error("unsupported"));
        }
        let image = NonNull::new(CGImageSourceCreateImageAtIndex(source.0, 0, null()))
            .map(|p| CFRetained::from_raw(p))
            .ok_or_else(|| error("damaged"))?;
        if CGImageSourceGetStatusAtIndex(source.0, 0) != 0 {
            return Err(error("damaged"));
        }
        if CGImage::width(Some(&image)) != w as usize || CGImage::height(Some(&image)) != h as usize
        {
            return Err(error("damaged"));
        }
        let space =
            CGColorSpace::with_name(Some(kCGColorSpaceSRGB)).ok_or_else(|| error("unsupported"))?;
        let mut pixels = vec![0; (w * h * 4) as usize];
        let context = CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            w as usize,
            h as usize,
            8,
            w as usize * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
        )
        .ok_or_else(|| error("unsupported"))?;
        CGContext::draw_image(
            Some(&context),
            CGRect::new(CGPoint::new(0., 0.), CGSize::new(w as f64, h as f64)),
            Some(&image),
        );
        drop(context);
        // ImageIO can create a lazy image even when decoding fails during drawing.
        if pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
            return Err(error("damaged"));
        }
        let (pixels, ow, oh) = orient(pixels, w, h, orientation);
        normalize_rgba(pixels, ow, oh)
    }
}

// Skip length-delimited metadata and stuffed scan bytes so an EXIF marker cannot fake EOI.
fn complete_jpeg(bytes: &[u8]) -> bool {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return false;
    }
    let mut i = 2;
    while i < bytes.len() {
        if bytes[i] != 0xff {
            return false;
        }
        while bytes.get(i) == Some(&0xff) {
            i += 1;
        }
        let Some(&marker) = bytes.get(i) else {
            return false;
        };
        i += 1;
        if marker == 0xd9 {
            return true;
        }
        if marker == 0 || marker == 0xd8 {
            return false;
        }
        if marker == 1 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let Some(len) = bytes.get(i..i + 2) else {
            return false;
        };
        let len = u16::from_be_bytes([len[0], len[1]]) as usize;
        if len < 2 || len > bytes.len() - i {
            return false;
        }
        i += len;
        if marker == 0xda {
            while i < bytes.len() {
                if bytes[i] == 0xff {
                    let Some(&next) = bytes.get(i + 1) else {
                        return false;
                    };
                    if next == 0 || (0xd0..=0xd7).contains(&next) {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
        }
    }
    false
}

fn orient(pixels: Vec<u8>, w: u32, h: u32, orientation: i64) -> (Vec<u8>, u32, u32) {
    if orientation == 1 {
        return (pixels, w, h);
    }
    let (ow, oh) = if orientation >= 5 { (h, w) } else { (w, h) };
    let mut output = vec![0; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = match orientation {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (h - 1 - y, x),
                7 => (h - 1 - y, w - 1 - x),
                8 => (y, w - 1 - x),
                _ => unreachable!(),
            };
            let from = ((y * w + x) * 4) as usize;
            let to = ((dy * ow + dx) * 4) as usize;
            output[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
        }
    }
    (output, ow, oh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::io::Cursor;
    const RGB: &[u8] = include_bytes!("../../../tests/fixtures/badges/color/rgb.jpg");
    fn pixels(mask: &Mask) -> Vec<u8> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&mask.png)
            .unwrap();
        let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
        assert!(reader.info().exif_metadata.is_none());
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut data).unwrap();
        data
    }
    fn exif(bytes: &[u8], orientation: u8) -> Vec<u8> {
        let mut result = bytes[..2].to_vec();
        result.extend_from_slice(&[0xff, 0xe1, 0, 34]);
        result.extend_from_slice(b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0");
        result.extend_from_slice(&[orientation, 0, 0, 0, 0, 0, 0, 0]);
        result.extend_from_slice(&bytes[2..]);
        result
    }
    #[test]
    fn imageio_decodes_baseline_progressive_and_grayscale_and_rejects_cmyk() {
        for bytes in [
            RGB,
            include_bytes!("../../../tests/fixtures/badges/color/progressive.jpeg"),
        ] {
            let mask = decode(bytes).unwrap();
            assert_eq!((mask.width, mask.height), (24, 16));
            assert!(mask.opaque);
            let data = pixels(&mask);
            assert!(data.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
            for (x, y, expected) in [
                (3, 3, [255, 0, 0]),
                (20, 3, [0, 255, 0]),
                (3, 12, [0, 0, 255]),
                (20, 12, [255, 255, 0]),
            ] {
                let p = &data[(y * 24 + x) * 4..][..3];
                assert!(
                    p.iter().zip(expected).all(|(&a, b)| a.abs_diff(b) < 25),
                    "{p:?}"
                );
            }
        }
        assert_eq!(
            decode(include_bytes!(
                "../../../tests/fixtures/badges/color/cmyk.jpg"
            ))
            .unwrap_err()
            .params["reason"],
            "unsupported"
        );
        let data = pixels(
            &decode(include_bytes!(
                "../../../tests/fixtures/badges/color/gray.jpg"
            ))
            .unwrap(),
        );
        assert!(
            data.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255)
        );
    }
    #[test]
    fn exif_rotations_and_mirrors_are_applied_and_metadata_is_removed() {
        let original = pixels(&decode(RGB).unwrap());
        // Expected top-left source corners for all eight EXIF orientations.
        for (o, corner) in [
            (1, 0),
            (2, 23),
            (3, 383),
            (4, 360),
            (5, 0),
            (6, 360),
            (7, 383),
            (8, 23),
        ] {
            let mask = decode(&exif(RGB, o)).unwrap();
            assert_eq!(
                (mask.width, mask.height),
                if o >= 5 { (16, 24) } else { (24, 16) }
            );
            let data = pixels(&mask);
            assert_eq!(
                &data[..4],
                &original[corner * 4..corner * 4 + 4],
                "orientation {o}"
            );
            let (expected, _, _) = orient(original.clone(), 24, 16, i64::from(o));
            assert_eq!(data, expected);
        }
    }
    #[test]
    fn all_exif_orientations_map_every_pixel_to_the_expected_position() {
        let pixels = (1..=6).flat_map(|v| [v, 0, 0, 255]).collect::<Vec<_>>();
        for (o, expected) in [
            (1, [1, 2, 3, 4, 5, 6]),
            (2, [3, 2, 1, 6, 5, 4]),
            (3, [6, 5, 4, 3, 2, 1]),
            (4, [4, 5, 6, 1, 2, 3]),
            (5, [1, 4, 2, 5, 3, 6]),
            (6, [4, 1, 5, 2, 6, 3]),
            (7, [6, 3, 5, 2, 4, 1]),
            (8, [3, 6, 2, 5, 1, 4]),
        ] {
            let (data, _, _) = orient(pixels.clone(), 3, 2, o);
            assert_eq!(
                data.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[0])
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
    #[test]
    fn failed_scan_decoding_is_rejected_even_with_eoi() {
        let sos = RGB.windows(2).position(|w| w == [0xff, 0xda]).unwrap();
        let length = u16::from_be_bytes([RGB[sos + 2], RGB[sos + 3]]) as usize;
        let scan = sos + 2 + length;
        for keep in [0, 1, 2, 4] {
            let mut damaged = RGB[..scan + keep].to_vec();
            damaged.extend_from_slice(&[0xff, 0xd9]);
            assert!(decode(&damaged).is_err(), "accepted {keep} entropy bytes");
        }
    }
    #[test]
    fn corrupt_truncated_and_oversized_jpegs_are_rejected_before_normalization() {
        assert!(decode(b"not jpeg").is_err());
        assert!(decode(&RGB[..RGB.len() / 2]).is_err());
        assert!(decode(&RGB[..RGB.len() - 2]).is_err());
        let mut oversized = RGB.to_vec();
        let sof = oversized
            .windows(2)
            .position(|w| w == [0xff, 0xc0])
            .unwrap();
        oversized[sof + 5..sof + 9].copy_from_slice(&[0x10, 0, 0x10, 0]);
        assert_eq!(
            decode(&oversized).unwrap_err().params["reason"],
            "dimensions"
        );
    }
}
