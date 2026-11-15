//! Bounded icon metadata reads and coordinated native writes.
use crate::{
    localization::AppError,
    targets::{self, Target},
};
use objc2::{
    AnyThread,
    rc::{Retained, autoreleasepool},
};
use objc2_app_kit::{NSImage, NSWorkspace, NSWorkspaceIconCreationOptions};
use objc2_foundation::{
    NSData, NSFileCoordinator, NSFileCoordinatorWritingOptions, NSString, NSURL,
};
use std::{cell::RefCell, fs::File, os::unix::fs::OpenOptionsExt, path::Path, ptr::NonNull};
const LIMIT: usize = 16 * 1024 * 1024;
// Finder coalesces the custom-icon flag transition unless the cleared state is visible.
const REFRESH_PAUSE: std::time::Duration = std::time::Duration::from_millis(100);
pub fn error(reason: &str) -> AppError {
    AppError::reason("operation_error", reason)
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fingerprint {
    pub custom: bool,
    resources: Vec<(i16, [u8; 32])>,
}
impl Fingerprint {
    // Icon data can outlive the FinderInfo flag (for example after xattr loss).
    pub fn has_icon(&self) -> bool {
        self.custom || !self.resources.is_empty()
    }
}
#[link(name = "System")]
unsafe extern "C" {
    fn CC_SHA256(data: *const std::ffi::c_void, len: u32, output: *mut u8) -> *mut u8;
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut output = [0; 32];
    // All callers pass resource data bounded to 16 MiB; CommonCrypto writes exactly 32 bytes.
    unsafe {
        CC_SHA256(
            bytes.as_ptr().cast(),
            bytes.len() as u32,
            output.as_mut_ptr(),
        );
    }
    output
}
fn word(bytes: &[u8], at: usize) -> Result<usize, AppError> {
    Ok(u16::from_be_bytes(
        bytes
            .get(at..at + 2)
            .ok_or_else(|| error("fingerprint"))?
            .try_into()
            .unwrap(),
    ) as usize)
}
fn long(bytes: &[u8], at: usize) -> Result<usize, AppError> {
    Ok(u32::from_be_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| error("fingerprint"))?
            .try_into()
            .unwrap(),
    ) as usize)
}
// Resource-map offsets are relative to the map, type list, or data area respectively.
fn resources(bytes: &[u8]) -> Result<Vec<(i16, [u8; 32])>, AppError> {
    let data_at = long(bytes, 0)?;
    let map_at = long(bytes, 4)?;
    let data = bytes
        .get(
            data_at
                ..data_at
                    .checked_add(long(bytes, 8)?)
                    .ok_or_else(|| error("fingerprint"))?,
        )
        .ok_or_else(|| error("fingerprint"))?;
    let map = bytes
        .get(
            map_at
                ..map_at
                    .checked_add(long(bytes, 12)?)
                    .ok_or_else(|| error("fingerprint"))?,
        )
        .ok_or_else(|| error("fingerprint"))?;
    let types = word(map, 24)?;
    let count = word(map, types)?;
    let mut out = Vec::new();
    let mut payload_budget = 0;
    if count == 65535 {
        return Ok(out);
    }
    for i in 0..=count {
        let at = types + 2 + i * 8;
        let kind = map.get(at..at + 4).ok_or_else(|| error("fingerprint"))?;
        let n = word(map, at + 4)?;
        let refs = types + word(map, at + 6)?;
        if kind != b"icns" {
            continue;
        }
        if n == 65535 {
            continue;
        }
        if n >= 16 {
            return Err(error("fingerprint"));
        }
        for j in 0..=n {
            let r = refs + j * 12;
            let id = word(map, r)? as u16 as i16;
            let b = map.get(r + 5..r + 8).ok_or_else(|| error("fingerprint"))?;
            let offset = ((b[0] as usize) << 16) | ((b[1] as usize) << 8) | b[2] as usize;
            let len = long(data, offset)?;
            let payload = data
                .get(offset + 4..offset + 4 + len)
                .ok_or_else(|| error("fingerprint"))?;
            if payload.len() < 8 || &payload[..4] != b"icns" || long(payload, 4)? != payload.len() {
                return Err(error("fingerprint"));
            }
            payload_budget += payload.len();
            if payload_budget > LIMIT {
                return Err(error("fingerprint"));
            }
            out.push((id, digest(payload)));
        }
    }
    out.sort_by_key(|v| v.0);
    if out.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(error("fingerprint"));
    }
    Ok(out)
}
pub fn fingerprint(path: &Path) -> Result<Fingerprint, AppError> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| error("fingerprint"))?;
    let mut finder = [0u8; 32];
    let n = unsafe {
        libc::getxattr(
            c.as_ptr(),
            c"com.apple.FinderInfo".as_ptr(),
            finder.as_mut_ptr().cast(),
            finder.len(),
            0,
            0,
        )
    };
    if n < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOATTR) {
        return Err(error("fingerprint"));
    }
    if n >= 0 && n != 32 {
        return Err(error("fingerprint"));
    }
    let custom = n == 32 && u16::from_be_bytes([finder[8], finder[9]]) & 0x400 != 0;
    let icon = path.join("Icon\r");
    // Reject a substituted Icon file rather than following it outside the selected directory.
    let file = match File::options()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&icon)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !custom => {
            return Ok(Fingerprint {
                custom,
                resources: vec![],
            });
        }
        Err(_) => return Err(error("fingerprint")),
    };
    if !file.metadata().map_err(|_| error("fingerprint"))?.is_file() {
        return Err(error("fingerprint"));
    }
    use std::os::fd::AsRawFd;
    let n = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            c"com.apple.ResourceFork".as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOATTR) && !custom {
        return Ok(Fingerprint {
            custom,
            resources: vec![],
        });
    }
    if n <= 0 || n as usize > LIMIT {
        return Err(error("fingerprint"));
    }
    let mut bytes = vec![0; n as usize];
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            c"com.apple.ResourceFork".as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            0,
            0,
        )
    };
    if read != n {
        return Err(error("fingerprint"));
    }
    let resources = resources(&bytes)?;
    if custom && resources.is_empty() {
        return Err(error("fingerprint"));
    }
    Ok(Fingerprint { custom, resources })
}
fn decode(bytes: Option<&[u8]>) -> Result<Option<Retained<NSImage>>, AppError> {
    bytes
        .map(|data| {
            NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(data))
                .ok_or_else(|| error("image"))
        })
        .transpose()
}
fn set_image(path: &Path, image: Option<&NSImage>) -> Result<(), AppError> {
    #[cfg(test)]
    NATIVE_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = NSString::from_str(path.to_str().ok_or_else(|| error("write"))?);
    if NSWorkspace::sharedWorkspace().setIcon_forFile_options(
        image,
        &path,
        NSWorkspaceIconCreationOptions::empty(),
    ) {
        Ok(())
    } else {
        Err(error("write"))
    }
}
pub fn native(path: &Path, bytes: Option<&[u8]>) -> Result<(), AppError> {
    autoreleasepool(|_| {
        let image = decode(bytes)?;
        set_image(path, image.as_deref())
    })
}
pub fn write(
    target: &Target,
    expected: &Fingerprint,
    bytes: Option<&[u8]>,
) -> Result<(), AppError> {
    autoreleasepool(|_| {
        let coordinator = NSFileCoordinator::new();
        let url = NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str(target.actual_path.to_str().ok_or_else(|| error("write"))?),
            true,
        );
        let result = RefCell::new(None);
        let callback = block2::RcBlock::new(|url: NonNull<NSURL>| {
            let outcome = (|| {
                let path = unsafe { url.as_ref() }
                    .path()
                    .ok_or_else(|| error("coordination"))?;
                let path = std::path::PathBuf::from(path.to_string());
                targets::revalidate(target)?;
                targets::check_actual(target, &path)?;
                if &fingerprint(&path)? != expected {
                    return Err(error("confirmation"));
                }
                let outcome = if bytes.is_some() && expected.has_icon() {
                    (|| {
                        // Decode before clearing so a payload error leaves the old icon in place.
                        let image = decode(bytes)?;
                        if let Err(e) = native(&path, None) {
                            let unchanged = fingerprint(&path).is_ok_and(|f| f == *expected);
                            return Err(if unchanged { e } else { error("uncertain") });
                        }
                        std::thread::sleep(REFRESH_PAUSE);
                        targets::revalidate(target).map_err(|_| error("uncertain"))?;
                        targets::check_actual(target, &path).map_err(|_| error("uncertain"))?;
                        if fingerprint(&path)
                            .map_err(|_| error("uncertain"))?
                            .has_icon()
                        {
                            return Err(error("uncertain"));
                        }
                        set_image(&path, image.as_deref()).map_err(|_| error("uncertain"))?;
                        if !fingerprint(&path).map_err(|_| error("uncertain"))?.custom {
                            return Err(error("uncertain"));
                        }
                        Ok(())
                    })()
                } else {
                    native(&path, bytes)
                };
                if targets::revalidate(target).is_err()
                    || targets::check_actual(target, &path).is_err()
                {
                    return Err(error("uncertain"));
                }
                outcome
            })();
            *result.borrow_mut() = Some(outcome);
        });
        let mut native_error = None;
        coordinator.coordinateWritingItemAtURL_options_error_byAccessor(
            &url,
            NSFileCoordinatorWritingOptions::empty(),
            Some(&mut native_error),
            &callback,
        );
        drop(callback);
        result
            .into_inner()
            .unwrap_or_else(|| Err(error("coordination")))
    })
}
#[cfg(test)]
pub static NATIVE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(test)]
pub static NATIVE_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_resource_maps_fail_closed() {
        for n in 0..128 {
            assert!(resources(&vec![0xff; n]).is_err());
        }
        assert!(resources(&[0; 32]).is_err());
    }
    #[test]
    fn resource_map_layout_is_not_part_of_the_fingerprint() {
        let mut bytes = vec![0u8; 128];
        bytes[0..4].copy_from_slice(&16u32.to_be_bytes());
        bytes[4..8].copy_from_slice(&48u32.to_be_bytes());
        bytes[8..12].copy_from_slice(&12u32.to_be_bytes());
        bytes[12..16].copy_from_slice(&80u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&8u32.to_be_bytes());
        bytes[20..24].copy_from_slice(b"icns");
        bytes[24..28].copy_from_slice(&8u32.to_be_bytes());
        bytes[72..74].copy_from_slice(&28u16.to_be_bytes());
        bytes[78..82].copy_from_slice(b"icns");
        bytes[84..86].copy_from_slice(&10u16.to_be_bytes());
        bytes[86..88].copy_from_slice(&(-16455i16).to_be_bytes());
        let expected = resources(&bytes).unwrap();
        assert_eq!(expected.len(), 1);
        bytes[64..68].copy_from_slice(&0xfeed1234u32.to_be_bytes());
        bytes[90] = 64;
        assert_eq!(resources(&bytes).unwrap(), expected);
        bytes[27] = 9;
        assert!(resources(&bytes).is_err());
    }
    #[test]
    fn symlinked_icon_file_is_rejected() {
        let root = std::env::temp_dir().join(targets::next_id("t08-link"));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("body"), b"keep").unwrap();
        std::os::unix::fs::symlink(root.join("body"), root.join("Icon\r")).unwrap();
        assert!(fingerprint(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn coordinated_replacement_clears_and_reapplies_a_custom_icon() {
        let _serial = NATIVE_TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(targets::next_id("t08-refresh"));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let path = root.join("target");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("body"), b"keep").unwrap();
        let t = targets::check(targets::Request::Inspect(vec![path.clone()]))
            .unwrap()
            .items
            .remove(0);
        let a =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png")).unwrap();
        let b =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/32x32.png")).unwrap();
        native(&path, Some(&a)).unwrap();
        let applied = fingerprint(&path).unwrap();
        assert!(applied.custom);
        NATIVE_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        let error = write(&t, &applied, Some(b"not-an-image")).unwrap_err();
        assert_eq!(error.params["reason"], "image");
        assert_eq!(NATIVE_CALLS.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(fingerprint(&path).unwrap(), applied);
        NATIVE_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        write(&t, &applied, Some(&b)).unwrap();
        assert_eq!(
            NATIVE_CALLS.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "replacement must clear and reapply"
        );
        let replaced = fingerprint(&path).unwrap();
        assert!(replaced.custom);
        assert_ne!(replaced, applied);
        assert_eq!(std::fs::read(path.join("body")).unwrap(), b"keep");

        let clean = root.join("clean");
        std::fs::create_dir(&clean).unwrap();
        let clean_target = targets::check(targets::Request::Inspect(vec![clean.clone()]))
            .unwrap()
            .items
            .remove(0);
        let clean_before = fingerprint(&clean).unwrap();
        assert!(!clean_before.has_icon());
        NATIVE_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        write(&clean_target, &clean_before, Some(&b)).unwrap();
        assert_eq!(
            NATIVE_CALLS.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "first apply must not clear"
        );
        assert!(fingerprint(&clean).unwrap().custom);

        let orphan = root.join("orphan");
        std::fs::create_dir(&orphan).unwrap();
        native(&orphan, Some(&a)).unwrap();
        let path_c = std::ffi::CString::new(orphan.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(
            unsafe { libc::removexattr(path_c.as_ptr(), c"com.apple.FinderInfo".as_ptr(), 0) },
            0
        );
        let orphan_before = fingerprint(&orphan).unwrap();
        assert!(!orphan_before.custom && orphan_before.has_icon());
        let orphan_target = targets::check(targets::Request::Inspect(vec![orphan.clone()]))
            .unwrap()
            .items
            .remove(0);
        NATIVE_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        write(&orphan_target, &orphan_before, Some(&b)).unwrap();
        assert_eq!(
            NATIVE_CALLS.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "orphan replacement must clear and reapply"
        );
        assert!(fingerprint(&orphan).unwrap().custom);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn coordinated_callback_rejects_changed_icons_identities_and_links() {
        let _serial = NATIVE_TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(targets::next_id("t08-callback"));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let path = root.join("target");
        let other = root.join("other");
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(&other).unwrap();
        let alias = root.join("link");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        let t = targets::check(targets::Request::Inspect(vec![alias.clone()]))
            .unwrap()
            .items
            .remove(0);
        let clean = fingerprint(&path).unwrap();
        let png =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png")).unwrap();
        native(&path, Some(&png)).unwrap();
        let changed = fingerprint(&path).unwrap();
        assert_eq!(
            write(&t, &clean, None).unwrap_err().params["reason"],
            "confirmation"
        );
        assert_eq!(fingerprint(&path).unwrap(), changed);
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&other, &alias).unwrap();
        assert!(write(&t, &changed, None).is_err());
        assert_eq!(fingerprint(&path).unwrap(), changed);
        assert!(!fingerprint(&other).unwrap().custom);
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        std::fs::rename(&path, root.join("moved")).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(write(&t, &changed, Some(&png)).is_err());
        assert!(!fingerprint(&path).unwrap().custom);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn orphan_icon_data_requires_overwrite_confirmation() {
        let _serial = NATIVE_TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(targets::next_id("t08-orphan"));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("orphan");
        std::fs::create_dir(&path).unwrap();
        let png =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png")).unwrap();
        native(&path, Some(&png)).unwrap();
        let path_c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(
            unsafe { libc::removexattr(path_c.as_ptr(), c"com.apple.FinderInfo".as_ptr(), 0) },
            0
        );
        let orphan = fingerprint(&path).unwrap();
        assert!(!orphan.custom);
        assert!(orphan.has_icon());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_fingerprint_and_content_protection() {
        let _serial = NATIVE_TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(targets::next_id("t08-icon"));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("目录\n\"\\");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("body"), b"keep").unwrap();
        let path_c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(
            unsafe {
                libc::setxattr(
                    path_c.as_ptr(),
                    c"dev.dimple.test".as_ptr(),
                    b"keep".as_ptr().cast(),
                    4,
                    0,
                    0,
                )
            },
            0
        );
        let clean = fingerprint(&path).unwrap();
        assert!(!clean.custom);
        let a =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png")).unwrap();
        native(&path, Some(&a)).unwrap();
        let first = fingerprint(&path).unwrap();
        assert!(first.custom);
        assert_eq!(
            unsafe {
                libc::setxattr(
                    path_c.as_ptr(),
                    c"dev.dimple.test".as_ptr(),
                    b"changed".as_ptr().cast(),
                    7,
                    0,
                    0,
                )
            },
            0
        );
        for _ in 0..5 {
            assert_eq!(fingerprint(&path).unwrap(), first);
        }
        let b =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/32x32.png")).unwrap();
        native(&path, Some(&b)).unwrap();
        assert_ne!(fingerprint(&path).unwrap(), first);
        native(&path, None).unwrap();
        assert_eq!(fingerprint(&path).unwrap(), clean);
        assert_eq!(std::fs::read(path.join("body")).unwrap(), b"keep");
        let mut value = [0; 7];
        assert_eq!(
            unsafe {
                libc::getxattr(
                    path_c.as_ptr(),
                    c"dev.dimple.test".as_ptr(),
                    value.as_mut_ptr().cast(),
                    7,
                    0,
                    0,
                )
            },
            7
        );
        assert_eq!(&value, b"changed");
        std::fs::remove_dir_all(root).unwrap();
    }
}
