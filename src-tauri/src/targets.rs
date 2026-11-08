use crate::localization::AppError;
use serde::{Deserialize, Serialize};
use std::os::macos::fs::MetadataExt as MacMetadataExt;
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
    pub birth: i64,
    pub birth_nsec: i64,
}
impl Identity {
    fn read(m: &fs::Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            birth: m.st_birthtime(),
            birth_nsec: m.st_birthtime_nsec(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub path: PathBuf,
    pub actual_path: PathBuf,
    pub linked: bool,
    pub aliases: Vec<PathBuf>,
    pub identity: Identity,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub path: PathBuf,
    pub error: AppError,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Checked {
    pub mask: Option<crate::assets::Mask>,
    pub items: Vec<Target>,
    pub issues: Vec<Issue>,
    pub fingerprints: Vec<crate::icon::Fingerprint>,
}
#[derive(Serialize, Deserialize)]
pub enum Request {
    Inspect(Vec<PathBuf>),
    Fingerprint(Vec<Target>),
    Text(String),
}
fn failure(reason: &'static str) -> AppError {
    AppError::reason("invalid_input", reason)
}
fn io_error(e: std::io::Error) -> AppError {
    AppError::reason(
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "permission_denied"
        } else {
            "invalid_input"
        },
        "unavailable",
    )
}
fn application(url: &objc2_foundation::NSURL) -> Result<bool, AppError> {
    let mut value = None;
    // NSURLIsApplicationKey is documented as a Boolean resource value.
    unsafe {
        url.getResourceValue_forKey_error(&mut value, objc2_foundation::NSURLIsApplicationKey)
            .map_err(|_| failure("unavailable"))?;
        value
            .map(|v| objc2::msg_send![&*v, boolValue])
            .ok_or_else(|| failure("unavailable"))
    }
}
fn inspect(path: PathBuf) -> Result<Target, AppError> {
    use objc2_foundation::{NSString, NSURL};
    if !path.is_absolute() || path.to_str().is_none() {
        return Err(failure("path"));
    }
    let actual = fs::canonicalize(&path).map_err(io_error)?;
    let metadata = fs::metadata(&actual).map_err(io_error)?;
    if !metadata.is_dir() {
        return Err(failure(
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("svg"))
            {
                "badge"
            } else {
                "type"
            },
        ));
    }
    let protected = actual == Path::new("/")
        || [
            "/System",
            "/bin",
            "/sbin",
            "/Library/Apple",
            "/private/var/db",
        ]
        .iter()
        .any(|p| actual.starts_with(p))
        || (actual.starts_with("/usr") && !actual.starts_with("/usr/local"))
        || metadata.st_flags() & 0x00080000 != 0;
    if protected {
        return Err(failure("protected"));
    }
    let url = NSURL::fileURLWithPath_isDirectory(
        &NSString::from_str(actual.to_str().ok_or_else(|| failure("path"))?),
        true,
    );
    if application(&url)? {
        return Err(failure("application"));
    }
    let text = std::ffi::CString::new(actual.as_os_str().as_encoded_bytes())
        .map_err(|_| failure("path"))?;
    // Preflight is advisory; native writing must still handle actual permission errors.
    if unsafe { libc::access(text.as_ptr(), libc::W_OK | libc::X_OK) } != 0 {
        return Err(AppError::reason("permission_denied", "unwritable"));
    }
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::statfs(text.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(io_error(std::io::Error::last_os_error()));
    }
    if unsafe { stat.assume_init() }.f_flags & libc::MNT_RDONLY as u32 != 0 {
        return Err(AppError::reason("permission_denied", "read_only"));
    }
    Ok(Target {
        id: String::new(),
        aliases: vec![path.clone()],
        linked: path != actual,
        path,
        actual_path: actual,
        identity: Identity::read(&metadata),
    })
}
pub fn check_actual(target: &Target, path: &Path) -> Result<(), AppError> {
    let now = inspect(path.to_owned())?;
    if now.identity != target.identity || now.actual_path != target.actual_path {
        return Err(AppError::reason("target_changed", "changed").target(&target.id));
    }
    Ok(())
}
pub fn revalidate(target: &Target) -> Result<(), AppError> {
    for alias in &target.aliases {
        check_actual(target, alias)?;
    }
    Ok(())
}
fn execute(request: Request) -> Result<Checked, AppError> {
    match request {
        Request::Text(text) => Ok(Checked {
            mask: Some(crate::text::render(&text)?),
            ..Default::default()
        }),
        Request::Inspect(paths) => {
            if paths.is_empty() {
                return Err(failure("empty"));
            }
            if paths.len() > 4096 {
                return Err(failure("budget"));
            }
            let mut out = Checked::default();
            let mut seen = HashSet::new();
            let mut badges = Vec::new();
            let input_count = paths.len();
            for path in paths {
                match inspect(path.clone()) {
                    Ok(t) => {
                        if seen.insert((t.identity.device, t.identity.inode)) {
                            out.items.push(t);
                        } else if let Some(existing) = out.items.iter_mut().find(|e| {
                            e.identity.device == t.identity.device
                                && e.identity.inode == t.identity.inode
                        }) {
                            if t.linked && !existing.linked {
                                existing.path = t.path.clone();
                                existing.linked = true;
                            }
                            for alias in t.aliases {
                                if !existing.aliases.contains(&alias) {
                                    existing.aliases.push(alias);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        if error.params.get("reason").is_some_and(|v| v == "badge") {
                            badges.push(path.clone());
                        }
                        out.issues.push(Issue { path, error });
                    }
                }
            }
            if !badges.is_empty() {
                if !out.items.is_empty() || input_count != badges.len() {
                    return Err(failure("mixed"));
                }
                if badges.len() != 1 {
                    return Err(failure("multiple_badges"));
                }
                out.mask = Some(crate::assets::load(&badges[0])?);
            }
            if out.items.len() > 100 {
                return Err(AppError::reason("invalid_input", "limit").param("limit", 100));
            }
            Ok(out)
        }
        Request::Fingerprint(targets) => {
            let mut fingerprints = Vec::new();
            for target in &targets {
                revalidate(target)?;
                fingerprints.push(
                    crate::icon::fingerprint(&target.actual_path)
                        .map_err(|e| e.target(&target.id))?,
                );
                revalidate(target)?;
            }
            Ok(Checked {
                items: targets,
                issues: vec![],
                fingerprints,
                ..Default::default()
            })
        }
    }
}
// The worker performs only bounded-output reads; its parent can terminate blocked filesystem calls.
pub fn worker() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--target-check-worker") {
        return false;
    }
    // Also bound orphaned workers if the application exits during a filesystem call.
    unsafe {
        libc::alarm(11);
    }
    let result = objc2::rc::autoreleasepool(|_| {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > 1024 * 1024 {
            return Err(failure("budget"));
        }
        let request = serde_json::from_slice(&bytes).map_err(|_| failure("payload"))?;
        execute(request)
    });
    if let Ok(bytes) = serde_json::to_vec(&result) {
        let _ = std::io::stdout().write_all(&bytes);
    }
    true
}
// Command integration tests exercise the same inspector in-process; the real worker has a separate executable-level check.
#[cfg(test)]
pub fn check(request: Request) -> Result<Checked, AppError> {
    check_cancellable(request, || true)
}
#[cfg(test)]
pub fn check_cancellable(
    request: Request,
    current: impl Fn() -> bool,
) -> Result<Checked, AppError> {
    if !current() {
        return Err(failure("check"));
    }
    objc2::rc::autoreleasepool(|_| execute(request))
}
#[cfg(not(test))]
pub fn check(request: Request) -> Result<Checked, AppError> {
    check_cancellable(request, || true)
}
#[cfg(not(test))]
pub fn check_cancellable(
    request: Request,
    current: impl Fn() -> bool,
) -> Result<Checked, AppError> {
    static WORKER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _worker = WORKER.lock().map_err(|_| failure("check"))?;
    if !current() {
        return Err(failure("check"));
    }
    let input = serde_json::to_vec(&request).map_err(|_| failure("payload"))?;
    if input.len() > 1024 * 1024 {
        return Err(failure("budget"));
    }
    let mut command = Command::new(std::env::current_exe().map_err(io_error)?);
    command.arg("--target-check-worker");
    check_process(command, input, Duration::from_secs(10), current)
}
fn check_process(
    mut command: Command,
    input: Vec<u8>,
    budget: Duration,
    current: impl Fn() -> bool,
) -> Result<Checked, AppError> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(io_error)?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if current() && start.elapsed() < budget => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(AppError::reason("timeout", "check"));
            }
        }
    };
    let sent = writer.join();
    let received = reader.join();
    if !status?.success() || !matches!(sent, Ok(Ok(()))) {
        return Err(failure("check"));
    }
    let bytes = received.map_err(|_| failure("check"))?.map_err(io_error)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(failure("budget"));
    }
    serde_json::from_slice(&bytes).map_err(|_| failure("check"))?
}
pub fn next_id(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    static SESSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let session = SESSION.get_or_init(|| {
        let mut bytes = [0; 16];
        fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .expect("E_SESSION_RANDOM");
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    });
    format!(
        "{session}-{prefix}-{}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn root() -> Temp {
        let p = std::env::temp_dir().join(next_id("test"));
        fs::create_dir(&p).unwrap();
        Temp(fs::canonicalize(p).unwrap())
    }
    #[test]
    fn timed_out_readers_are_killed_and_reaped() {
        let mut command = Command::new("/bin/sleep");
        command.arg("5");
        let start = Instant::now();
        let err = check_process(command, vec![], Duration::from_millis(30), || true)
            .err()
            .unwrap();
        assert_eq!(err.code, "timeout");
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn invalidated_workers_are_killed_and_reaped() {
        let mut command = Command::new("/bin/sleep");
        command.arg("5");
        let start = Instant::now();
        assert!(check_process(command, vec![], Duration::from_secs(10), || false).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn permissions_are_checked_without_elevation() {
        use std::os::unix::fs::PermissionsExt;
        let root = root();
        let p = root.0.join("readonly");
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o555)).unwrap();
        let out = execute(Request::Inspect(vec![p.clone()])).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(out.items.is_empty());
        assert_eq!(out.issues[0].error.code, "permission_denied");
    }
    #[test]
    fn input_rules_deduplicate_and_preserve_special_paths() {
        let root = root();
        let p = root.0.join(" 中文\n\"\\ ");
        fs::create_dir(&p).unwrap();
        let link = root.0.join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        let bad = root.0.join("missing");
        let out = execute(Request::Inspect(vec![p.clone(), link.clone(), bad])).unwrap();
        assert_eq!(out.items.len(), 1);
        assert_eq!(out.items[0].actual_path, p);
        assert_eq!(out.items[0].aliases.len(), 2);
        assert_eq!(out.issues.len(), 1);
        let linked = execute(Request::Inspect(vec![link])).unwrap();
        assert!(linked.items[0].linked);
        let mut t = linked.items;
        t[0].id = next_id("target");
        fs::rename(&p, root.0.join("moved")).unwrap();
        fs::create_dir(&p).unwrap();
        assert!(revalidate(&t[0]).is_err());
    }
    #[test]
    fn imports_are_atomic_and_return_only_safe_masks() {
        let root = root();
        let one = root.0.join("badge.svg");
        std::fs::write(
            &one,
            include_bytes!("../../tests/fixtures/badges/geometric-cross.svg"),
        )
        .unwrap();
        let checked = execute(Request::Inspect(vec![one.clone()])).unwrap();
        assert!(checked.items.is_empty());
        assert_eq!(checked.mask.unwrap().bounds, [0, 0, 384, 384]);
        assert_eq!(
            execute(Request::Inspect(vec![one.clone(), one.clone()]))
                .err()
                .unwrap()
                .params["reason"],
            "multiple_badges"
        );
        assert_eq!(
            execute(Request::Inspect(vec![one.clone(), root.0.clone()]))
                .err()
                .unwrap()
                .params["reason"],
            "mixed"
        );
        std::fs::write(one.clone(), b"broken").unwrap();
        assert_eq!(
            execute(Request::Inspect(vec![one])).err().unwrap().code,
            "asset_error"
        );
    }
    #[test]
    fn unique_limit_is_after_deduplication_and_badges_are_atomic() {
        let root = root();
        let many = vec![root.0.clone(); 101];
        assert_eq!(execute(Request::Inspect(many)).unwrap().items.len(), 1);
        let paths: Vec<_> = (0..101)
            .map(|n| {
                let p = root.0.join(n.to_string());
                fs::create_dir(&p).unwrap();
                p
            })
            .collect();
        assert_eq!(
            execute(Request::Inspect(paths[..100].to_vec()))
                .unwrap()
                .items
                .len(),
            100
        );
        assert!(execute(Request::Inspect(paths)).is_err());
        let badge = root.0.join("image.png");
        fs::write(&badge, []).unwrap();
        assert!(execute(Request::Inspect(vec![root.0.clone(), badge])).is_err());
    }
    #[test]
    fn rejects_protected_apps_and_broken_links() {
        let root = root();
        let app = root.0.join("Fixture.app");
        fs::create_dir(&app).unwrap();
        let bad = root.0.join("loop");
        std::os::unix::fs::symlink(&bad, &bad).unwrap();
        let out = execute(Request::Inspect(vec!["/System".into(), app, bad])).unwrap();
        assert!(out.items.is_empty());
        assert_eq!(out.issues.len(), 3);
    }
}
