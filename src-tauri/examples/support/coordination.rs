//! Coordinated-write experiment; no production IPC or universal exclusion guarantee.
use super::{Result, Target, fingerprint, identity, native};
use block2::RcBlock;
use objc2::rc::autoreleasepool;
use objc2_foundation::{NSFileCoordinator, NSFileCoordinatorWritingOptions, NSString, NSURL};
use std::{
    cell::RefCell, fs, os::unix::fs::MetadataExt, path::Path, ptr::NonNull, sync::mpsc, thread,
    time::Duration,
};

fn coordinate(path: &Path, mut action: impl FnMut(&Path) -> Result<()>) -> Result<()> {
    autoreleasepool(|_| {
        let coordinator = NSFileCoordinator::new();
        let url = NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str(path.to_str().ok_or("E_PATH_ENCODING")?),
            true,
        );
        let result = RefCell::new(None);
        let action = RefCell::new(&mut action);
        let callback = RcBlock::new(|url: NonNull<NSURL>| {
            // Foundation guarantees a live URL for the synchronous accessor invocation.
            let url = unsafe { url.as_ref() };
            let outcome = url
                .path()
                .ok_or_else(|| "E_COORDINATION_URL".into())
                .and_then(|p| action.borrow_mut()(Path::new(&p.to_string())));
            *result.borrow_mut() = Some(outcome);
        });
        let mut error = None;
        coordinator.coordinateWritingItemAtURL_options_error_byAccessor(
            &url,
            NSFileCoordinatorWritingOptions::empty(),
            Some(&mut error),
            &callback,
        );
        // An accessor error after native writing must retain its uncertain-write meaning.
        drop(callback);
        result
            .into_inner()
            .unwrap_or_else(|| Err("E_COORDINATION_NOT_EXECUTED".into()))
    })
}

fn write(
    target: &Target,
    input: &Path,
    bytes: Option<&[u8]>,
    mut gap: impl FnMut() -> Result<()>,
) -> Result<()> {
    coordinate(&target.path, |actual| {
        identity::check_input(target, input)?;
        let current = fs::metadata(actual)?;
        let held = target.handle.metadata()?;
        if (current.dev(), current.ino()) != (held.dev(), held.ino()) {
            return Err("E_TARGET_CHANGED".into());
        }
        gap()?;
        let success = native(actual, bytes)?;
        if identity::check_input(target, input).is_err() {
            return Err("E_WRITE_UNCERTAIN".into());
        }
        if !success {
            return Err("E_NATIVE_WRITE_FAILED".into());
        }
        Ok(())
    })
}

pub(super) fn run(root: &Path, bytes: &[u8]) -> Result<()> {
    let path = root.join("coordinated 中文\n\"\\");
    identity::fixture(&path, None)?;
    let target = Target::new(&path)?;
    let protected = identity::protected(&path)?;
    write(&target, &path, Some(bytes), || Ok(()))?;
    assert!(identity::custom_icon(&path)?);
    let applied = fingerprint(&path)?;
    let other = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png"))?;
    write(&target, &path, Some(&other), || Ok(()))?;
    assert_ne!(fingerprint(&path)?, applied);
    write(&target, &path, None, || Ok(()))?;
    assert!(!identity::custom_icon(&path)?);
    assert!(fingerprint(&path)?.1.is_none());
    assert_eq!(identity::protected(&path)?, protected);
    println!("PASS coordinated apply/overwrite/clear, special paths and protected metadata");

    for mode in ["directory", "parent", "link", "deleted"] {
        let case = root.join(format!("before-{mode}"));
        fs::create_dir(&case)?;
        let parent = case.join("parent");
        fs::create_dir(&parent)?;
        let path = parent.join("target");
        identity::fixture(&path, None)?;
        let input = case.join("input");
        std::os::unix::fs::symlink(&path, &input)?;
        let target = Target::new(&path)?;
        match mode {
            "parent" => {
                fs::rename(&parent, case.join("moved"))?;
                fs::create_dir(&parent)?;
                identity::fixture(&path, None)?;
            }
            "directory" => {
                fs::rename(&path, case.join("moved"))?;
                identity::fixture(&path, None)?;
            }
            "deleted" => fs::remove_dir_all(&path)?,
            "link" => {
                fs::remove_file(&input)?;
                let other = case.join("other");
                identity::fixture(&other, None)?;
                std::os::unix::fs::symlink(other, &input)?;
            }
            _ => unreachable!(),
        }
        let mut reached = false;
        assert!(
            write(&target, &input, Some(bytes), || {
                reached = true;
                Ok(())
            })
            .is_err()
        );
        assert!(!reached);
        println!("PASS changed input rejected mode={mode} native_calls=0");
    }

    // Cooperating writers wait; bounded channels prevent a broken experiment hanging forever.
    let path = root.join("serialized");
    identity::fixture(&path, None)?;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first_path = path.clone();
    let first = thread::spawn(move || {
        coordinate(&first_path, |_| {
            entered_tx.send(())?;
            release_rx.recv_timeout(Duration::from_secs(5))?;
            Ok(())
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(5))?;
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (second_tx, second_rx) = mpsc::channel();
    let second = thread::spawn(move || {
        attempt_tx.send(())?;
        coordinate(&path, |_| {
            second_tx.send(())?;
            Ok(())
        })
    });
    attempt_rx.recv_timeout(Duration::from_secs(5))?;
    let pending = second_rx.recv_timeout(Duration::from_millis(250));
    release_tx.send(())?;
    first.join().map_err(|_| "E_FIRST_WORKER")??;
    second.join().map_err(|_| "E_SECOND_WORKER")??;
    assert!(matches!(pending, Err(mpsc::RecvTimeoutError::Timeout)));
    second_rx.recv_timeout(Duration::from_secs(5))?;
    println!(
        "PASS cooperating writer waited for accessor release (two threads, not Finder evidence)"
    );

    let path = root.join("uncoordinated");
    identity::fixture(&path, None)?;
    let target = Target::new(&path)?;
    let original = root.join("uncoordinated-original");
    let outcome = write(&target, &path, Some(bytes), || {
        fs::rename(&path, &original)?;
        identity::fixture(&path, None)
    });
    assert_eq!(outcome.unwrap_err().to_string(), "E_WRITE_UNCERTAIN");
    assert!(identity::custom_icon(&path)?);
    assert!(!identity::custom_icon(&original)?);
    println!(
        "EXPECTED_LIMIT uncoordinated replacement written; E_WRITE_UNCERTAIN; no automatic retry"
    );
    println!(
        "PASS coordination experiment complete; Finder, cross-process and slow volumes NOT verified"
    );
    Ok(())
}
