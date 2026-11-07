use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};
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
    #[serde(rename = "reading", serialize_with = "serialize_reading")]
    active_read: Option<u64>,
    #[serde(skip)]
    read_counter: u64,
    checking: bool,
}
fn serialize_reading<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bool(value.is_some())
}
#[derive(Clone, serde::Serialize)]
pub struct InputItem {
    path: PathBuf,
    kind: &'static str,
}
#[derive(Default)]
pub struct Inbox(Mutex<Snapshot>);
fn commit_update(app: &tauri::AppHandle, mut state: MutexGuard<'_, Snapshot>) {
    state.revision += 1;
    drop(state);
    let _ = app.emit("entry-changed", ());
}
fn update(app: &tauri::AppHandle, f: impl FnOnce(&mut Snapshot)) {
    let state = app.state::<Inbox>();
    let mut state = state.0.lock().unwrap();
    f(&mut state);
    commit_update(app, state);
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
        let revision = s.request;
        commit_update(&app, s);
        revision
    };
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
    if items.iter().any(|i| i.kind != "directory") {
        s.error = Some("badgeUnavailable".into());
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
    let (automatic, _) = {
        let state = app.state::<Inbox>();
        let mut state = state.0.lock().unwrap();
        let plan = plan_wake(&mut state, hidden);
        if plan.1 {
            commit_update(app, state);
        } else {
            drop(state);
        }
        plan
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
pub fn remove_target(app: tauri::AppHandle, expected_revision: u64, index: usize) {
    let inbox = app.state::<Inbox>();
    let mut state = inbox.0.lock().unwrap();
    remove(&mut state, expected_revision, index);
    commit_update(&app, state);
}
fn remove(s: &mut Snapshot, expected_revision: u64, index: usize) {
    if s.revision != expected_revision || index >= s.items.len() {
        s.error = Some("changed".into());
        return;
    }
    if s.choosing || s.checking || s.active_read.is_some() {
        s.error = Some("busy".into());
        return;
    }
    s.request += 1;
    invalidate_read(s);
    s.items.remove(index);
    s.source = if s.items.is_empty() { "" } else { "manual" }.into();
    s.error = None;
}
#[tauri::command]
pub fn choose_targets(app: tauri::AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    {
        let state = app.state::<Inbox>();
        let mut s = state.0.lock().unwrap();
        if s.choosing || s.active_read.is_some() || s.checking {
            return;
        }
        s.choosing = true;
        commit_update(&app, s);
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
    let (revision, generation) = {
        let state = app.state::<Inbox>();
        let mut state = state.0.lock().unwrap();
        if state.active_read.is_some() || state.choosing || state.checking {
            return;
        }
        state.read_counter += 1;
        state.active_read = Some(state.read_counter);
        state.request += 1;
        let revision = state.request;
        let generation = state.read_counter;
        commit_update(&app, state);
        (revision, generation)
    };
    tauri::async_runtime::spawn_blocking(move || {
        if !read_is_current(&app, generation) {
            return;
        }
        let result = match finder_paths() {
            Ok(paths) if read_is_current(&app, generation) => check_paths(paths),
            Ok(_) => return,
            Err(error) => Err(error),
        };
        let state = app.state::<Inbox>();
        let mut state = state.0.lock().unwrap();
        if state.active_read != Some(generation) {
            return;
        }
        complete_read(&mut state, generation, revision, automatic, result);
        commit_update(&app, state);
    });
}
fn read_is_current(app: &tauri::AppHandle, generation: u64) -> bool {
    let state = app.state::<Inbox>();
    let s = state.0.lock().unwrap();
    s.active_read == Some(generation)
}
fn plan_wake(s: &mut Snapshot, hidden: bool) -> (bool, bool) {
    let automatic = hidden && (s.items.is_empty() || s.source == "finder-auto");
    // A new activation decides again; any read already in flight must not answer it.
    let invalidated = automatic && invalidate_read(s);
    (automatic, invalidated)
}
fn invalidate_read(s: &mut Snapshot) -> bool {
    if s.active_read.is_none() {
        return false;
    }
    s.active_read = None;
    true
}
fn complete_read(
    s: &mut Snapshot,
    generation: u64,
    revision: u64,
    automatic: bool,
    result: Result<Vec<InputItem>, &'static str>,
) {
    // A newer read owns the state; this result cannot clear or replace it.
    if s.active_read != Some(generation) {
        return;
    }
    s.active_read = None;
    if s.request != revision {
        return;
    }
    finish_finder(s, automatic, result);
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
    fn targets() -> super::Snapshot {
        super::Snapshot {
            revision: 7,
            source: "finder-auto".into(),
            items: ["first", "middle", "last"]
                .into_iter()
                .map(|name| super::InputItem {
                    path: format!("/tmp/{name}").into(),
                    kind: "directory",
                })
                .collect(),
            ..Default::default()
        }
    }
    #[test]
    fn removal_preserves_order_and_becomes_manual_editing() {
        for index in 0..3 {
            let mut s = targets();
            let expected: Vec<_> = s
                .items
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, item)| item.path.clone())
                .collect();
            super::remove(&mut s, 7, index);
            assert_eq!(
                s.items.iter().map(|i| i.path.clone()).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(s.source, "manual");
            assert_eq!(super::plan_wake(&mut s, true), (false, false));
            super::remove(&mut s, 7, 0);
            super::remove(&mut s, 7, 0);
            assert!(s.items.is_empty());
            assert!(s.source.is_empty());
        }
    }
    #[test]
    fn stale_invalid_and_busy_removals_preserve_targets() {
        for case in 0..5 {
            let mut s = targets();
            s.choosing = case == 2;
            s.checking = case == 3;
            s.active_read = (case == 4).then_some(1);
            let revision = if case == 0 { 6 } else { 7 };
            let index = if case == 1 { usize::MAX } else { 0 };
            super::remove(&mut s, revision, index);
            assert_eq!(s.items.len(), 3);
            assert_eq!(s.source, "finder-auto");
            assert_eq!(
                s.error.as_deref(),
                Some(if case < 2 { "changed" } else { "busy" })
            );
        }
    }
    #[test]
    fn removed_targets_cannot_be_restored_by_a_superseded_read() {
        let mut s = targets();
        super::remove(&mut s, 7, 0);
        super::complete_read(&mut s, 1, 0, true, Ok(targets().items));
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.source, "manual");
    }
    #[test]
    fn badge_input_keeps_the_current_targets() {
        let mut s = targets();
        super::accept(
            &mut s,
            "window-drop",
            vec![super::InputItem {
                path: "/tmp/input.svg".into(),
                kind: "badge",
            }],
        );
        assert_eq!(s.error.as_deref(), Some("badgeUnavailable"));
        assert_eq!(s.items.len(), 3);
        assert_eq!(s.source, "finder-auto");
        super::finish_finder(
            &mut s,
            false,
            Ok(vec![super::InputItem {
                path: "/tmp/input.png".into(),
                kind: "badge",
            }]),
        );
        assert_eq!(s.items.len(), 3);
    }
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
    fn stale_reads_do_not_replace_a_newer_wake_decision() {
        let mut s = super::Snapshot {
            active_read: Some(1),
            read_counter: 1,
            request: 1,
            ..Default::default()
        };
        assert!(super::invalidate_read(&mut s));
        assert_eq!(s.active_read, None);
        s.error = Some("finderInactive".into());
        super::complete_read(
            &mut s,
            1,
            1,
            true,
            Ok(vec![super::InputItem {
                path: "/tmp/old".into(),
                kind: "directory",
            }]),
        );
        assert!(s.items.is_empty());
        assert_eq!(s.error.as_deref(), Some("finderInactive"));
    }
    #[test]
    fn invalidated_reads_do_not_clear_a_newer_read() {
        let mut s = super::Snapshot {
            active_read: Some(1),
            read_counter: 1,
            request: 1,
            ..Default::default()
        };
        assert!(super::invalidate_read(&mut s));
        s.active_read = Some(2);
        s.read_counter = 2;
        s.request += 1;
        super::complete_read(&mut s, 1, 1, true, Err("finderFailed"));
        assert_eq!(s.active_read, Some(2));
        assert_eq!(s.error, None);
        let (generation, revision) = (s.read_counter, s.request);
        super::complete_read(
            &mut s,
            generation,
            revision,
            true,
            Ok(vec![super::InputItem {
                path: "/tmp/new".into(),
                kind: "directory",
            }]),
        );
        assert_eq!(s.active_read, None);
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.source, "finder-auto");
    }
    #[test]
    fn wake_plan_invalidates_reads_only_for_an_automatic_activation() {
        let mut s = super::Snapshot {
            active_read: Some(1),
            read_counter: 1,
            ..Default::default()
        };
        assert_eq!(super::plan_wake(&mut s, true), (true, true));
        assert_eq!(s.active_read, None);

        let mut s = super::Snapshot {
            active_read: Some(1),
            read_counter: 1,
            items: vec![super::InputItem {
                path: "/tmp/manual".into(),
                kind: "directory",
            }],
            source: "picker".into(),
            ..Default::default()
        };
        assert_eq!(super::plan_wake(&mut s, true), (false, false));
        assert_eq!(s.active_read, Some(1));

        let mut s = super::Snapshot {
            active_read: Some(1),
            read_counter: 1,
            items: vec![super::InputItem {
                path: "/tmp/auto".into(),
                kind: "directory",
            }],
            source: "finder-auto".into(),
            ..Default::default()
        };
        assert_eq!(super::plan_wake(&mut s, true), (true, true));
        assert_eq!(s.active_read, None);

        let mut s = super::Snapshot {
            active_read: Some(1),
            read_counter: 1,
            ..Default::default()
        };
        assert_eq!(super::plan_wake(&mut s, false), (false, false));
        assert_eq!(s.active_read, Some(1));
    }
    #[test]
    fn invalidate_read_clears_any_in_flight_read() {
        let mut s = super::Snapshot {
            active_read: Some(1),
            ..Default::default()
        };
        assert!(super::invalidate_read(&mut s));
        assert_eq!(s.active_read, None);
        assert!(!super::invalidate_read(&mut s));
    }
    #[test]
    fn superseded_input_keeps_a_completed_read_from_applying() {
        let mut s = super::Snapshot {
            active_read: Some(1),
            read_counter: 1,
            request: 5,
            ..Default::default()
        };
        super::complete_read(&mut s, 1, 4, false, Err("finderDenied"));
        assert_eq!(s.active_read, None);
        assert_eq!(s.error, None);
    }
    #[test]
    fn snapshot_serializes_active_read_as_boolean() {
        let mut s = super::Snapshot::default();
        assert_eq!(serde_json::to_value(&s).unwrap()["reading"], false);
        s.active_read = Some(7);
        assert_eq!(serde_json::to_value(&s).unwrap()["reading"], true);
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
