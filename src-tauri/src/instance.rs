use std::{
    fs::{File, OpenOptions},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    time::{Duration, Instant},
};
use tauri::Manager;
// Only an activation signal crosses processes; native input stays in the owning app.
pub fn install(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let directory = app.path().app_data_dir()?;
    std::fs::create_dir_all(&directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("instance.lock"))?;
    let socket = directory.join("activate.sock");
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
