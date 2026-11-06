use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    revision: u64,
    #[serde(skip)]
    request: u64,
    source: String,
    items: Vec<InputItem>,
    error: Option<String>,
    choosing: bool,
    reading: bool,
    checking: bool,
}
#[derive(Clone, serde::Serialize)]
pub struct InputItem {
    path: PathBuf,
    kind: &'static str,
}
#[derive(Default)]
pub struct Inbox(Mutex<Snapshot>);
fn update(app: &tauri::AppHandle, f: impl FnOnce(&mut Snapshot)) {
    let state = app.state::<Inbox>();
    let mut state = state.0.lock().unwrap();
    f(&mut state);
    state.revision += 1;
    drop(state);
    let _ = app.emit("entry-changed", ());
}
pub fn error(app: &tauri::AppHandle, code: &str) {
    update(app, |s| s.error = Some(code.into()));
}
pub fn receive(app: &tauri::AppHandle, source: &str, paths: Vec<PathBuf>) {
    show(app);
    let app = app.clone();
    let source = source.to_owned();
    let revision = {
        let state = app.state::<Inbox>();
        let mut s = state.0.lock().unwrap();
        if s.choosing || s.checking {
            drop(s);
            error(&app, "busy");
            return;
        }
        s.checking = true;
        s.request += 1;
        s.revision += 1;
        s.request
    };
    let _ = app.emit("entry-changed", ());
    tauri::async_runtime::spawn_blocking(move || {
        let checked = check_paths(paths);
        update(&app, |s| {
            s.checking = false;
            if s.request != revision {
                return;
            }
            match checked {
                Ok(items) => accept(s, &source, items),
                Err(code) => s.error = Some(code.into()),
            }
        });
    });
}
fn check_paths(paths: Vec<PathBuf>) -> Result<Vec<InputItem>, &'static str> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    if paths.is_empty() {
        return Err("empty");
    }
    if paths.len() > 100 {
        return Err("limit");
    }
    let start = Instant::now();
    paths
        .into_iter()
        .map(|path| {
            if !path.is_absolute() {
                return Err("invalid");
            }
            // Isolate potentially blocking filesystem queries in a cancellable system process.
            let mut child = Command::new("/usr/bin/stat")
                .args(["-L", "-f", "%HT"])
                .arg(&path)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| "checkFailed")?;
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("checkFailed");
                    }
                }
            };
            let mut output = String::new();
            child
                .stdout
                .take()
                .unwrap()
                .take(128)
                .read_to_string(&mut output)
                .map_err(|_| "checkFailed")?;
            if !status.success() {
                return Err("invalid");
            }
            let kind = match output.trim_end() {
                "Directory" => "directory",
                "Regular File"
                    if path.extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("svg")
                    }) =>
                {
                    "badge"
                }
                _ => return Err("invalid"),
            };
            Ok(InputItem { path, kind })
        })
        .collect()
}
fn accept(s: &mut Snapshot, source: &str, items: Vec<InputItem>) {
    if s.choosing {
        s.error = Some("busy".into());
        return;
    }
    if items.is_empty() {
        s.error = Some("empty".into());
        return;
    }
    if items.len() > 100 {
        s.error = Some("limit".into());
        return;
    }
    if items.iter().any(|i| i.kind == "invalid")
        || items.iter().any(|i| i.kind != items[0].kind)
        || (items[0].kind == "badge" && items.len() != 1)
    {
        s.error = Some("invalid".into());
        return;
    }
    s.source = source.into();
    s.items = items;
    s.error = None;
}
pub fn show(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
pub fn wake(app: &tauri::AppHandle) {
    let hidden = app
        .get_webview_window("main")
        .is_some_and(|w| !w.is_visible().unwrap_or(true));
    let automatic = {
        let s = app.state::<Inbox>();
        let s = s.0.lock().unwrap();
        hidden && (s.items.is_empty() || s.source == "finder-auto")
    };
    let finder = crate::macos::finder_is_frontmost();
    show(app);
    if automatic {
        if finder {
            read_finder(app.clone(), true);
        } else {
            update(app, |s| {
                s.items.clear();
                s.source.clear();
                s.error = Some("finderInactive".into());
            });
        }
    }
}
#[tauri::command]
pub fn get_entry(app: tauri::AppHandle) -> Snapshot {
    app.state::<Inbox>().0.lock().unwrap().clone()
}
#[tauri::command]
pub fn choose_targets(app: tauri::AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    {
        let state = app.state::<Inbox>();
        let mut s = state.0.lock().unwrap();
        if s.choosing || s.reading || s.checking {
            return;
        }
        s.choosing = true;
    }
    app.dialog().file().pick_folders(move |files| {
        update(&app, |s| s.choosing = false);
        if let Some(files) = files {
            let paths = files
                .into_iter()
                .filter_map(|f| f.into_path().ok())
                .collect();
            receive(&app, "picker", paths);
        }
    });
}
#[tauri::command]
pub fn finder_selection(app: tauri::AppHandle) {
    read_finder(app, false);
}
fn read_finder(app: tauri::AppHandle, automatic: bool) {
    let revision = {
        let state = app.state::<Inbox>();
        let mut s = state.0.lock().unwrap();
        if s.reading || s.choosing || s.checking {
            return;
        }
        s.reading = true;
        s.revision += 1;
        s.request += 1;
        s.request
    };
    let _ = app.emit("entry-changed", ());
    tauri::async_runtime::spawn_blocking(move || {
        let result = finder_paths().and_then(check_paths);
        update(&app, |s| {
            let current = s.request == revision;
            s.reading = false;
            if !current {
                return;
            }
            finish_finder(s, automatic, result);
        });
    });
}
fn finish_finder(s: &mut Snapshot, automatic: bool, result: Result<Vec<InputItem>, &'static str>) {
    if automatic {
        s.items.clear();
        s.source.clear();
    }
    match result {
        Ok(items) => accept(s, if automatic { "finder-auto" } else { "finder" }, items),
        Err(code) => s.error = Some(code.into()),
    }
}
fn finder_paths() -> Result<Vec<PathBuf>, &'static str> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let script = r#"try { var f = Application('com.apple.finder'); JSON.stringify({paths: f.selection().map(x => ObjC.unwrap($.NSURL.URLWithString(x.url()).path))}); } catch(e) { JSON.stringify({denied: e.errorNumber === -1743}); }"#;
    let mut child = Command::new("/usr/bin/osascript")
        .args([
            "-l",
            "JavaScript",
            "-e",
            "ObjC.import('Foundation');",
            "-e",
            script,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "finderFailed")?;
    let pipe = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = pipe.take(1024 * 1024).read_to_end(&mut bytes);
        (result, bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err("finderTimeout");
            }
        }
    };
    let (read, bytes) = reader.join().map_err(|_| "finderFailed")?;
    if !status?.success() || read.is_err() || bytes.len() >= 1024 * 1024 {
        return Err("finderFailed");
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "finderFailed")?;
    if value["denied"] == true {
        return Err("finderDenied");
    }
    serde_json::from_value(value["paths"].clone()).map_err(|_| "finderFailed")
}

#[cfg(test)]
mod tests {
    #[test]
    fn invalid_and_busy_inputs_preserve_valid_editing() {
        let mut s = super::Snapshot::default();
        super::accept(
            &mut s,
            "picker",
            super::check_paths(vec![std::env::current_dir().unwrap()]).unwrap(),
        );
        assert_eq!(s.items.len(), 1);
        super::accept(&mut s, "service", vec![]);
        assert_eq!(s.source, "picker");
        assert_eq!(s.items.len(), 1);
        super::accept(
            &mut s,
            "service",
            vec![super::InputItem {
                path: "/nonexistent/folderify-probe".into(),
                kind: "invalid",
            }],
        );
        assert_eq!(s.error.as_deref(), Some("invalid"));
        super::accept(
            &mut s,
            "service",
            (0..101)
                .map(|_| super::InputItem {
                    path: std::env::current_dir().unwrap(),
                    kind: "directory",
                })
                .collect(),
        );
        assert_eq!(s.error.as_deref(), Some("limit"));
        s.choosing = true;
        super::accept(
            &mut s,
            "service",
            super::check_paths(vec![std::env::temp_dir()]).unwrap(),
        );
        assert_eq!(s.error.as_deref(), Some("busy"));
        assert_eq!(s.source, "picker");
    }
    #[test]
    fn failed_automatic_reads_clear_old_targets_but_manual_reads_preserve_them() {
        for automatic in [true, false] {
            for result in [Ok(vec![]), Err("invalid"), Err("finderDenied")] {
                let mut s = super::Snapshot::default();
                super::accept(
                    &mut s,
                    "finder-auto",
                    vec![super::InputItem {
                        path: "/tmp/old".into(),
                        kind: "directory",
                    }],
                );
                super::finish_finder(&mut s, automatic, result);
                assert_eq!(s.items.is_empty(), automatic);
                assert!(s.error.is_some());
            }
        }
    }
    #[test]
    fn snapshot_preserves_special_paths() {
        let path = std::path::PathBuf::from("/tmp/ space\n中文\"\\ ");
        let item = super::InputItem {
            path: path.clone(),
            kind: "directory",
        };
        assert_eq!(
            serde_json::to_value(item).unwrap()["path"],
            path.to_str().unwrap()
        );
    }
}
