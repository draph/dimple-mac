use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    os::unix::{
        ffi::OsStrExt,
        fs::MetadataExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::Manager;
// macOS stores at most 104 bytes in sockaddr_un.sun_path, including the trailing NUL.
const MAX_SOCKET_PATH_BYTES: usize = 103;
fn fits_unix_socket_limit(path: &Path) -> bool {
    path.as_os_str().as_bytes().len() <= MAX_SOCKET_PATH_BYTES
}
// The Darwin per-user temp directory is private and stable across launches.
// Unlike TMPDIR, callers cannot redirect it, and other users cannot pre-create the path.
fn parse_confstr_path(bytes: &[u8]) -> Option<PathBuf> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let path = Path::new(OsStr::from_bytes(&bytes[..end]));
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return None;
    }
    Some(path.to_path_buf())
}
fn darwin_user_temp_dir() -> Option<PathBuf> {
    // SAFETY: confstr writes at most the supplied buffer length and returns the required size.
    let size = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null_mut(), 0) };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size];
    let written = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if written == 0 || written > buffer.len() {
        return None;
    }
    buffer.truncate(written);
    parse_confstr_path(&buffer)
}
fn short_hash(bytes: &[u8]) -> u64 {
    // FNV-1a keeps the fallback name deterministic without another dependency.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
fn fallback_name(data_dir: &Path, uid: u32) -> String {
    let hash = short_hash(data_dir.as_os_str().as_bytes());
    format!("folderify-desktop-{uid}-{hash:016x}.sock")
}
fn choose_socket_path(
    data_dir: &Path,
    user_temp_dir: Option<&Path>,
    uid: u32,
) -> Result<PathBuf, &'static str> {
    let preferred = data_dir.join("activate.sock");
    if fits_unix_socket_limit(&preferred) {
        return Ok(preferred);
    }
    let fallback = user_temp_dir
        .ok_or("E_INSTANCE_SOCKET")?
        .join(fallback_name(data_dir, uid));
    if fits_unix_socket_limit(&fallback) {
        Ok(fallback)
    } else {
        Err("E_INSTANCE_SOCKET")
    }
}
// Only an activation signal crosses processes; native input stays in the owning app.
pub fn install(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let directory = app.path().app_data_dir()?;
    std::fs::create_dir_all(&directory)?;
    // Canonical text keeps the fallback identity stable across equivalent path spellings.
    let directory = std::fs::canonicalize(directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("instance.lock"))?;
    let uid = std::fs::metadata(&directory)?.uid();
    let socket = choose_socket_path(&directory, darwin_user_temp_dir().as_deref(), uid)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            let start = Instant::now();
            loop {
                if UnixStream::connect(&socket).is_ok() {
                    std::process::exit(0);
                }
                if start.elapsed() >= Duration::from_secs(10) {
                    return Err("E_INSTANCE_UNAVAILABLE".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        Err(error) => return Err(error.into()),
    }
    // The lifetime lock makes stale socket removal and bind exclusive.
    match std::fs::remove_file(&socket) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let listener = UnixListener::bind(&socket)?;
    app.manage(Instance {
        _lock: lock,
        socket,
    });
    let app = app.clone();
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            match connection {
                Ok(_) => crate::entry::show(&app),
                Err(_) => break,
            }
        }
    });
    Ok(())
}
struct Instance {
    _lock: File,
    socket: PathBuf,
}
impl Drop for Instance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const LONG_DATA_DIR: &str = "/Users/a-very-long-account-name-exceeding-limit/Library/Application Support/dev.efun.folderify-desktop";
    #[test]
    fn preferred_socket_path_is_kept_when_it_fits() {
        let data_dir =
            Path::new("/Users/example/Library/Application Support/dev.efun.folderify-desktop");
        assert_eq!(
            choose_socket_path(data_dir, None, 501),
            Ok(data_dir.join("activate.sock"))
        );
    }
    #[test]
    fn long_data_directories_fall_back_to_the_private_user_temp_dir() {
        let data_dir = Path::new(LONG_DATA_DIR);
        let temp = Path::new("/var/folders/xx/T");
        assert!(!fits_unix_socket_limit(&data_dir.join("activate.sock")));
        let selected = choose_socket_path(data_dir, Some(temp), 501).unwrap();
        assert_eq!(selected, temp.join(fallback_name(data_dir, 501)));
        assert!(fits_unix_socket_limit(&selected));
    }
    #[test]
    fn fallback_names_are_stable_and_identity_scoped() {
        let app_a = Path::new(LONG_DATA_DIR);
        let app_b = Path::new(
            "/Users/a-very-long-account-name-exceeding-limit/Library/Application Support/dev.efun.folderify-desktop-dev",
        );
        assert_eq!(fallback_name(app_a, 501), fallback_name(app_a, 501));
        assert_ne!(fallback_name(app_a, 501), fallback_name(app_b, 501));
        assert_ne!(fallback_name(app_a, 501), fallback_name(app_a, 502));
    }
    #[test]
    fn missing_or_too_long_user_temp_directories_fail_closed() {
        let data_dir = Path::new(LONG_DATA_DIR);
        assert_eq!(
            choose_socket_path(data_dir, None, 501),
            Err("E_INSTANCE_SOCKET")
        );
        let temp = Path::new(
            "/a/very/long/temporary/directory/path/that/exceeds/the/limit/for/unix/sockets",
        );
        assert!(!fits_unix_socket_limit(
            &temp.join(fallback_name(data_dir, 501))
        ));
        assert_eq!(
            choose_socket_path(data_dir, Some(temp), 501),
            Err("E_INSTANCE_SOCKET")
        );
    }
    #[test]
    fn confstr_paths_must_be_absolute_and_non_empty() {
        assert_eq!(
            parse_confstr_path(b"/var/folders/xx/T/\0"),
            Some(PathBuf::from("/var/folders/xx/T/"))
        );
        assert_eq!(
            parse_confstr_path(b"/var/folders/xx/T/"),
            Some(PathBuf::from("/var/folders/xx/T/"))
        );
        assert_eq!(parse_confstr_path(b"\0"), None);
        assert_eq!(parse_confstr_path(b"relative/temp/\0"), None);
    }
    #[test]
    fn darwin_user_temp_dir_is_an_absolute_path() {
        // Path length varies by machine; only the absolute shape is guaranteed here.
        // Fallback length handling uses a synthetic temp directory in the tests above.
        let temp = darwin_user_temp_dir().expect("macOS per-user temp directory");
        assert!(temp.is_absolute());
    }
}
