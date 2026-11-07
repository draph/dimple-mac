//! T03 experiment, not a production writer: path-based setIcon has a TOCTOU gap.
//! Run with `cargo run --manifest-path src-tauri/Cargo.toml --example icon_write_probe`.
//! Only writes inside its own temporary root. Enter advances each Finder checkpoint.
//! `--metadata-only` skips visual checkpoints and never claims Finder verification.
//! `--identity-only` runs T07 identity candidates without Finder or the T03 stall.
//! `--coordination-only` tests coordinated writes and the uncoordinated replacement limit.
//! Known limits: small legacy representations differ on system readback; the controlled
//! stall tests a CLI heartbeat, not UI/batch cancellation. Never ship this as a writer.
use objc2::{AnyThread, rc::autoreleasepool};
use objc2_app_kit::{NSImage, NSWorkspace, NSWorkspaceIconCreationOptions};
use objc2_foundation::{NSData, NSString};
#[path = "support/coordination.rs"]
mod coordination;
#[path = "support/identity.rs"]
mod identity;
use std::{
    error::Error,
    fs::{self, File},
    io::{self, Cursor},
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const TYPES: [(&str, &str, u32); 10] = [
    ("ic04", "16x16", 16),
    ("ic11", "16x16@2x", 32),
    ("ic05", "32x32", 32),
    ("ic12", "32x32@2x", 64),
    ("ic07", "128x128", 128),
    ("ic13", "128x128@2x", 256),
    ("ic08", "256x256", 256),
    ("ic14", "256x256@2x", 512),
    ("ic09", "512x512", 512),
    ("ic10", "512x512@2x", 1024),
];

// Fixed, trusted repository PNGs only; this is not an untrusted input parser.
fn encode() -> Result<Vec<u8>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset");
    let mut bytes = Vec::from(&b"icns\0\0\0\0"[..]);
    for (kind, name, size) in TYPES {
        let png = fs::read(root.join(format!("icon_{name}.png")))?;
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into()?), size);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into()?), size);
        let payload = if kind == "ic04" || kind == "ic05" {
            let (_, _, rgba) = pixels(&root.join(format!("icon_{name}.png")))?;
            let mut argb = Vec::from(&b"ARGB"[..]);
            // ICNS channel RLE: literal packets avoid needing a second compression implementation.
            for channel in [3, 0, 1, 2] {
                let plane: Vec<_> = rgba.as_chunks::<4>().0.iter().map(|p| p[channel]).collect();
                for packet in plane.chunks(128) {
                    argb.push((packet.len() - 1) as u8);
                    argb.extend(packet);
                }
            }
            argb
        } else {
            png
        };
        bytes.extend(kind.as_bytes());
        bytes.extend(u32::try_from(payload.len() + 8)?.to_be_bytes());
        bytes.extend(payload);
    }
    let len = u32::try_from(bytes.len())?;
    bytes[4..8].copy_from_slice(&len.to_be_bytes());
    let mut offset = 8;
    for (kind, _, _) in TYPES {
        assert_eq!(&bytes[offset..offset + 4], kind.as_bytes());
        let len = u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        assert!(len > 8 && offset + len <= bytes.len());
        offset += len;
    }
    assert_eq!(offset, bytes.len());
    Ok(bytes)
}

struct Target {
    path: PathBuf,
    handle: File,
}
impl Target {
    fn new(path: &Path) -> Result<Self> {
        if !fs::symlink_metadata(path)?.is_dir() {
            return Err("E_TARGET_TYPE".into());
        }
        Ok(Self {
            path: path.to_owned(),
            handle: File::open(path)?,
        })
    }
    fn check(&self) -> Result<()> {
        let now = fs::symlink_metadata(&self.path)?;
        let old = self.handle.metadata()?;
        if !now.is_dir() || (now.dev(), now.ino()) != (old.dev(), old.ino()) {
            return Err("E_TARGET_CHANGED".into());
        }
        Ok(())
    }
}

fn native(path: &Path, bytes: Option<&[u8]>) -> Result<bool> {
    autoreleasepool(|_| {
        let image = bytes
            .map(|data| {
                NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(data))
                    .ok_or("E_IMAGE_DECODE")
            })
            .transpose()?;
        let path = NSString::from_str(path.to_str().ok_or("E_PATH_ENCODING")?);
        Ok(NSWorkspace::sharedWorkspace().setIcon_forFile_options(
            image.as_deref(),
            &path,
            NSWorkspaceIconCreationOptions::empty(),
        ))
    })
}
fn write(target: &Target, bytes: Option<&[u8]>) -> Result<()> {
    target.check()?;
    // shortcut: setIcon accepts a path, production must resolve the remaining replacement race.
    if !native(&target.path, bytes)? {
        return Err("E_NATIVE_WRITE_FAILED".into());
    }
    target.check()?;
    Ok(())
}
fn xattr(path: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    let out = Command::new("/usr/bin/xattr")
        .args(["-px", name])
        .arg(path)
        .output()?;
    if out.status.success() {
        return Ok(Some(out.stdout));
    }
    // Only the explicit missing-attribute diagnostic is absence; other errors are not swallowed.
    if String::from_utf8_lossy(&out.stderr).contains("No such xattr") {
        return Ok(None);
    }
    Err("E_XATTR_READ".into())
}
type Fingerprint = (Option<Vec<u8>>, Option<Vec<u8>>);
fn fingerprint(path: &Path) -> Result<Fingerprint> {
    let finder = xattr(path, "com.apple.FinderInfo")?;
    let icon = path.join("Icon\r");
    let fork = if icon.exists() {
        xattr(&icon, "com.apple.ResourceFork")?
    } else {
        None
    };
    Ok((finder, fork))
}
fn checkpoint(path: &Path, stage: &str) -> Result<()> {
    if std::env::args().any(|arg| arg == "--metadata-only") {
        println!("UNVERIFIED Finder checkpoint: {stage}");
        return Ok(());
    }
    assert!(
        Command::new("/usr/bin/open")
            .arg("-R")
            .arg(path)
            .status()?
            .success()
    );
    println!("CHECKPOINT {stage}: inspect Finder, capture evidence, then press Enter");
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    if line.is_empty() {
        return Err("E_VISUAL_CONFIRMATION_REQUIRED".into());
    }
    Ok(())
}
fn body(path: &Path) -> Result<Vec<(PathBuf, bool, Vec<u8>)>> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(PathBuf, bool, Vec<u8>)>) -> Result<()> {
        let mut entries = fs::read_dir(path)?.collect::<std::result::Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if entry.file_name() == "Icon\r" {
                continue;
            }
            let p = entry.path();
            let rel = p.strip_prefix(root)?.to_owned();
            if p.is_dir() {
                out.push((rel, true, vec![]));
                visit(root, &p, out)?;
            } else {
                out.push((rel, false, fs::read(p)?));
            }
        }
        Ok(())
    }
    let mut out = vec![];
    visit(path, path, &mut out)?;
    Ok(out)
}
fn experiment(root: PathBuf, bytes: Vec<u8>) -> Result<()> {
    println!("THREAD background serial writer");
    let path = root.join("Folderify T03 target");
    fs::create_dir(&path)?;
    fs::create_dir(path.join("nested"))?;
    fs::write(path.join("nested/正文.txt"), "T03 content\n")?;
    fs::write(path.join("empty"), [])?;
    assert!(
        Command::new("/usr/bin/xattr")
            .args(["-w", "org.folderify.probe", "unchanged"])
            .arg(&path)
            .status()?
            .success()
    );
    let original = body(&path)?;
    let unrelated = xattr(&path, "org.folderify.probe")?;
    let target = Target::new(&path)?;
    let before = fingerprint(&path)?;
    let started = Instant::now();
    write(&target, Some(&bytes))?;
    println!("PASS apply elapsed_ms={}", started.elapsed().as_millis());
    let applied = fingerprint(&path)?;
    assert!(before != applied);
    checkpoint(&path, "applied")?;
    // A different pre-existing icon makes the overwrite visible.
    let existing = root.join("existing.png");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png"),
        &existing,
    )?;
    write(&target, Some(&fs::read(existing)?))?;
    let other = fingerprint(&path)?;
    assert!(other != applied);
    checkpoint(&path, "existing custom icon")?;
    write(&target, Some(&bytes))?;
    assert!(
        fingerprint(&path)? != other,
        "overwrite must change the existing icon"
    );
    checkpoint(&path, "overwritten")?;
    write(&target, None)?;
    let cleared = fingerprint(&path)?;
    // Clearing may leave unrelated/default FinderInfo bytes, so check the custom-icon flag.
    if let Some(info) = &cleared.0 {
        let hex: String = String::from_utf8(info.clone())?
            .split_whitespace()
            .collect();
        let flags = u16::from_str_radix(&hex[16..20], 16)?;
        assert_eq!(flags & 0x0400, 0);
    }
    assert!(cleared.1.is_none());
    checkpoint(&path, "restored default (not previous icon)")?;
    assert!(body(&path)? == original, "body changed");
    assert!(xattr(&path, "org.folderify.probe")? == unrelated);
    println!("PASS body and unrelated xattr unchanged");

    assert!(native(&root.join("missing"), Some(&bytes)).is_ok_and(|ok| !ok));
    assert!(native(&path, Some(b"invalid image")).is_err());
    assert!(Target::new(&path.join("empty")).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o555))?;
    let denied = native(&path, Some(&bytes));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    assert!(!denied?);
    println!("PASS missing/type/decode/permission failures");

    fs::rename(&path, root.join("moved"))?;
    fs::create_dir(&path)?;
    let replacement = fingerprint(&path)?;
    assert!(write(&target, Some(&bytes)).is_err());
    assert!(fingerprint(&path)? == replacement);
    fs::remove_dir(&path)?;
    symlink(root.join("moved"), &path)?;
    assert!(write(&target, Some(&bytes)).is_err());
    println!("PASS replacement and link change rejected before write");

    // Deterministically expose the check-to-call race; only touches our sacrificial directories.
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    let race = Target::new(&path)?;
    race.check()?;
    fs::rename(&path, root.join("race-original"))?;
    fs::create_dir(&path)?;
    assert!(native(&path, Some(&bytes))?);
    assert!(race.check().is_err());
    assert!(fingerprint(&path)? != replacement);
    println!(
        "BLOCKER E_PATH_RACE: replacement received icon after precheck; postcheck detects too late"
    );
    native(&path, None)?;
    Ok(())
}
struct TempRoot(PathBuf);
impl TempRoot {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        if fs::remove_dir_all(&self.0).is_err() {
            eprintln!("E_TEMP_CLEANUP");
        }
    }
}
fn pixels(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(fs::read(path)?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("E_PNG_SIZE")?];
    let info = reader.next_frame(&mut buffer)?;
    buffer.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    Ok((info.width, info.height, buffer))
}
fn run() -> Result<()> {
    let output = Command::new("/usr/bin/mktemp")
        .args(["-d", "/private/tmp/folderify-t03.XXXXXXXX"])
        .output()?;
    if !output.status.success() {
        return Err("E_TEMP_CREATE".into());
    }
    let root = TempRoot(PathBuf::from(String::from_utf8(output.stdout)?.trim_end()));
    let bytes = encode()?;
    if std::env::args().any(|arg| arg == "--coordination-only") {
        let path = root.path().to_owned();
        return thread::spawn(move || coordination::run(&path, &bytes))
            .join()
            .map_err(|_| "E_COORDINATION_WORKER_PANIC")?;
    }
    if std::env::args().any(|arg| arg == "--identity-only") {
        let path = root.path().to_owned();
        return thread::spawn(move || identity::run(&path, &bytes))
            .join()
            .map_err(|_| "E_IDENTITY_WORKER_PANIC")?;
    }
    let icns = root.path().join("probe.icns");
    fs::write(&icns, &bytes)?;
    assert!(
        Command::new("/usr/bin/iconutil")
            .args(["-c", "iconset"])
            .arg(&icns)
            .status()?
            .success()
    );
    for (_, name, size) in TYPES {
        let png = root
            .path()
            .join("probe.iconset")
            .join(format!("icon_{name}.png"));
        let input = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset")
            .join(format!("icon_{name}.png"));
        let decoded = pixels(&png)?;
        assert_eq!((decoded.0, decoded.1), (size, size));
        let source = pixels(&input)?;
        let changed = decoded
            .2
            .iter()
            .zip(&source.2)
            .filter(|(a, b)| a != b)
            .count();
        let max_delta = decoded
            .2
            .iter()
            .zip(&source.2)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        println!("READBACK size={size} changed_channels={changed} max_delta={max_delta}");
    }
    println!(
        "PASS ICNS system-decoded dimensions; pixel differences reported, not accepted as equivalent"
    );
    let (tx, rx) = mpsc::channel();
    let path = root.path().to_owned();
    let worker = thread::spawn(move || {
        // A controlled stalled worker, not a claim about actual network-volume behavior.
        thread::sleep(Duration::from_secs(32));
        let result = experiment(path, bytes);
        let _ = tx.send(result);
    });
    let start = Instant::now();
    let mut beats = 0;
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => {
                worker.join().expect("worker panicked");
                result?;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                beats += 1;
                if beats == 300 {
                    println!(
                        "PASS controlled 30s worker stall: main heartbeat alive; batch stop/retry and UI responsiveness NOT tested"
                    );
                }
            }
            Err(_) => {
                worker.join().expect("worker panicked");
                return Err("E_WORKER_LOST".into());
            }
        }
    }
    println!(
        "PASS probe finished elapsed_s={} heartbeats={beats}; T03 remains blocked by path race",
        start.elapsed().as_secs()
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("PROBE_FAILED {error}");
        std::process::exit(1);
    }
}
