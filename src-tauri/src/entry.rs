use crate::{localization::AppError, operation, targets};
use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    badge: Badge,
    badge_error: Option<AppError>,
    importing: bool,
    #[serde(skip)]
    mask: Option<std::sync::Arc<crate::assets::Mask>>,
    revision: u64,
    edit_revision: u64,
    pinned: bool,
    #[serde(skip)]
    cleanup: Option<(String, u64)>,
    #[serde(skip)]
    request: u64,
    source: String,
    items: Vec<InputItem>,
    error: Option<AppError>,
    issues: Vec<InputItem>,
    preparing: bool,
    #[serde(skip)]
    prepared: Option<std::sync::Arc<operation::Prepared>>,
    #[serde(skip)]
    operation: Option<operation::Progress>,
    #[serde(skip)]
    quit_pending: bool,
    choosing: bool,
    #[serde(rename = "reading", serialize_with = "serialize_reading")]
    active_read: Option<u64>,
    #[serde(skip)]
    read_counter: u64,
    checking: bool,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Badge {
    id: String,
    width: u32,
    height: u32,
    opaque: bool,
}
impl Default for Badge {
    fn default() -> Self {
        Self {
            id: "code".into(),
            width: 384,
            height: 256,
            opaque: false,
        }
    }
}
impl Snapshot {
    fn operation_locked(&self) -> bool {
        self.prepared.is_some() || self.operation.as_ref().is_some_and(|p| p.busy())
    }
}
fn serialize_reading<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bool(value.is_some())
}
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputItem {
    #[serde(skip)]
    mask: Option<crate::assets::Mask>,
    id: String,
    actual_path: PathBuf,
    linked: bool,
    #[serde(skip)]
    identity: Option<targets::Identity>,
    #[serde(skip)]
    aliases: Vec<PathBuf>,
    error: Option<AppError>,
    path: PathBuf,
    kind: &'static str,
}
#[derive(Default)]
pub struct Inbox(Mutex<Snapshot>);
fn commit_update<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    mut state: MutexGuard<'_, Snapshot>,
) {
    state.revision += 1;
    let cleanup = if state.cleanup.is_none() && all_succeeded(&state) {
        let token = (
            state.operation.as_ref().unwrap().operation_id.clone(),
            state.edit_revision,
        );
        state.cleanup = Some(token.clone());
        Some(token)
    } else {
        None
    };
    drop(state);
    let _ = app.emit("entry-changed", ());
    if let Some(token) = cleanup {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            loop {
                let (tx, rx) = std::sync::mpsc::channel();
                let on_main = app.clone();
                let token = token.clone();
                if app
                    .run_on_main_thread(move || {
                        let _ = tx.send(finish_success(&on_main, &token));
                    })
                    .is_err()
                {
                    return;
                }
                if rx.recv() != Ok(true) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
    }
}
fn finish_success<R: tauri::Runtime>(app: &tauri::AppHandle<R>, token: &(String, u64)) -> bool {
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    if s.cleanup.as_ref() != Some(token) {
        return false;
    }
    if candidate_pending(&s) {
        return true;
    }
    if release_success(&mut s, token) {
        // This runs on the main thread, so hide completes before a later input can show.
        if !s.pinned
            && let Some(w) = app.get_webview_window("main")
        {
            let _ = w.hide();
        }
        commit_update(app, s);
    }
    false
}
fn candidate_pending(s: &Snapshot) -> bool {
    s.choosing || s.checking || s.importing || s.active_read.is_some() || s.preparing
}
fn all_succeeded(s: &Snapshot) -> bool {
    s.operation.as_ref().is_some_and(|p| {
        p.phase == "completed"
            && !p.items.is_empty()
            && p.items.iter().all(|i| i.status == "succeeded")
    })
}
fn edited(s: &mut Snapshot) {
    s.edit_revision += 1;
    s.cleanup = None;
    s.prepared = None;
    s.operation = None;
}
fn release_success(s: &mut Snapshot, token: &(String, u64)) -> bool {
    if s.cleanup.as_ref() != Some(token)
        || s.edit_revision != token.1
        || !all_succeeded(s)
        || candidate_pending(s)
        || s.operation
            .as_ref()
            .is_none_or(|p| p.operation_id != token.0)
    {
        return false;
    }
    s.items.clear();
    s.source.clear();
    s.issues.clear();
    s.error = None;
    s.request += 1;
    edited(s);
    true
}
fn update<R: tauri::Runtime>(app: &tauri::AppHandle<R>, f: impl FnOnce(&mut Snapshot)) {
    let state = app.state::<Inbox>();
    let mut state = state.0.lock().unwrap();
    f(&mut state);
    commit_update(app, state);
}
pub fn error<R: tauri::Runtime>(app: &tauri::AppHandle<R>, code: &str) {
    update(app, |s| s.error = Some(code.into()));
}
pub fn receive<R: tauri::Runtime>(app: &tauri::AppHandle<R>, source: &str, paths: Vec<PathBuf>) {
    receive_paths(app, source, paths, false);
}
fn receive_paths<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    source: &str,
    paths: Vec<PathBuf>,
    from_picker: bool,
) {
    let app = app.clone();
    let source = source.to_owned();
    let revision = {
        let state = app.state::<Inbox>();
        let mut s = state.0.lock().unwrap();
        show(&app);
        if s.operation_locked()
            || (s.choosing && !from_picker)
            || (s.checking && !s.importing)
            || s.preparing
        {
            drop(s);
            error(&app, "busy");
            return;
        }
        s.choosing = false;
        invalidate_read(&mut s);
        s.checking = true;
        s.importing = paths.len() == 1
            && paths[0]
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("svg"));
        s.request += 1;
        let revision = s.request;
        commit_update(&app, s);
        revision
    };
    tauri::async_runtime::spawn_blocking(move || {
        let checked = check_native_paths_current(paths, || {
            app.state::<Inbox>().0.lock().unwrap().request == revision
        });
        update(&app, |s| finish_native_input(s, revision, &source, checked));
    });
}
fn finish_native_input(
    s: &mut Snapshot,
    request: u64,
    source: &str,
    checked: Result<Vec<InputItem>, AppError>,
) {
    if s.request != request {
        return;
    }
    match checked {
        Ok(items) => finish_input(s, request, source, Ok(items)),
        Err(e) => {
            let import_failed = e.code == "asset_error"
                || (s.importing
                    && !matches!(
                        e.params.get("reason").and_then(|v| v.as_str()),
                        Some("mixed" | "multiple_badges")
                    ));
            s.checking = false;
            s.importing = false;
            if import_failed {
                s.badge_error = Some(if e.code == "asset_error" {
                    e
                } else {
                    AppError::reason(
                        "asset_error",
                        if e.code == "timeout" {
                            "timeout"
                        } else {
                            "unavailable"
                        },
                    )
                });
            } else {
                s.error = Some(e);
            }
        }
    }
}
fn finish_input(
    s: &mut Snapshot,
    request: u64,
    source: &str,
    checked: Result<Vec<InputItem>, &'static str>,
) {
    if s.request != request {
        return;
    }
    s.checking = false;
    s.importing = false;
    match checked {
        Ok(items) => accept(s, source, items),
        Err(code) => s.error = Some(code.into()),
    }
}

#[cfg(test)]
fn check_paths(paths: Vec<PathBuf>) -> Result<Vec<InputItem>, &'static str> {
    check_paths_current(paths, || true)
}
fn check_paths_current(
    paths: Vec<PathBuf>,
    current: impl Fn() -> bool,
) -> Result<Vec<InputItem>, &'static str> {
    let badge_candidate = paths.len() == 1
        && paths[0]
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("svg"));
    finder_check_result(badge_candidate, check_native_paths_current(paths, current))
}
fn finder_check_result(
    badge_candidate: bool,
    result: Result<Vec<InputItem>, AppError>,
) -> Result<Vec<InputItem>, &'static str> {
    match result {
        Err(e) if e.code == "asset_error" || badge_candidate => Ok(vec![InputItem {
            kind: "badge",
            error: Some(if e.code == "asset_error" {
                e
            } else {
                AppError::reason(
                    "asset_error",
                    if e.code == "timeout" {
                        "timeout"
                    } else {
                        "unavailable"
                    },
                )
            }),
            ..Default::default()
        }]),
        result => result.map_err(|e| match e.params.get("reason").and_then(|v| v.as_str()) {
            Some("limit") => "limit",
            Some("mixed") => "mixed",
            Some("multiple_badges") => "multipleBadges",
            Some("empty") => "empty",
            Some("budget") => "budget",
            _ => "checkFailed",
        }),
    }
}
fn check_native_paths_current(
    paths: Vec<PathBuf>,
    current: impl Fn() -> bool,
) -> Result<Vec<InputItem>, AppError> {
    let checked = targets::check_cancellable(targets::Request::Inspect(paths), current)?;
    if let Some(mask) = checked.mask {
        return Ok(vec![InputItem {
            kind: "badge",
            mask: Some(mask),
            ..Default::default()
        }]);
    }
    let mut items: Vec<_> = checked
        .items
        .into_iter()
        .map(|t| InputItem {
            path: t.path,
            actual_path: t.actual_path,
            linked: t.linked,
            identity: Some(t.identity),
            aliases: t.aliases,
            kind: "directory",
            ..Default::default()
        })
        .collect();
    items.extend(checked.issues.into_iter().map(|i| InputItem {
        path: i.path,
        kind: "invalid",
        error: Some(i.error),
        ..Default::default()
    }));
    Ok(items)
}
fn accept(s: &mut Snapshot, source: &str, items: Vec<InputItem>) {
    if s.operation_locked() || s.choosing || s.preparing {
        s.error = Some("busy".into());
        return;
    }
    if items.is_empty() {
        s.error = Some("empty".into());
        return;
    }
    if items.iter().any(|i| i.kind == "badge") {
        if items.len() == 1 {
            let item = items.into_iter().next().unwrap();
            if let Some(error) = item.error {
                s.badge_error = Some(error);
            } else if let Some(mask) = item.mask {
                s.badge = Badge {
                    id: targets::next_id("badge"),
                    width: mask.width,
                    height: mask.height,
                    opaque: mask.opaque,
                };
                s.mask = Some(std::sync::Arc::new(mask));
                s.badge_error = None;
                s.error = None;
                edited(s);
            } else {
                s.error = Some("badgeUnavailable".into());
            }
        } else {
            s.error = Some("mixed".into());
        }
        return;
    }
    let (mut valid, invalid): (Vec<_>, Vec<_>) =
        items.into_iter().partition(|i| i.kind == "directory");
    if valid.len() > 100 {
        s.error = Some("limit".into());
        return;
    }
    s.issues = invalid;
    if valid.is_empty() {
        s.error = Some("invalid".into());
        return;
    }
    for item in &mut valid {
        item.id = targets::next_id("target");
    }
    s.items = valid;
    edited(s);
    s.source = source.into();
    s.error = None;
}
pub fn show<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
pub fn wake<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let hidden = app
        .get_webview_window("main")
        .is_some_and(|w| !w.is_visible().unwrap_or(true));
    let (automatic, expected_request) = {
        let state = app.state::<Inbox>();
        let mut state = state.0.lock().unwrap();
        let plan = plan_wake(&mut state, hidden);
        let expected_request = state.request;
        if plan.1 {
            commit_update(app, state);
        } else {
            drop(state);
        }
        (plan.0, expected_request)
    };
    let finder = crate::macos::finder_is_frontmost();
    show(app);
    if automatic {
        if finder {
            read_finder(app.clone(), true, Some(expected_request));
        } else {
            update(app, |s| {
                clear_automatic(s, expected_request);
            });
        }
    }
}
#[tauri::command]
pub fn get_entry<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Snapshot {
    app.state::<Inbox>().0.lock().unwrap().clone()
}
#[tauri::command]
pub fn remove_target<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    expected_revision: u64,
    target_id: String,
) {
    let inbox = app.state::<Inbox>();
    let mut state = inbox.0.lock().unwrap();
    let index = state
        .items
        .iter()
        .position(|i| i.id == target_id)
        .unwrap_or(usize::MAX);
    remove(&mut state, expected_revision, index);
    commit_update(&app, state);
}
fn remove(s: &mut Snapshot, expected_revision: u64, index: usize) {
    if s.revision != expected_revision || index >= s.items.len() {
        s.error = Some("changed".into());
        return;
    }
    if s.operation_locked()
        || s.choosing
        || s.checking
        || s.importing
        || s.active_read.is_some()
        || s.preparing
    {
        s.error = Some("busy".into());
        return;
    }
    s.request += 1;
    invalidate_read(s);
    s.items.remove(index);
    edited(s);
    s.issues.clear();
    s.source = if s.items.is_empty() { "" } else { "manual" }.into();
    s.error = None;
}
#[tauri::command]
pub fn choose_targets<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri_plugin_dialog::DialogExt;
    {
        let state = app.state::<Inbox>();
        let mut s = state.0.lock().unwrap();
        if s.operation_locked()
            || s.choosing
            || s.active_read.is_some()
            || s.checking
            || s.preparing
        {
            return;
        }
        s.choosing = true;
        commit_update(&app, s);
    }
    app.dialog().file().pick_folders(move |files| {
        if let Some(files) = files {
            let paths = files
                .into_iter()
                .filter_map(|f| f.into_path().ok())
                .collect();
            receive_paths(&app, "picker", paths, true);
        } else {
            update(&app, |s| s.choosing = false);
        }
    });
}
#[tauri::command]
pub fn choose_badge<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri_plugin_dialog::DialogExt;
    {
        let inbox = app.state::<Inbox>();
        let mut s = inbox.0.lock().unwrap();
        if s.operation_locked() || candidate_pending(&s) {
            return;
        }
        s.choosing = true;
        s.importing = true;
        commit_update(&app, s);
    }
    app.dialog()
        .file()
        .add_filter("PNG / SVG", &["png", "svg"])
        .pick_file(move |file| {
            if let Some(file) = file {
                match file.into_path() {
                    Ok(path) => receive_paths(&app, "badge-picker", vec![path], true),
                    Err(_) => update(&app, |s| {
                        s.choosing = false;
                        s.importing = false;
                        s.badge_error = Some(AppError::reason("asset_error", "unavailable"));
                    }),
                }
            } else {
                update(&app, |s| {
                    s.choosing = false;
                    s.importing = false;
                });
            }
        });
}
#[tauri::command]
pub fn get_badge_mask<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    badge_id: String,
) -> Result<String, AppError> {
    let inbox = app.state::<Inbox>();
    let s = inbox.0.lock().unwrap();
    if s.badge.id != badge_id {
        return Err(AppError::reason("invalid_input", "asset"));
    }
    s.mask
        .as_ref()
        .map(|m| m.png.clone())
        .or_else(|| {
            crate::builtins::builtin(&badge_id)
                .and_then(|bytes| crate::assets::svg_mask(bytes).ok())
                .map(|m| m.png)
        })
        .ok_or_else(|| AppError::reason("invalid_input", "asset"))
}
#[tauri::command]
pub fn select_badge<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    expected_edit_revision: u64,
    operation_id: Option<String>,
    badge_id: String,
) -> Result<(), AppError> {
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    let builtin = crate::builtins::builtin(&badge_id);
    if builtin.is_none() && badge_id != s.badge.id {
        return Err(AppError::reason("invalid_input", "asset"));
    }
    let mask = builtin.map(crate::assets::svg_mask).transpose()?;
    accept_edit(&mut s, expected_edit_revision, operation_id.as_deref())?;
    if let Some(mask) = mask {
        s.badge = Badge {
            id: badge_id,
            width: mask.width,
            height: mask.height,
            opaque: false,
        };
        s.mask = Some(std::sync::Arc::new(mask));
    }
    s.badge_error = None;
    commit_update(&app, s);
    Ok(())
}
#[tauri::command]
pub fn finder_selection<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    read_finder(app, false, None);
}
fn read_finder<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    automatic: bool,
    expected_request: Option<u64>,
) {
    let (revision, generation) = {
        let state = app.state::<Inbox>();
        let mut state = state.0.lock().unwrap();
        if state.operation_locked()
            || state.active_read.is_some()
            || state.choosing
            || state.checking
            || state.preparing
            || expected_request.is_some_and(|expected| expected != state.request)
        {
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
        let result = match finder_paths(|| read_is_current(&app, generation)) {
            Ok(paths) if read_is_current(&app, generation) => {
                check_paths_current(paths, || read_is_current(&app, generation))
            }
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
fn read_is_current<R: tauri::Runtime>(app: &tauri::AppHandle<R>, generation: u64) -> bool {
    let state = app.state::<Inbox>();
    let s = state.0.lock().unwrap();
    s.active_read == Some(generation)
}
fn clear_automatic(s: &mut Snapshot, expected_request: u64) {
    if s.operation_locked()
        || s.operation.is_some()
        || s.request != expected_request
        || s.preparing
        || s.choosing
        || s.checking
        || s.active_read.is_some()
        || !(s.items.is_empty() || s.source == "finder-auto")
    {
        return;
    }
    s.request += 1;
    s.items.clear();
    edited(s);
    s.issues.clear();
    s.source.clear();
    s.error = Some("finderInactive".into());
}
fn plan_wake(s: &mut Snapshot, hidden: bool) -> (bool, bool) {
    let automatic = !s.operation_locked()
        && s.operation.is_none()
        && !s.preparing
        && !s.choosing
        && !s.checking
        && hidden
        && (s.items.is_empty() || s.source == "finder-auto");
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
    if automatic
        && s.operation.is_none()
        && !matches!(
            &result,
            Err("limit" | "mixed" | "budget" | "multipleBadges")
        )
        && !result
            .as_ref()
            .is_ok_and(|items| items.iter().any(|i| i.kind == "badge"))
    {
        s.items.clear();
        edited(s);
        s.issues.clear();
        s.source.clear();
    }
    match result {
        Ok(items) => accept(s, if automatic { "finder-auto" } else { "finder" }, items),
        Err(code) => s.error = Some(code.into()),
    }
}
fn finder_paths(current: impl Fn() -> bool) -> Result<Vec<PathBuf>, &'static str> {
    let script = r#"try { var f = Application('com.apple.finder'); JSON.stringify({paths: f.selection().map(x => ObjC.unwrap($.NSURL.URLWithString(x.url()).path))}); } catch(e) { JSON.stringify({denied: e.errorNumber === -1743}); }"#;
    let mut command = std::process::Command::new("/usr/bin/osascript");
    command.args([
        "-l",
        "JavaScript",
        "-e",
        "ObjC.import('Foundation');",
        "-e",
        script,
    ]);
    let bytes = finder_output(command, current)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "finderFailed")?;
    if value["denied"] == true {
        return Err("finderDenied");
    }
    serde_json::from_value(value["paths"].clone()).map_err(|_| "finderFailed")
}
fn finder_output(
    mut command: std::process::Command,
    current: impl Fn() -> bool,
) -> Result<Vec<u8>, &'static str> {
    use std::{
        io::Read,
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut child = command
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
            Ok(None) if current() && start.elapsed() < Duration::from_secs(10) => {
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
    Ok(bytes)
}

fn begin_prepare(
    s: &mut Snapshot,
    input: &operation::Request,
) -> Result<Vec<targets::Target>, AppError> {
    if s.operation_locked() || s.preparing || s.choosing || s.checking || s.active_read.is_some() {
        return Err(AppError::reason("busy", "busy"));
    }
    if s.operation.is_some() {
        return Err(crate::icon::error("edit"));
    }
    if s.revision != input.revision {
        return Err(AppError::reason("target_changed", "changed"));
    }
    if let operation::Action::Apply { badge_id, .. } = &input.action
        && (s.badge_error.is_some() || *badge_id != s.badge.id)
    {
        return Err(AppError::reason("invalid_input", "asset"));
    }
    let selected = operation::resolve(
        &input.target_ids,
        &s.items
            .iter()
            .filter_map(|i| {
                i.identity.clone().map(|identity| targets::Target {
                    id: i.id.clone(),
                    path: i.path.clone(),
                    actual_path: i.actual_path.clone(),
                    linked: i.linked,
                    identity,
                    aliases: i.aliases.clone(),
                })
            })
            .collect::<Vec<_>>(),
    )?;
    s.preparing = true;
    s.request += 1;
    Ok(selected)
}
fn finish_prepare(
    s: &mut Snapshot,
    token: u64,
    result: Result<operation::Prepared, AppError>,
) -> Result<operation::Summary, AppError> {
    if s.request != token {
        return Err(AppError::reason("target_changed", "changed"));
    }
    s.preparing = false;
    result.map(|prepared| {
        let summary = prepared.summary();
        s.prepared = Some(std::sync::Arc::new(prepared));
        summary
    })
}
#[tauri::command]
pub async fn prepare_operation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    request: String,
) -> Result<operation::Summary, AppError> {
    let input = operation::parse(&request)?;
    let (selected, token, mask) = {
        let inbox = app.state::<Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let selected = begin_prepare(&mut s, &input)?;
        let token = s.request;
        let mask = s.mask.clone();
        commit_update(&app, s);
        (selected, token, mask)
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let images = operation::validate(&input)?;
        let mut prepared = operation::Prepared::build(input, selected, images)?;
        prepared.mask = mask;
        Ok(prepared)
    })
    .await
    .unwrap_or_else(|_| Err(AppError::reason("invalid_input", "check")));
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    let result = finish_prepare(&mut s, token, result);
    commit_update(&app, s);
    result
}
#[tauri::command]
pub fn cancel_prepared<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    operation_id: String,
) -> Result<(), AppError> {
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    if s.preparing || s.operation.as_ref().is_some_and(|p| p.busy()) {
        return Err(AppError::reason("busy", "busy"));
    }
    if s.prepared.as_ref().is_none_or(|p| p.id != operation_id) {
        return Err(AppError::reason("target_changed", "operation"));
    }
    s.prepared = None;
    commit_update(&app, s);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationState {
    revision: u64,
    prepared: Option<operation::Summary>,
    progress: Option<operation::Progress>,
}
#[tauri::command]
pub fn get_operation<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> OperationState {
    let inbox = app.state::<Inbox>();
    let s = inbox.0.lock().unwrap();
    operation_state(&s)
}
fn operation_state(s: &Snapshot) -> OperationState {
    OperationState {
        revision: s.revision,
        prepared: if s.operation.as_ref().is_some_and(|p| p.busy()) {
            None
        } else {
            s.prepared.as_ref().map(|p| p.summary())
        },
        progress: s.operation.clone(),
    }
}
pub fn session<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> (Snapshot, OperationState) {
    let inbox = app.state::<Inbox>();
    let s = inbox.0.lock().unwrap();
    (s.clone(), operation_state(&s))
}
fn accept_edit(
    s: &mut Snapshot,
    expected_edit_revision: u64,
    operation_id: Option<&str>,
) -> Result<(), AppError> {
    if s.operation_locked() || candidate_pending(s) {
        return Err(AppError::reason("busy", "busy"));
    }
    if s.edit_revision != expected_edit_revision
        || s.operation.as_ref().map(|p| p.operation_id.as_str()) != operation_id
    {
        return Err(AppError::reason("target_changed", "changed"));
    }
    s.request += 1;
    edited(s);
    s.error = None;
    Ok(())
}
#[tauri::command]
pub fn begin_edit<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    expected_edit_revision: u64,
    operation_id: Option<String>,
) -> Result<(), AppError> {
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    accept_edit(&mut s, expected_edit_revision, operation_id.as_deref())?;
    commit_update(&app, s);
    Ok(())
}
#[tauri::command]
pub fn set_pinned<R: tauri::Runtime>(app: tauri::AppHandle<R>, pinned: bool) {
    update(&app, |s| s.pinned = pinned);
}
fn claim_commit(
    s: &mut Snapshot,
    operation_id: &str,
    token: Option<&str>,
) -> Result<Option<std::sync::Arc<operation::Prepared>>, AppError> {
    if let Some(progress) = &s.operation {
        if progress.operation_id == operation_id {
            return Ok(None);
        }
        if progress.busy() {
            return Err(crate::icon::error("busy"));
        }
    }
    let p = s
        .prepared
        .as_ref()
        .filter(|p| p.id == operation_id)
        .ok_or_else(|| crate::icon::error("operation"))?;
    if p.needs_confirmation() && token != Some(p.token.as_str()) {
        return Err(crate::icon::error("confirmation"));
    }
    s.operation = Some(operation::Progress::new(p));
    Ok(Some(p.clone()))
}
#[tauri::command]
pub fn commit_operation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    operation_id: String,
    confirmation_token: Option<String>,
) -> Result<(), AppError> {
    let p = {
        let inbox = app.state::<Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let p = claim_commit(&mut s, &operation_id, confirmation_token.as_deref())?;
        commit_update(&app, s);
        p
    };
    if let Some(p) = p {
        tauri::async_runtime::spawn_blocking(move || execute_operation(app, p));
    }
    Ok(())
}
fn progress<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    f: impl FnOnce(&mut operation::Progress),
) {
    update(app, |s| {
        if let Some(p) = s.operation.as_mut() {
            f(p);
            p.sequence += 1;
        }
    });
}
fn execute_operation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    p: std::sync::Arc<operation::Prepared>,
) {
    use std::sync::mpsc;
    // Recheck the entire batch before the first write, so stale consent never partially starts it.
    match p.refreshed() {
        Ok(mut current) if current.fingerprints != p.fingerprints => {
            current.force_confirmation = true;
            update(&app, |s| {
                s.prepared = Some(std::sync::Arc::new(current));
                s.operation = None;
            });
            finish_quit(&app);
            return;
        }
        Err(e) => {
            update(&app, |s| {
                s.prepared = None;
                let v = s.operation.as_mut().unwrap();
                v.phase = "completed".into();
                v.sequence += 1;
                for item in &mut v.items {
                    item.status = "failed".into();
                    item.error = Some(e.clone());
                }
            });
            finish_quit(&app);
            return;
        }
        _ => {}
    }
    for index in 0..p.targets.len() {
        {
            let inbox = app.state::<Inbox>();
            let mut s = inbox.0.lock().unwrap();
            let quit = s.quit_pending;
            let v = s.operation.as_mut().unwrap();
            if quit || v.stopped {
                break;
            }
            v.phase = "running".into();
            v.items[index].status = "running".into();
            v.slow = false;
            v.sequence += 1;
            commit_update(&app, s);
        }
        let (tx, rx) = mpsc::channel();
        let writing = p.clone();
        std::thread::spawn(move || {
            let result = crate::icon::write(
                &writing.targets[index],
                &writing.fingerprints[index],
                writing.icon.as_deref(),
            );
            let _ = tx.send(result);
        });
        let result = wait_for_write(
            rx,
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(20),
            |unknown| {
                progress(&app, |v| {
                    v.slow = true;
                    if unknown {
                        v.items[index].status = "unknown".into();
                        v.stopped = true;
                    }
                });
            },
        );
        // A missing result means the writer died and may still have written; fail closed.
        let uncertain = result.as_ref().is_none_or(|r| {
            r.as_ref()
                .is_err_and(|e| e.params.get("reason").is_some_and(|r| r == "uncertain"))
        });
        progress(&app, |v| {
            v.slow = false;
            let item = &mut v.items[index];
            if uncertain {
                v.phase = "uncertain".into();
                item.status = "unknown".into();
                item.error = Some(crate::icon::error("uncertain"));
                v.stopped = true;
            } else {
                match result.unwrap() {
                    Ok(()) => item.status = "succeeded".into(),
                    Err(e) => {
                        item.status = "failed".into();
                        item.error = Some(e);
                    }
                }
            }
        });
        // A returned uncertain result keeps writes blocked, but no longer delays normal exit.
        if uncertain {
            finish_quit(&app);
            return;
        }
    }
    update(&app, |s| {
        s.prepared = None;
        let v = s.operation.as_mut().unwrap();
        v.phase = "completed".into();
        v.sequence += 1;
    });
    finish_quit(&app);
}
fn wait_for_write<T>(
    rx: std::sync::mpsc::Receiver<T>,
    slow: std::time::Duration,
    uncertain: std::time::Duration,
    mut feedback: impl FnMut(bool),
) -> Option<T> {
    use std::sync::mpsc::RecvTimeoutError;
    match rx.recv_timeout(slow) {
        Ok(r) => Some(r),
        Err(RecvTimeoutError::Timeout) => {
            feedback(false);
            match rx.recv_timeout(uncertain) {
                Ok(r) => Some(r),
                Err(RecvTimeoutError::Timeout) => {
                    feedback(true);
                    // Never cancel a system write already in progress; wait for its definite result.
                    rx.recv().ok()
                }
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}
pub fn request_quit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    if s.operation
        .as_ref()
        .is_some_and(|p| p.phase == "checking" || p.phase == "running")
    {
        s.quit_pending = true;
        if let Some(p) = &mut s.operation {
            p.stopped = true;
            p.sequence += 1;
        }
        commit_update(app, s);
        show(app);
        true
    } else {
        false
    }
}
fn finish_quit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if app.state::<Inbox>().0.lock().unwrap().quit_pending {
        app.exit(0);
    }
}
pub fn can_hide<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let inbox = app.state::<Inbox>();
    let s = inbox.0.lock().unwrap();
    !s.operation_locked() && !s.preparing && !s.choosing && !s.importing
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
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }
    fn completed_operation() -> crate::operation::Progress {
        let request = crate::operation::parse(
            r#"{"revision":7,"targetIds":["x"],"action":{"type":"restore"}}"#,
        )
        .unwrap();
        let mut progress =
            crate::operation::Progress::new(&crate::operation::Prepared::new(request, vec![]));
        progress.phase = "completed".into();
        progress
    }
    fn successful_state() -> super::Snapshot {
        let mut s = targets();
        let mut progress = completed_operation();
        progress.items = s
            .items
            .iter()
            .map(|i| crate::operation::ItemResult {
                target_id: i.id.clone(),
                status: "succeeded".into(),
                error: None,
            })
            .collect();
        s.cleanup = Some((progress.operation_id.clone(), s.edit_revision));
        s.operation = Some(progress);
        s
    }
    #[test]
    fn successful_release_waits_for_candidates_and_preserves_pin() {
        for pending in 0..4 {
            let mut s = successful_state();
            s.pinned = true;
            let token = s.cleanup.clone().unwrap();
            s.choosing = pending == 0;
            s.checking = pending == 1;
            s.active_read = (pending == 2).then_some(1);
            s.preparing = pending == 3;
            assert!(!super::release_success(&mut s, &token));
            assert_eq!(s.items.len(), 3);
            s.choosing = false;
            s.checking = false;
            s.active_read = None;
            s.preparing = false;
            assert!(super::release_success(&mut s, &token));
            assert!(s.items.is_empty());
            assert!(s.operation.is_none());
            assert!(s.cleanup.is_none());
            assert!(s.pinned);
            assert_eq!(s.edit_revision, token.1 + 1);
        }
    }
    #[test]
    fn successful_timer_cannot_clear_new_edits_or_another_operation() {
        for edit in 0..3 {
            let mut s = successful_state();
            let token = s.cleanup.clone().unwrap();
            match edit {
                0 => super::accept(&mut s, "service", targets().items),
                1 => super::remove(&mut s, 7, 0),
                _ => super::accept_edit(&mut s, token.1, Some(&token.0)).unwrap(),
            }
            assert!(!super::release_success(&mut s, &token));
            assert!(!s.items.is_empty());
            assert!(s.operation.is_none());
        }
        let mut s = successful_state();
        let token = s.cleanup.clone().unwrap();
        s.operation.as_mut().unwrap().operation_id = "new-operation".into();
        assert!(!super::release_success(&mut s, &token));
    }
    #[test]
    fn queued_success_callback_rechecks_input_before_hiding() {
        use tauri::Manager;
        let state = successful_state();
        let token = state.cleanup.clone().unwrap();
        let app = tauri::test::mock_builder()
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        // Capture the callback first, then let input run before the main queue executes it.
        let queued = || super::finish_success(app.handle(), &token);
        {
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            s.checking = true;
        }
        assert!(queued());
        {
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            let request = s.request;
            super::finish_input(&mut s, request, "service", Ok(targets().items));
        }
        assert!(!queued());
        let snapshot = super::session(app.handle()).0;
        assert_eq!(snapshot.items.len(), 3);
        assert_eq!(snapshot.source, "service");
        assert!(snapshot.cleanup.is_none());
        assert!(snapshot.operation.is_none());
    }
    #[test]
    fn invalid_input_keeps_success_context_and_release_eligibility() {
        let mut s = successful_state();
        let token = s.cleanup.clone().unwrap();
        super::accept(&mut s, "service", vec![]);
        assert_eq!(s.cleanup.as_ref(), Some(&token));
        assert_eq!(s.items.len(), 3);
        assert!(super::release_success(&mut s, &token));
    }
    #[test]
    fn explicit_edit_rejects_stale_busy_and_uncertain_contexts() {
        for case in 0..6 {
            let mut s = successful_state();
            let token = s.cleanup.clone().unwrap();
            match case {
                0 => s.edit_revision += 1,
                1 => s.operation.as_mut().unwrap().operation_id = "different".into(),
                2 => s.checking = true,
                3 => s.choosing = true,
                4 => s.operation.as_mut().unwrap().phase = "running".into(),
                _ => s.operation.as_mut().unwrap().phase = "uncertain".into(),
            }
            assert!(super::accept_edit(&mut s, token.1, Some(&token.0)).is_err());
            assert_eq!(s.items.len(), 3);
            assert!(s.operation.is_some());
        }
    }
    #[test]
    fn late_input_and_prepare_cannot_clear_a_new_request_busy_state() {
        let mut s = targets();
        s.request = 2;
        s.checking = true;
        s.preparing = true;
        super::finish_input(&mut s, 1, "service", Ok(vec![]));
        assert!(s.checking);
        assert!(super::finish_prepare(&mut s, 1, Err("invalid".into())).is_err());
        assert!(s.preparing);
    }
    #[test]
    fn invalidated_finder_process_is_killed_and_reaped() {
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("5");
        let start = std::time::Instant::now();
        assert_eq!(
            super::finder_output(command, || false),
            Err("finderTimeout")
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }
    #[test]
    fn commit_claim_is_single_use_and_confirmation_is_bound() {
        let r = crate::operation::parse(
            r#"{"revision":1,"targetIds":["x"],"action":{"type":"restore"}}"#,
        )
        .unwrap();
        let p = std::sync::Arc::new(crate::operation::Prepared::new(r, vec![]));
        let mut s = super::Snapshot {
            prepared: Some(p.clone()),
            ..Default::default()
        };
        assert!(super::claim_commit(&mut s, "forged", None).is_err());
        assert!(super::claim_commit(&mut s, &p.id, None).is_err());
        assert!(super::claim_commit(&mut s, &p.id, Some("forged")).is_err());
        assert!(
            super::claim_commit(&mut s, &p.id, Some(&p.token))
                .unwrap()
                .is_some()
        );
        assert!(
            super::claim_commit(&mut s, &p.id, Some(&p.token))
                .unwrap()
                .is_none()
        );
        assert!(s.operation_locked());
        s.prepared = None;
        assert!(super::claim_commit(&mut s, &p.id, None).unwrap().is_none());
    }
    #[test]
    fn normal_quit_stops_scheduling_and_keeps_the_inflight_slot() {
        let r = crate::operation::parse(
            r#"{"revision":1,"targetIds":["x"],"action":{"type":"restore"}}"#,
        )
        .unwrap();
        let p = std::sync::Arc::new(crate::operation::Prepared::new(r, vec![]));
        let state = super::Snapshot {
            prepared: Some(p.clone()),
            operation: Some(crate::operation::Progress::new(&p)),
            ..Default::default()
        };
        let app = tauri::test::mock_builder()
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        assert!(!super::can_hide(app.handle()));
        assert!(super::request_quit(app.handle()));
        use tauri::Manager;
        let inbox = app.state::<super::Inbox>();
        let mut s = inbox.0.lock().unwrap();
        assert!(s.quit_pending);
        assert!(s.operation.as_ref().unwrap().stopped);
        assert!(s.operation_locked());
        // A timed-out call is still running; a returned uncertain result is not in flight.
        s.operation.as_mut().unwrap().phase = "uncertain".into();
        assert!(s.operation_locked());
        drop(s);
        assert!(!super::request_quit(app.handle()));
    }
    #[test]
    fn slow_feedback_keeps_tracking_the_late_result() {
        use std::{sync::mpsc, time::Duration};
        let (tx, rx) = mpsc::channel();
        let mut feedback = Vec::new();
        let result = super::wait_for_write(
            rx,
            Duration::from_millis(1),
            Duration::from_millis(1),
            |unknown| {
                feedback.push(unknown);
                if unknown {
                    tx.send(42).unwrap();
                }
            },
        );
        assert_eq!(feedback, vec![false, true]);
        assert_eq!(result, Some(42));
        let (tx, rx) = mpsc::channel::<()>();
        drop(tx);
        assert_eq!(
            super::wait_for_write(rx, Duration::ZERO, Duration::ZERO, |_| panic!(
                "disconnected is uncertain immediately"
            )),
            None
        );
    }
    #[test]
    fn operation_claim_rejects_stale_ids_duplicates_and_busy_inputs() {
        let mut s = targets();
        s.items[0].id = "registered".into();
        s.items[0].identity = Some(crate::targets::Identity {
            device: 1,
            inode: 1,
            birth: 1,
            birth_nsec: 0,
        });
        let request = |revision, ids: serde_json::Value| {
            crate::operation::parse(&serde_json::json!({"revision":revision,"targetIds":ids,"action":{"type":"restore"}}).to_string()).unwrap()
        };
        for r in [
            request(6, serde_json::json!(["registered"])),
            request(7, serde_json::json!(["forged"])),
            request(7, serde_json::json!(["registered", "registered"])),
        ] {
            assert!(super::begin_prepare(&mut s, &r).is_err());
            assert!(!s.preparing);
        }
        let r = request(7, serde_json::json!(["registered"]));
        assert_eq!(super::begin_prepare(&mut s, &r).unwrap().len(), 1);
        assert!(super::begin_prepare(&mut s, &r).is_err());
        super::remove(&mut s, 7, 0);
        assert_eq!(s.items.len(), 3);
        let old_ids: Vec<_> = s.items.iter().map(|i| i.id.clone()).collect();
        super::accept(&mut s, "service", targets().items);
        assert_eq!(
            s.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            old_ids
        );
        let wire = serde_json::to_value(&s).unwrap();
        assert!(wire.get("prepared").is_none());
        assert!(wire["items"][0].get("identity").is_none());
    }
    #[test]
    fn partial_invalid_inputs_replace_only_after_successful_validation() {
        let mut s = targets();
        let mut input = targets().items;
        input.truncate(1);
        input.push(super::InputItem {
            path: "/missing".into(),
            kind: "invalid",
            error: Some("invalid".into()),
            ..Default::default()
        });
        super::accept(&mut s, "service", input);
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.issues.len(), 1);
        assert!(!s.items[0].id.is_empty());
        let old = s.items[0].id.clone();
        super::accept(
            &mut s,
            "service",
            vec![super::InputItem {
                kind: "invalid",
                ..Default::default()
            }],
        );
        assert_eq!(s.items[0].id, old);
    }

    #[test]
    fn wake_decisions_and_prepare_results_recheck_request_ownership() {
        let mut s = targets();
        let old = s.request;
        s.preparing = true;
        super::clear_automatic(&mut s, old);
        assert_eq!(s.items.len(), 3);
        s.preparing = false;
        s.request += 1;
        super::clear_automatic(&mut s, old);
        assert_eq!(s.items.len(), 3);
        let r = crate::operation::parse(
            r#"{"revision":7,"targetIds":["x"],"action":{"type":"restore"}}"#,
        )
        .unwrap();
        let prepared = crate::operation::Prepared::new(r, vec![]);
        assert!(super::finish_prepare(&mut s, old, Ok(prepared)).is_err());
        assert!(s.prepared.is_none());
    }
    #[test]
    fn automatic_limits_preserve_ids_order_and_prepared_context() {
        let mut s = targets();
        let request = crate::operation::parse(
            r#"{"revision":7,"targetIds":["x"],"action":{"type":"restore"}}"#,
        )
        .unwrap();
        s.prepared = Some(std::sync::Arc::new(crate::operation::Prepared::new(
            request,
            vec![],
        )));
        let operation = s.prepared.as_ref().unwrap().id.clone();
        let before = serde_json::to_value(&s.items).unwrap();
        for reason in ["limit", "mixed", "budget"] {
            super::finish_finder(&mut s, true, Err(reason));
            assert_eq!(serde_json::to_value(&s.items).unwrap(), before);
            assert_eq!(s.prepared.as_ref().unwrap().id, operation);
            assert_eq!(s.source, "finder-auto");
        }
    }
    #[test]
    fn tauri_ipc_prepares_cancels_recovers_and_protects_confirmation() {
        let _serial = crate::icon::NATIVE_TEST_LOCK.lock().unwrap();
        use serde_json::{Value, json};
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let mut temp = Temp(std::env::temp_dir().join(crate::targets::next_id("ipc")));
        std::fs::create_dir(&temp.0).unwrap();
        temp.0 = std::fs::canonicalize(&temp.0).unwrap();
        let mut state = super::Snapshot::default();
        super::accept(
            &mut state,
            "picker",
            super::check_paths(vec![temp.0.clone()]).unwrap(),
        );
        let app = tauri::test::mock_builder()
            .manage(crate::Session {
                version: env!("CARGO_PKG_VERSION"),
                locale: crate::localization::Locale::En,
            })
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .invoke_handler(tauri::generate_handler![
                crate::get_session,
                super::begin_edit,
                super::set_pinned,
                super::get_entry,
                super::prepare_operation,
                super::cancel_prepared,
                super::commit_operation,
                super::get_operation,
                super::remove_target
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let ipc = |cmd: &str, body: Value| {
            tauri::test::get_ipc_response(
                &window,
                tauri::webview::InvokeRequest {
                    cmd: cmd.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.into(),
                },
            )
            .map(|b| b.deserialize::<Value>().unwrap())
        };
        let entry = || ipc("get_entry", json!({})).unwrap();
        let prepare = |action: Value| {
            let e = entry();
            ipc(
                "prepare_operation",
                json!({"request":json!({"revision":e["revision"],"targetIds":[e["items"][0]["id"]],"action":action}).to_string()}),
            )
        };
        let e = entry();
        assert!(e["items"][0].get("identity").is_none());
        assert!(e.get("prepared").is_none());
        assert!(ipc("prepare_operation",json!({"request":json!({"revision":e["revision"],"targetIds":["forged"],"action":{"type":"restore"}}).to_string()})).is_err());
        use base64::Engine as _;
        let images:Vec<_>=[(16,"16x16"),(32,"16x16@2x"),(64,"32x32@2x"),(128,"128x128"),(256,"128x128@2x"),(512,"256x256@2x"),(1024,"512x512@2x")].into_iter().map(|(size,name)|{
            let file=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset/icon_{name}.png"));
            json!({"size":size,"png":base64::engine::general_purpose::STANDARD.encode(std::fs::read(file).unwrap())})
        }).collect();
        let applied=prepare(json!({"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0},"images":images})).unwrap();
        assert_eq!(applied["commitAvailable"], true);
        assert!(entry().get("prepared").is_none());
        ipc(
            "cancel_prepared",
            json!({"operationId":applied["operationId"]}),
        )
        .unwrap();
        let prepared = prepare(json!({"type":"restore"})).unwrap();
        assert_eq!(prepared["commitAvailable"], true);
        ipc(
            "cancel_prepared",
            json!({"operationId":prepared["operationId"]}),
        )
        .unwrap();
        assert!(
            ipc(
                "cancel_prepared",
                json!({"operationId":prepared["operationId"]})
            )
            .is_err()
        );
        assert!(prepare(json!({"type":"apply","templateId":"forged","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0},"images":[]})).is_err());
        assert_eq!(entry()["preparing"], false);
        let prepared = prepare(json!({"type":"restore"})).unwrap();
        let e = entry();
        ipc(
            "remove_target",
            json!({"expectedRevision":e["revision"],"targetId":e["items"][0]["id"]}),
        )
        .unwrap();
        assert_eq!(entry()["items"].as_array().unwrap().len(), 1);
        // Confirmation owns its target set; external inputs and edits cannot replace it.
        super::receive(app.handle(), "service", vec![temp.0.clone()]);
        ipc(
            "cancel_prepared",
            json!({"operationId":prepared["operationId"]}),
        )
        .unwrap();
        let e = entry();
        ipc(
            "remove_target",
            json!({"expectedRevision":e["revision"],"targetId":e["items"][0]["id"]}),
        )
        .unwrap();
        assert!(entry()["items"].as_array().unwrap().is_empty());
        assert!(entry().get("prepared").is_none());
        assert!(entry()["items"][0].get("aliases").is_none());
        let select = || {
            super::receive(app.handle(), "service", vec![temp.0.clone()]);
            for _ in 0..1000 {
                if entry()["checking"] == false {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(entry()["items"].as_array().unwrap().len(), 1);
        };
        let op = || ipc("get_operation", json!({})).unwrap();
        let wait = || {
            for _ in 0..1000 {
                let v = op();
                if v["progress"]["phase"] == "completed" || v["prepared"].is_object() {
                    return v;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("operation did not finish");
        };
        let apply = || {
            prepare(json!({"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0},"images":images})).unwrap()
        };
        select();
        let p = apply();
        assert!(p["confirmationToken"].is_null());
        ipc(
            "commit_operation",
            json!({"operationId":p["operationId"],"confirmationToken":null}),
        )
        .unwrap();
        let done = wait();
        let snapshot = ipc("get_session", json!({})).unwrap();
        assert_eq!(
            snapshot["entry"]["revision"],
            snapshot["operation"]["revision"]
        );
        assert_eq!(snapshot["operation"], done);
        assert!(snapshot["entry"].get("cleanup").is_none());
        assert_eq!(
            done["progress"]["items"][0]["status"], "succeeded",
            "{done}"
        );
        assert!(crate::icon::fingerprint(&temp.0).unwrap().custom);
        assert!(done["progress"].get("icon").is_none());
        assert!(done["progress"].get("fingerprints").is_none());
        ipc(
            "commit_operation",
            json!({"operationId":p["operationId"],"confirmationToken":null}),
        )
        .unwrap();
        assert_eq!(op()["progress"], done["progress"]);
        assert!(prepare(json!({"type":"restore"})).is_err());
        let before_edit = entry();
        super::receive(app.handle(), "service", vec![]);
        for _ in 0..1000 {
            if entry()["checking"] == false {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(entry()["editRevision"], before_edit["editRevision"]);
        assert_eq!(op()["progress"], done["progress"]);
        ipc("begin_edit", json!({"expectedEditRevision": before_edit["editRevision"], "operationId": p["operationId"]})).unwrap();
        assert_eq!(entry()["items"], before_edit["items"]);
        assert!(op()["progress"].is_null());

        select();
        let p = apply();
        assert!(p["confirmationToken"].is_string());
        assert_eq!(p["overwriteTargetIds"].as_array().unwrap().len(), 1);
        let before = crate::icon::fingerprint(&temp.0).unwrap();
        assert!(
            ipc(
                "commit_operation",
                json!({"operationId":p["operationId"],"confirmationToken":"forged"})
            )
            .is_err()
        );
        ipc("cancel_prepared", json!({"operationId":p["operationId"]})).unwrap();
        assert_eq!(crate::icon::fingerprint(&temp.0).unwrap(), before);
        let p = apply();
        let other =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/32x32.png"))
                .unwrap();
        crate::icon::native(&temp.0, Some(&other)).unwrap();
        let changed = crate::icon::fingerprint(&temp.0).unwrap();
        ipc(
            "commit_operation",
            json!({"operationId":p["operationId"],"confirmationToken":p["confirmationToken"]}),
        )
        .unwrap();
        let renewed = wait()["prepared"].clone();
        assert!(renewed.is_object());
        assert_ne!(renewed["confirmationToken"], p["confirmationToken"]);
        assert_eq!(crate::icon::fingerprint(&temp.0).unwrap(), changed);
        assert!(
            ipc(
                "commit_operation",
                json!({"operationId":p["operationId"],"confirmationToken":p["confirmationToken"]})
            )
            .is_err()
        );
        ipc(
            "cancel_prepared",
            json!({"operationId":renewed["operationId"]}),
        )
        .unwrap();
        let p = prepare(json!({"type":"restore"})).unwrap();
        ipc(
            "commit_operation",
            json!({"operationId":p["operationId"],"confirmationToken":p["confirmationToken"]}),
        )
        .unwrap();
        let done = wait();
        assert_eq!(
            done["progress"]["items"][0]["status"], "succeeded",
            "{done}"
        );
        assert!(!crate::icon::fingerprint(&temp.0).unwrap().custom);
        ipc("set_pinned", json!({"pinned":true})).unwrap();
        for _ in 0..1000 {
            if entry()["items"].as_array().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let released = ipc("get_session", json!({})).unwrap();
        assert!(released["entry"]["items"].as_array().unwrap().is_empty());
        assert!(released["operation"]["progress"].is_null());
        assert!(released["operation"]["prepared"].is_null());
        assert_eq!(released["entry"]["pinned"], true);
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
                s.error
                    .as_ref()
                    .and_then(|e| e.params.get("reason").and_then(|v| v.as_str())),
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
    fn imported_mask() -> crate::assets::Mask {
        crate::assets::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/fixtures/badges/geometric-cross.svg"),
        )
        .unwrap()
    }
    #[test]
    fn finder_import_timeout_preserves_manual_and_automatic_context() {
        for automatic in [false, true] {
            for with_result in [false, true] {
                let mut s = targets();
                s.active_read = Some(1);
                s.request = 2;
                s.operation = with_result.then(completed_operation);
                let before = serde_json::to_value(&s.items).unwrap();
                let revision = s.edit_revision;
                let operation = s.operation.as_ref().map(|p| p.operation_id.clone());
                let result = super::finder_check_result(
                    true,
                    Err(crate::localization::AppError::reason("timeout", "check")),
                );
                super::complete_read(&mut s, 1, 2, automatic, result);
                assert_eq!(serde_json::to_value(&s.items).unwrap(), before);
                assert_eq!(s.edit_revision, revision);
                assert_eq!(s.source, "finder-auto");
                assert_eq!(s.badge.id, "code");
                assert_eq!(s.badge_error.as_ref().unwrap().params["reason"], "timeout");
                assert_eq!(
                    s.operation.as_ref().map(|p| p.operation_id.clone()),
                    operation
                );
                assert!(s.active_read.is_none());
            }
        }
    }
    #[test]
    fn imports_preserve_targets_reject_stale_results_and_gate_preparation() {
        let mut s = targets();
        s.operation = Some(completed_operation());
        s.checking = true;
        s.importing = true;
        s.request = 10;
        let before = s.edit_revision;
        super::finish_native_input(
            &mut s,
            9,
            "drop",
            Ok(vec![super::InputItem {
                kind: "badge",
                mask: Some(imported_mask()),
                ..Default::default()
            }]),
        );
        assert!(s.importing);
        assert_eq!(s.badge.id, "code");
        super::finish_native_input(
            &mut s,
            10,
            "drop",
            Ok(vec![super::InputItem {
                kind: "badge",
                mask: Some(imported_mask()),
                ..Default::default()
            }]),
        );
        assert_eq!(s.items.len(), 3);
        assert_eq!(s.source, "finder-auto");
        assert_eq!(s.edit_revision, before + 1);
        assert!(s.operation.is_none());
        assert!(!s.importing);
        let id = s.badge.id.clone();
        let png = s.mask.as_ref().unwrap().png.clone();
        s.operation = Some(completed_operation());
        s.importing = true;
        s.checking = true;
        super::finish_native_input(
            &mut s,
            10,
            "drop",
            Err(crate::localization::AppError::reason(
                "asset_error",
                "empty",
            )),
        );
        assert_eq!(s.badge.id, id);
        assert_eq!(s.mask.as_ref().unwrap().png, png);
        assert_eq!(s.edit_revision, before + 1);
        assert!(s.operation.is_some());
        assert!(s.badge_error.is_some());
        assert!(!s.importing);
        s.operation = None;
        let request = crate::operation::parse(&serde_json::json!({"revision":s.revision,"targetIds":["any"],"action":{"type":"apply","templateId":"big-sur-light-blue","badgeId":id,"params":{"scale":1,"offsetX":0,"offsetY":0},"images":[]}}).to_string()).unwrap();
        assert_eq!(
            super::begin_prepare(&mut s, &request).unwrap_err().params["reason"],
            "asset"
        );
        let snapshot = serde_json::to_value(&s).unwrap();
        assert!(snapshot.get("mask").is_none());
        assert!(snapshot["badge"].get("png").is_none());
    }
    #[test]
    fn badge_commands_restore_current_asset_and_reject_forged_ids() {
        use serde_json::{Value, json};
        use tauri::Manager;
        let mut state = targets();
        super::accept(
            &mut state,
            "drop",
            vec![super::InputItem {
                kind: "badge",
                mask: Some(imported_mask()),
                ..Default::default()
            }],
        );
        state.badge_error = Some(crate::localization::AppError::reason(
            "asset_error",
            "empty",
        ));
        let id = state.badge.id.clone();
        let app = tauri::test::mock_builder()
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .manage(crate::Session {
                version: "test",
                locale: crate::localization::Locale::En,
            })
            .invoke_handler(tauri::generate_handler![
                crate::get_session,
                super::get_badge_mask,
                super::select_badge
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let ipc = |cmd: &str, body: Value| {
            tauri::test::get_ipc_response(
                &window,
                tauri::webview::InvokeRequest {
                    cmd: cmd.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.into(),
                },
            )
            .map(|b| b.deserialize::<Value>().unwrap())
        };
        assert!(ipc("get_badge_mask", json!({"badgeId":"/tmp/arbitrary.svg"})).is_err());
        let png = ipc("get_badge_mask", json!({"badgeId":id})).unwrap();
        assert!(png.as_str().unwrap().starts_with("iVBOR"));
        let snapshot = ipc("get_session", json!({})).unwrap();
        assert_eq!(snapshot["entry"]["badge"]["id"], id);
        assert!(!snapshot.to_string().contains("iVBOR"));
        let revision = snapshot["entry"]["editRevision"].as_u64().unwrap();
        for (version, badge) in [(revision, "forged"), (revision - 1, id.as_str())] {
            assert!(
                ipc(
                    "select_badge",
                    json!({"expectedEditRevision":version,"operationId":null,"badgeId":badge})
                )
                .is_err()
            );
        }
        ipc(
            "select_badge",
            json!({"expectedEditRevision":revision,"operationId":null,"badgeId":id}),
        )
        .unwrap();
        let snapshot = ipc("get_session", json!({})).unwrap();
        assert!(snapshot["entry"]["badgeError"].is_null());
        assert_eq!(snapshot["entry"]["items"].as_array().unwrap().len(), 3);
        ipc(
            "select_badge",
            json!({"expectedEditRevision":revision+1,"operationId":null,"badgeId":"code"}),
        )
        .unwrap();
        assert!(ipc("get_badge_mask", json!({"badgeId":id})).is_err());
        assert!(app.state::<super::Inbox>().0.lock().unwrap().mask.is_some());
        for (badge, _) in crate::builtins::BUILTINS {
            let snapshot = ipc("get_session", json!({})).unwrap();
            let revision = snapshot["entry"]["editRevision"].as_u64().unwrap();
            assert!(
                ipc(
                    "select_badge",
                    json!({"expectedEditRevision":revision-1,"operationId":null,"badgeId":badge})
                )
                .is_err()
            );
            ipc(
                "select_badge",
                json!({"expectedEditRevision":revision,"operationId":null,"badgeId":badge}),
            )
            .unwrap();
            let snapshot = ipc("get_session", json!({})).unwrap();
            assert_eq!(snapshot["entry"]["badge"]["id"], *badge);
            assert_eq!(snapshot["entry"]["editRevision"], revision + 1);
            assert_eq!(snapshot["entry"]["items"].as_array().unwrap().len(), 3);
            assert!(
                ipc("get_badge_mask", json!({"badgeId":badge}))
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .starts_with("iVBOR")
            );
        }
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
                ..Default::default()
            }],
        );
        assert_eq!(
            s.error
                .as_ref()
                .and_then(|e| e.params.get("reason").and_then(|v| v.as_str())),
            Some("badgeUnavailable")
        );
        assert_eq!(s.items.len(), 3);
        assert_eq!(s.source, "finder-auto");
        super::finish_finder(
            &mut s,
            false,
            Ok(vec![super::InputItem {
                path: "/tmp/input.png".into(),
                kind: "badge",
                ..Default::default()
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
            targets().items.into_iter().take(1).collect(),
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
                ..Default::default()
            }],
        );
        assert_eq!(
            s.error
                .as_ref()
                .and_then(|e| e.params.get("reason").and_then(|v| v.as_str())),
            Some("invalid")
        );
        super::accept(
            &mut s,
            "service",
            (0..101)
                .map(|_| super::InputItem {
                    path: std::env::current_dir().unwrap(),
                    kind: "directory",
                    ..Default::default()
                })
                .collect(),
        );
        assert_eq!(
            s.error
                .as_ref()
                .and_then(|e| e.params.get("reason").and_then(|v| v.as_str())),
            Some("limit")
        );
        s.choosing = true;
        super::accept(&mut s, "service", targets().items);
        assert_eq!(
            s.error
                .as_ref()
                .and_then(|e| e.params.get("reason").and_then(|v| v.as_str())),
            Some("busy")
        );
        assert_eq!(s.source, "picker");
    }
    #[test]
    fn failed_reads_clear_only_automatic_targets_without_results() {
        for automatic in [true, false] {
            for context in [true, false] {
                for result in [Ok(vec![]), Err("invalid"), Err("finderDenied")] {
                    let mut s = targets();
                    if context {
                        s.operation = Some(completed_operation());
                    }
                    let clears = automatic && !context;
                    super::finish_finder(&mut s, automatic, result);
                    assert_eq!(s.items.is_empty(), clears);
                    assert_eq!(s.operation.is_some(), context);
                    assert!(s.error.is_some());
                }
            }
        }
    }
    #[test]
    fn automatic_clear_preserves_a_completed_result() {
        let mut s = targets();
        s.operation = Some(completed_operation());
        let request = s.request;
        super::clear_automatic(&mut s, request);
        assert_eq!(s.items.len(), 3);
        assert!(s.operation.is_some());
        assert_eq!(super::plan_wake(&mut s, true), (false, false));
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
                ..Default::default()
            }]),
        );
        assert!(s.items.is_empty());
        assert_eq!(
            s.error
                .as_ref()
                .and_then(|e| e.params.get("reason").and_then(|v| v.as_str())),
            Some("finderInactive")
        );
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(item).unwrap()["path"],
            path.to_str().unwrap()
        );
    }
}
