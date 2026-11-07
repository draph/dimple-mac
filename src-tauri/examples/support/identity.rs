//! Isolated T07 observations, never a production writer or a safety certification.
use super::{Result, Target, body, fingerprint, native, xattr};
use objc2::rc::Retained;
use objc2_foundation::{NSString, NSURL};
use std::{
    fs,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug)]
enum Candidate {
    Descriptor,
    Reference,
    ReferenceLiteral,
}
struct Invocation {
    argument: PathBuf,
    reference: Option<Retained<NSURL>>,
}
impl Candidate {
    fn prepare(self, target: &Target) -> Result<Invocation> {
        let (argument, reference) = match self {
            Self::Descriptor => (
                PathBuf::from(format!("/dev/fd/{}", target.handle.as_raw_fd())),
                None,
            ),
            Self::Reference | Self::ReferenceLiteral => {
                let url = NSURL::fileURLWithPath_isDirectory(
                    &NSString::from_str(target.path.to_str().ok_or("E_PATH_ENCODING")?),
                    true,
                )
                .fileReferenceURL()
                .ok_or("E_FILE_REFERENCE")?;
                assert!(url.isFileReferenceURL());
                let argument = if matches!(self, Self::ReferenceLiteral) {
                    let text = url.absoluteString().ok_or("E_REFERENCE_URL")?.to_string();
                    let id = text
                        .strip_prefix("file:///.file/id=")
                        .ok_or("E_REFERENCE_URL_FORM")?;
                    if !id.bytes().all(|c| c.is_ascii_digit() || b"./".contains(&c)) {
                        return Err("E_REFERENCE_URL_FORM".into());
                    }
                    PathBuf::from(format!("/.file/id={id}"))
                } else {
                    PathBuf::from(url.path().ok_or("E_REFERENCE_PATH")?.to_string())
                };
                (argument, Some(url))
            }
        };
        // stat(/dev/fd/N) describes devfs; opening it duplicates the held object.
        let b = target.handle.metadata()?;
        let opens_held_object = fs::File::open(&argument)
            .and_then(|f| f.metadata())
            .is_ok_and(|a| (a.dev(), a.ino()) == (b.dev(), b.ino()));
        if !matches!(self, Self::ReferenceLiteral) {
            assert!(opens_held_object);
        }
        println!(
            "ARGUMENT candidate={self:?} opens_held_object={opens_held_object} ordinary_path={}",
            argument == target.path
        );
        Ok(Invocation {
            argument,
            reference,
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Apply,
    Overwrite,
    Clear,
}
#[derive(Clone, Copy, Debug)]
enum Mutation {
    None,
    Directory,
    Parent,
    Link,
}

fn set_attribute(path: &Path, name: &str, value: &str, hex: bool) -> Result<()> {
    let status = Command::new("/usr/bin/xattr")
        .args([if hex { "-wx" } else { "-w" }, name, value])
        .arg(path)
        .status()?;
    if !status.success() {
        return Err("E_FIXTURE_XATTR".into());
    }
    Ok(())
}
pub(super) fn fixture(path: &Path, initial: Option<&[u8]>) -> Result<()> {
    fs::create_dir(path)?;
    fs::create_dir(path.join("nested"))?;
    fs::write(path.join("nested/正文 \n\"\\.txt"), "T07 body\n")?;
    set_attribute(path, "org.folderify.probe", "unchanged", false)?;
    // Preserve a label and folder-location bytes, not just an unrelated xattr.
    set_attribute(
        path,
        "com.apple.FinderInfo",
        "0000000000000000000a12340000000000000000000000000000000000000000",
        true,
    )?;
    if let Some(bytes) = initial {
        assert!(native(path, Some(bytes))?);
    }
    Ok(())
}
fn finder_info(path: &Path) -> Result<[u8; 32]> {
    let text = xattr(path, "com.apple.FinderInfo")?.ok_or("E_FINDERINFO_MISSING")?;
    let hex: String = String::from_utf8(text)?.split_whitespace().collect();
    if hex.len() != 64 {
        return Err("E_FINDERINFO_SIZE".into());
    }
    let mut bytes = [0; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(bytes)
}
pub(super) fn custom_icon(path: &Path) -> Result<bool> {
    let info = finder_info(path)?;
    Ok(u16::from_be_bytes([info[8], info[9]]) & 0x0400 != 0)
}
pub(super) fn protected(path: &Path) -> Result<impl PartialEq + std::fmt::Debug + use<>> {
    let mut info = finder_info(path)?;
    info[8] &= !0x04;
    let metadata = fs::metadata(path)?;
    Ok((
        body(path)?,
        xattr(path, "org.folderify.probe")?,
        info,
        (metadata.mode(), metadata.uid(), metadata.gid()),
    ))
}
pub(super) fn check_input(target: &Target, input: &Path) -> Result<()> {
    target.check()?;
    let now = fs::metadata(fs::canonicalize(input)?)?;
    let old = target.handle.metadata()?;
    if (now.dev(), now.ino()) != (old.dev(), old.ino()) {
        return Err("E_LINK_CHANGED".into());
    }
    Ok(())
}

fn observe(
    root: &Path,
    bytes: &[u8],
    other: &[u8],
    candidate: Candidate,
    operation: Operation,
    mutation: Mutation,
) -> Result<()> {
    let case = root.join(format!("{candidate:?}-{operation:?}-{mutation:?}"));
    fs::create_dir(&case)?;
    let parent = case.join("parent");
    fs::create_dir(&parent)?;
    let path = parent.join("target");
    let initial = match operation {
        Operation::Apply => None,
        Operation::Overwrite | Operation::Clear => Some(other),
    };
    fixture(&path, initial)?;
    let input = case.join("input-link");
    std::os::unix::fs::symlink(&path, &input)?;
    let target = Target::new(&path)?;
    check_input(&target, &input)?;
    let invocation = candidate.prepare(&target)?;
    let before = fingerprint(&path)?;
    let original_protected = protected(&path)?;
    let (original, replacement) = match mutation {
        Mutation::None => (path.clone(), None),
        Mutation::Directory => {
            let moved = case.join("moved");
            fs::rename(&path, &moved)?;
            fixture(&path, initial)?;
            (moved, Some(path.clone()))
        }
        Mutation::Parent => {
            let moved = case.join("moved-parent");
            fs::rename(&parent, &moved)?;
            fs::create_dir(&parent)?;
            fixture(&path, initial)?;
            (moved.join("target"), Some(path.clone()))
        }
        Mutation::Link => {
            let replacement = case.join("replacement");
            fixture(&replacement, initial)?;
            fs::remove_file(&input)?;
            std::os::unix::fs::symlink(&replacement, &input)?;
            (path.clone(), Some(replacement))
        }
    };
    let replacement_before = replacement.as_deref().map(fingerprint).transpose()?;
    let replacement_protected = replacement.as_deref().map(protected).transpose()?;
    if let Some(reference) = &invocation.reference {
        let late = reference.path().ok_or("E_REFERENCE_PATH")?.to_string();
        println!(
            "REFERENCE resolves_to_moved_original={} frozen_argument_changed={}",
            Path::new(&late) == original,
            Path::new(&late) != invocation.argument
        );
    }
    // Deliberately do not repeat the precheck: this is the check-to-call gap.
    let success = native(
        &invocation.argument,
        if matches!(operation, Operation::Clear) {
            None
        } else {
            Some(bytes)
        },
    )?;
    let original_changed = fingerprint(&original)? != before;
    let replacement_changed =
        replacement.as_deref().map(fingerprint).transpose()? != replacement_before;
    assert_eq!(protected(&original)?, original_protected);
    assert_eq!(
        replacement.as_deref().map(protected).transpose()?,
        replacement_protected
    );
    let original_custom = custom_icon(&original)?;
    let original_fork = fingerprint(&original)?.1.is_some();
    let replacement_custom = replacement.as_deref().map(custom_icon).transpose()?;
    let replacement_fork = replacement
        .as_deref()
        .map(|p| fingerprint(p).map(|f| f.1.is_some()))
        .transpose()?;
    println!(
        "OBSERVE candidate={candidate:?} operation={operation:?} mutation={mutation:?} api_success={success} original_changed={original_changed} replacement_changed={replacement_changed} original_custom={original_custom} original_fork={original_fork} replacement_custom={replacement_custom:?} replacement_fork={replacement_fork:?} input_still_valid={}",
        check_input(&target, &input).is_ok()
    );
    // Both candidates are experimental: report unsafe writes instead of hiding them.
    if replacement_changed {
        println!("BLOCKER E_REPLACEMENT_WRITTEN candidate={candidate:?}");
    }
    Ok(())
}

fn precheck_cases(root: &Path, candidate: Candidate) -> Result<()> {
    for mutation in ["moved", "deleted", "replaced", "link"] {
        let case = root.join(format!("precheck-{candidate:?}-{mutation}"));
        fs::create_dir(&case)?;
        let path = case.join("target");
        fixture(&path, None)?;
        let input = case.join("input-link");
        std::os::unix::fs::symlink(&path, &input)?;
        let target = Target::new(&path)?;
        match mutation {
            "moved" | "replaced" => fs::rename(&path, case.join("moved"))?,
            "deleted" => fs::remove_dir_all(&path)?,
            "link" => fs::remove_file(&input)?,
            _ => unreachable!(),
        }
        if mutation == "replaced" {
            fixture(&path, None)?;
        }
        if mutation == "link" {
            let replacement = case.join("replacement");
            fixture(&replacement, None)?;
            std::os::unix::fs::symlink(&replacement, &input)?;
        }
        assert!(check_input(&target, &input).is_err());
        println!("PASS precheck candidate={candidate:?} mutation={mutation} native_calls=0");
    }
    Ok(())
}

pub(super) fn run(root: &Path, bytes: &[u8]) -> Result<()> {
    let other = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png"))?;
    println!("T07 identity-only: serial worker; metadata observations, Finder NOT verified");
    for candidate in [
        Candidate::Descriptor,
        Candidate::Reference,
        Candidate::ReferenceLiteral,
    ] {
        precheck_cases(root, candidate)?;
        for operation in [Operation::Apply, Operation::Overwrite, Operation::Clear] {
            for mutation in [
                Mutation::None,
                Mutation::Directory,
                Mutation::Parent,
                Mutation::Link,
            ] {
                observe(root, bytes, &other, candidate, operation, mutation)?;
            }
        }
    }
    println!(
        "PASS 36 observations and 12 prechecks complete; body, unrelated xattr, non-icon FinderInfo and ownership/mode preserved; no candidate is certified identity-safe"
    );
    Ok(())
}
