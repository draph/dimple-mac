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
    params: operation::Params,
    badge_error: Option<AppError>,
    importing: bool,
    text: Option<String>,
    text_count: Option<usize>,
    text_pending: bool,
    #[serde(skip)]
    text_request: u64,
    #[serde(skip)]
    native_importing: bool,
    #[serde(skip)]
    mask: Option<std::sync::Arc<crate::assets::Mask>>,
    revision: u64,
    edit_revision: u64,
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
    retained: Option<std::sync::Arc<operation::Prepared>>,
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
    text: Option<String>,
}
impl Default for Badge {
    fn default() -> Self {
        Self {
            id: "code".into(),
            width: 384,
            height: 256,
            opaque: false,
            text: None,
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
    if state.preparing || state.operation_locked() {
        crate::settings::set_open(app, false);
    }
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
    s.retained = None;
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
    receive_native(app, source, paths.into_iter().map(Ok).collect());
}
pub fn receive_native<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    source: &str,
    paths: Vec<Result<PathBuf, AppError>>,
) {
    receive_paths(app, source, paths, false);
}
fn receive_paths<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    source: &str,
    paths: Vec<Result<PathBuf, AppError>>,
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
            || (s.checking && !s.native_importing)
            || s.preparing
        {
            drop(s);
            error(&app, "busy");
            return;
        }
        s.choosing = false;
        invalidate_read(&mut s);
        s.checking = true;
        s.native_importing = paths.len() == 1
            && paths[0]
                .as_ref()
                .ok()
                .and_then(|p| p.extension())
                .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("svg"));
        s.importing = s.native_importing || s.text_pending;
        s.request += 1;
        let revision = s.request;
        commit_update(&app, s);
        revision
    };
    tauri::async_runtime::spawn_blocking(move || {
        let checked = check_input_current(paths, || {
            app.state::<Inbox>().0.lock().unwrap().request == revision
        });
        update(&app, |s| finish_native_input(s, revision, &source, checked));
    });
}
fn check_input_current(
    input: Vec<Result<PathBuf, AppError>>,
    current: impl Fn() -> bool,
) -> Result<Vec<InputItem>, AppError> {
    if input.len() > 4096 {
        return Err(AppError::reason("invalid_input", "budget"));
    }
    let mut paths = Vec::new();
    let mut issues = Vec::new();
    for item in input {
        match item {
            Ok(path) => paths.push(path),
            Err(error)
                if error
                    .params
                    .get("reason")
                    .is_some_and(|reason| reason == "budget") =>
            {
                return Err(error);
            }
            Err(error) => issues.push(InputItem {
                kind: "invalid",
                error: Some(error),
                ..Default::default()
            }),
        }
    }
    if paths.is_empty() && !issues.is_empty() {
        return Ok(issues);
    }
    let mut items = match check_native_paths_current(paths, current) {
        Err(error) if !issues.is_empty() && error.code == "asset_error" => {
            return Err(AppError::reason("invalid_input", "mixed"));
        }
        result => result?,
    };
    if !issues.is_empty() && items.iter().any(|item| item.kind == "badge") {
        return Err(AppError::reason("invalid_input", "mixed"));
    }
    items.extend(issues);
    Ok(items)
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
                || (s.native_importing
                    && !matches!(
                        e.params.get("reason").and_then(|v| v.as_str()),
                        Some("mixed" | "multiple_badges" | "limit" | "budget")
                    ));
            s.checking = false;
            s.native_importing = false;
            s.importing = s.text_pending;
            if import_failed {
                cancel_text(s);
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
    s.native_importing = false;
    s.importing = s.text_pending;
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
                cancel_text(s);
                s.badge_error = Some(error);
            } else if let Some(mask) = item.mask {
                cancel_text(s);
                s.badge = Badge {
                    id: targets::next_id("badge"),
                    width: mask.width,
                    height: mask.height,
                    opaque: mask.opaque,
                    text: None,
                };
                s.mask = Some(std::sync::Arc::new(mask));
                s.badge_error = None;
                s.text = None;
                s.text_count = None;
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
    crate::panel::show(app, None);
}
pub fn open_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let inbox = app.state::<Inbox>();
    let s = inbox.0.lock().unwrap();
    // Serialize opening with operation updates so a late request cannot cover progress.
    crate::settings::set_open(app, !s.preparing && !s.operation_locked());
    drop(s);
    show(app);
}
pub fn wake<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    wake_from(app, crate::macos::finder_is_frontmost());
}
pub fn wake_from<R: tauri::Runtime>(app: &tauri::AppHandle<R>, finder: bool) {
    wake_at(app, finder, None);
}
pub fn wake_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    finder: bool,
    anchor: Option<(f64, f64)>,
) {
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
    crate::panel::show(app, anchor);
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
                .enumerate()
                .map(|(index, f)| {
                    f.into_path().map_err(|_| {
                        AppError::reason("invalid_input", "file_url").param("index", index + 1)
                    })
                })
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
                    Ok(path) => receive_paths(&app, "badge-picker", vec![Ok(path)], true),
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
    accept_badge_edit(&mut s, expected_edit_revision, operation_id.as_deref())?;
    if let Some(mask) = mask {
        s.badge = Badge {
            id: badge_id,
            width: mask.width,
            height: mask.height,
            opaque: false,
            text: None,
        };
        s.mask = Some(std::sync::Arc::new(mask));
    }
    s.badge_error = None;
    s.text = s.badge.text.clone();
    s.text_count = s.text.as_deref().and_then(|t| crate::text::count(t).ok());
    commit_update(&app, s);
    Ok(())
}

fn cancel_text(s: &mut Snapshot) {
    if s.text_pending {
        s.text_pending = false;
        s.importing = s.native_importing;
        s.badge_error = Some(AppError::reason("text_error", "render"));
        s.text_request += 1;
        s.request += 1;
    }
}

fn accept_badge_edit(
    s: &mut Snapshot,
    expected: u64,
    operation: Option<&str>,
) -> Result<(), AppError> {
    if s.edit_revision != expected
        || s.operation.as_ref().map(|p| p.operation_id.as_str()) != operation
    {
        return Err(AppError::reason("target_changed", "changed"));
    }
    if s.operation_locked()
        || s.choosing
        || s.checking
        || s.active_read.is_some()
        || s.preparing
        || (s.importing && !s.text_pending)
    {
        return Err(AppError::reason("busy", "busy"));
    }
    cancel_text(s);
    accept_edit(s, expected, operation)
}

fn start_text(
    s: &mut Snapshot,
    text: String,
    expected: u64,
    operation: Option<&str>,
) -> Result<u64, AppError> {
    accept_badge_edit(s, expected, operation)?;
    s.text_count = crate::text::count(&text).ok();
    let validation = crate::text::validate(&text);
    if text.len() <= crate::text::BYTE_LIMIT {
        s.text = Some(text);
    }
    s.badge_error = validation.err();
    s.text_pending = s.badge_error.is_none();
    s.importing = s.text_pending;
    s.text_request += 1;
    Ok(s.text_request)
}

fn finish_text(s: &mut Snapshot, request: u64, result: Result<targets::Checked, AppError>) {
    if s.text_request != request || !s.text_pending {
        return;
    }
    s.importing = s.native_importing;
    s.text_pending = false;
    match result.and_then(|r| {
        r.mask
            .ok_or_else(|| AppError::reason("text_error", "render"))
    }) {
        Ok(mask) => {
            s.badge = Badge {
                id: targets::next_id("text"),
                width: mask.width,
                height: mask.height,
                opaque: false,
                text: s.text.clone(),
            };
            s.mask = Some(std::sync::Arc::new(mask));
            s.badge_error = None;
        }
        Err(error) => {
            s.badge_error = Some(if error.code == "text_error" {
                error
            } else {
                AppError::reason(
                    "text_error",
                    if error.code == "timeout" {
                        "timeout"
                    } else {
                        "render"
                    },
                )
            })
        }
    }
}

#[tauri::command]
pub fn set_text_badge<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    text: String,
    composing: bool,
    expected_edit_revision: u64,
    operation_id: Option<String>,
) -> Result<(), AppError> {
    let request = {
        let inbox = app.state::<Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let request = start_text(
            &mut s,
            text.clone(),
            expected_edit_revision,
            operation_id.as_deref(),
        )?;
        let pending = s.text_pending;
        // Composition is a draft even if its interim contents are not yet valid.
        if composing && text.len() <= crate::text::BYTE_LIMIT {
            s.badge_error = None;
            s.text_pending = true;
            s.importing = true;
        }
        commit_update(&app, s);
        if composing || !pending {
            return Ok(());
        }
        request
    };
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let result = targets::check_cancellable(targets::Request::Text(text), || {
            app.state::<Inbox>().0.lock().unwrap().text_request == request
        });
        update(&app, |s| finish_text(s, request, result));
    });
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
        let Some(claim) = claim_finder_read(&mut state, expected_request) else {
            return;
        };
        commit_update(&app, state);
        claim
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
fn claim_finder_read(s: &mut Snapshot, expected_request: Option<u64>) -> Option<(u64, u64)> {
    if s.operation_locked()
        || s.active_read.is_some()
        || s.choosing
        || s.checking
        || s.preparing
        || expected_request.is_some_and(|expected| expected != s.request)
    {
        return None;
    }
    cancel_text(s);
    s.read_counter += 1;
    s.active_read = Some(s.read_counter);
    s.request += 1;
    Some((s.request, s.read_counter))
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
        || s.importing
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
        && !s.importing
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
    if s.operation_locked()
        || s.preparing
        || s.choosing
        || s.checking
        || s.importing
        || s.active_read.is_some()
    {
        return Err(AppError::reason("busy", "busy"));
    }
    if s.operation.is_some() {
        return Err(crate::icon::error("edit"));
    }
    if s.revision != input.revision {
        return Err(AppError::reason("target_changed", "changed"));
    }
    if let operation::Action::Apply {
        badge_id, params, ..
    } = &input.action
        && (s.badge_error.is_some() || *badge_id != s.badge.id || *params != s.params)
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
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMode {
    RetryFailed,
    ContinuePending,
}
fn recovery_available(s: &Snapshot, mode: RecoveryMode) -> bool {
    !s.operation_locked()
        && !candidate_pending(s)
        && !s.quit_pending
        && s.retained.is_some()
        && s.operation.as_ref().is_some_and(|p| {
            p.phase == "completed"
                && !p.items.iter().any(|i| i.status == "unknown")
                && p.items.iter().any(|i| {
                    i.status
                        == match mode {
                            RecoveryMode::RetryFailed => "failed",
                            RecoveryMode::ContinuePending => "pending",
                        }
                })
        })
}
fn begin_recovery(
    s: &mut Snapshot,
    operation_id: &str,
    expected_revision: u64,
    mode: RecoveryMode,
) -> Result<operation::Prepared, AppError> {
    if s.revision != expected_revision
        || s.operation
            .as_ref()
            .is_none_or(|p| p.operation_id != operation_id)
    {
        return Err(AppError::reason("target_changed", "operation"));
    }
    if !recovery_available(s, mode) {
        return Err(crate::icon::error("busy"));
    }
    let previous = s.operation.as_ref().unwrap().clone();
    let ids: Vec<_> = previous
        .items
        .iter()
        .filter(|i| {
            i.status
                == match mode {
                    RecoveryMode::RetryFailed => "failed",
                    RecoveryMode::ContinuePending => "pending",
                }
        })
        .map(|i| i.target_id.clone())
        .collect();
    let mut p = s.retained.as_ref().unwrap().as_ref().clone();
    let selected: Vec<_> = p
        .targets
        .iter()
        .cloned()
        .zip(p.fingerprints.iter().cloned())
        .filter(|(t, _)| ids.contains(&t.id))
        .collect();
    (p.targets, p.fingerprints) = selected.into_iter().unzip();
    p.id = targets::next_id("operation");
    p.request.target_ids = ids;
    p.previous = Some(previous);
    p.confirmation_ids = Some(vec![]);
    s.preparing = true;
    s.request += 1;
    Ok(p)
}
#[tauri::command]
pub async fn prepare_recovery<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    operation_id: String,
    expected_revision: u64,
    mode: RecoveryMode,
) -> Result<operation::Summary, AppError> {
    let (p, token) = {
        let inbox = app.state::<Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let p = begin_recovery(&mut s, &operation_id, expected_revision, mode)?;
        let token = s.request;
        commit_update(&app, s);
        (p, token)
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut current = p.refreshed()?;
        current.confirmation_ids = Some(current.changed_consent(&p));
        Ok(current)
    })
    .await
    .unwrap_or_else(|_| Err(AppError::reason("invalid_input", "check")));
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    let result = finish_prepare(&mut s, token, result);
    commit_update(&app, s);
    result
}
fn stop(s: &mut Snapshot, operation_id: &str) -> Result<(), AppError> {
    let p = s
        .operation
        .as_mut()
        .filter(|p| p.operation_id == operation_id)
        .ok_or_else(|| AppError::reason("target_changed", "operation"))?;
    if !matches!(p.phase.as_str(), "checking" | "running") {
        return Err(crate::icon::error("busy"));
    }
    p.stopped = true;
    p.sequence += 1;
    Ok(())
}
#[tauri::command]
pub fn stop_operation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    operation_id: String,
) -> Result<(), AppError> {
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    stop(&mut s, &operation_id)?;
    commit_update(&app, s);
    Ok(())
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
    if s.operation.is_none() {
        s.retained = None;
    }
    commit_update(&app, s);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationState {
    revision: u64,
    prepared: Option<operation::Summary>,
    progress: Option<operation::Progress>,
    can_stop: bool,
    can_retry: bool,
    can_continue: bool,
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
        can_stop: s.operation.as_ref().is_some_and(|p| {
            matches!(p.phase.as_str(), "checking" | "running")
                && !p.stopped
                && p.items.iter().any(|i| i.status == "pending")
        }),
        can_retry: recovery_available(s, RecoveryMode::RetryFailed),
        can_continue: recovery_available(s, RecoveryMode::ContinuePending),
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
pub fn set_render_params<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    expected_edit_revision: u64,
    operation_id: Option<String>,
    params: operation::Params,
) -> Result<(), AppError> {
    params.validate()?;
    let inbox = app.state::<Inbox>();
    let mut s = inbox.0.lock().unwrap();
    accept_edit(&mut s, expected_edit_revision, operation_id.as_deref())?;
    s.params = params;
    commit_update(&app, s);
    Ok(())
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
    let mut progress = p
        .previous
        .clone()
        .unwrap_or_else(|| operation::Progress::new(p));
    progress.operation_id = p.id.clone();
    progress.phase = "checking".into();
    progress.sequence += 1;
    progress.slow = false;
    progress.stopped = false;
    for item in &mut progress.items {
        if p.targets.iter().any(|t| t.id == item.target_id) {
            item.status = "pending".into();
            item.error = None;
        }
    }
    if p.previous.is_none() {
        s.retained = Some(p.clone());
    } else {
        let retained = std::sync::Arc::make_mut(s.retained.as_mut().unwrap());
        for (target, fingerprint) in p.targets.iter().zip(&p.fingerprints) {
            let index = retained
                .targets
                .iter()
                .position(|t| t.id == target.id)
                .unwrap();
            if fingerprint.is_ok() {
                retained.fingerprints[index] = fingerprint.clone();
            }
        }
    }
    s.operation = Some(progress);
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
    execute_with_writer(app, p, crate::icon::write, WRITE_WAIT);
}
const WRITE_WAIT: [std::time::Duration; 2] = [
    std::time::Duration::from_secs(10),
    std::time::Duration::from_secs(20),
];
fn execute_with_writer<R: tauri::Runtime, F>(
    app: tauri::AppHandle<R>,
    p: std::sync::Arc<operation::Prepared>,
    write: F,
    waits: [std::time::Duration; 2],
) where
    F: Fn(&targets::Target, &crate::icon::Fingerprint, Option<&[u8]>) -> Result<(), AppError>
        + Clone
        + Send
        + 'static,
{
    use std::sync::mpsc;
    // No item starts before renewed consent for the affected subset is collected.
    let current = match p.refreshed() {
        Ok(current) => current,
        Err(e) => {
            update(&app, |s| {
                s.prepared = None;
                let v = s.operation.as_mut().unwrap();
                v.phase = "completed".into();
                v.sequence += 1;
                for item in &mut v.items {
                    if !v.stopped && p.targets.iter().any(|t| t.id == item.target_id) {
                        item.status = "failed".into();
                        item.error = Some(e.clone());
                    }
                }
            });
            finish_quit(&app);
            return;
        }
    };
    let changed = current.changed_consent(&p);
    if !changed.is_empty() {
        let inbox = app.state::<Inbox>();
        let mut s = inbox.0.lock().unwrap();
        if !s.quit_pending && !s.operation.as_ref().unwrap().stopped {
            let mut current = current;
            current.confirmation_ids = Some(changed);
            s.operation = p.previous.clone();
            s.prepared = Some(std::sync::Arc::new(current));
            commit_update(&app, s);
            finish_quit(&app);
            return;
        }
    }
    let p = std::sync::Arc::new(current);
    for index in 0..p.targets.len() {
        {
            let inbox = app.state::<Inbox>();
            let mut s = inbox.0.lock().unwrap();
            let quit = s.quit_pending;
            let v = s.operation.as_mut().unwrap();
            if quit || v.stopped {
                break;
            }
            let item_index = v
                .items
                .iter()
                .position(|i| i.target_id == p.targets[index].id)
                .unwrap();
            if let Err(e) = &p.fingerprints[index] {
                v.items[item_index].status = "failed".into();
                v.items[item_index].error = Some(e.clone());
                v.sequence += 1;
                commit_update(&app, s);
                continue;
            }
            v.phase = "running".into();
            v.items[item_index].status = "running".into();
            v.slow = false;
            v.sequence += 1;
            commit_update(&app, s);
        }
        let (tx, rx) = mpsc::channel();
        let writing = p.clone();
        let write = write.clone();
        std::thread::spawn(move || {
            let result = write(
                &writing.targets[index],
                writing.fingerprints[index].as_ref().unwrap(),
                writing.icon.as_deref(),
            );
            let _ = tx.send(result);
        });
        let result = wait_for_write(rx, waits[0], waits[1], |unknown| {
            progress(&app, |v| {
                v.slow = true;
                if unknown {
                    let item = v
                        .items
                        .iter_mut()
                        .find(|i| i.target_id == p.targets[index].id)
                        .unwrap();
                    item.status = "unknown".into();
                    v.stopped = true;
                }
            });
        });
        // A missing result means the writer died and may still have written; fail closed.
        let uncertain = result.as_ref().is_none_or(|r| {
            r.as_ref()
                .is_err_and(|e| e.params.get("reason").is_some_and(|r| r == "uncertain"))
        });
        progress(&app, |v| {
            v.slow = false;
            let item = v
                .items
                .iter_mut()
                .find(|i| i.target_id == p.targets[index].id)
                .unwrap();
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
fn allows_hide(s: &Snapshot) -> bool {
    !s.operation_locked() && !candidate_pending(s)
}
pub fn hide_if_allowed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let inbox = app.state::<Inbox>();
    let s = inbox.0.lock().unwrap();
    if allows_hide(&s)
        && let Some(w) = app.get_webview_window("main")
        && w.hide().is_ok()
    {
        let _ = app.emit("panel-visibility", false);
    }
}
#[cfg(test)]
pub fn can_hide<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    allows_hide(&app.state::<Inbox>().0.lock().unwrap())
}

#[cfg(test)]
mod tests {
    #[test]
    fn settings_native_and_ipc_entries_recheck_operations_and_do_not_queue_opening() {
        let app = tauri::test::mock_builder()
            .manage(super::Inbox::default())
            .manage(crate::settings::Settings::default())
            .invoke_handler(tauri::generate_handler![crate::settings::open_settings])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let is_open = || {
            serde_json::to_value(crate::settings::get_shortcut_settings(app.handle().clone()))
                .unwrap()["open"]
                == true
        };
        for ipc in [false, true] {
            let open = || {
                if ipc {
                    tauri::test::get_ipc_response(
                        &window,
                        tauri::webview::InvokeRequest {
                            cmd: "open_settings".into(),
                            callback: tauri::ipc::CallbackFn(0),
                            error: tauri::ipc::CallbackFn(1),
                            url: "tauri://localhost".parse().unwrap(),
                            body: tauri::ipc::InvokeBody::Json(serde_json::json!({})),
                            headers: Default::default(),
                            invoke_key: tauri::test::INVOKE_KEY.into(),
                        },
                    )
                    .unwrap();
                } else {
                    crate::settings::open(app.handle());
                }
            };
            for case in 0..5 {
                super::update(app.handle(), |s| *s = super::Snapshot::default());
                open();
                assert!(is_open());
                super::update(app.handle(), |s| match case {
                    0 => s.preparing = true,
                    1 => {
                        let request = crate::operation::parse(
                            r#"{"revision":0,"targetIds":["x"],"action":{"type":"restore"}}"#,
                        )
                        .unwrap();
                        s.prepared = Some(std::sync::Arc::new(crate::operation::Prepared::new(
                            request,
                            vec![],
                        )));
                    }
                    _ => {
                        let mut progress = completed_operation();
                        progress.phase = ["checking", "running", "uncertain"][case - 2].into();
                        s.operation = Some(progress);
                    }
                });
                assert!(
                    !is_open(),
                    "operation must dismiss settings that opened first"
                );
                open();
                assert!(!is_open(), "late native and IPC requests must be refused");
                super::update(app.handle(), |s| {
                    s.preparing = false;
                    s.prepared = None;
                    s.operation = Some(completed_operation());
                });
                assert!(!is_open(), "completion must not reopen settings");
                open();
                assert!(is_open(), "completed results allow an explicit new request");
            }
        }
    }
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
    fn explicit_hiding_protects_candidates_and_unresolved_writes() {
        assert!(super::allows_hide(&super::Snapshot::default()));
        for candidate in 0..6 {
            let mut s = super::Snapshot::default();
            match candidate {
                0 => s.choosing = true,
                1 => s.checking = true,
                2 => s.importing = true,
                3 => s.active_read = Some(1),
                4 => s.preparing = true,
                _ => {
                    let request = crate::operation::parse(
                        r#"{"revision":0,"targetIds":["x"],"action":{"type":"restore"}}"#,
                    )
                    .unwrap();
                    s.prepared = Some(std::sync::Arc::new(crate::operation::Prepared::new(
                        request,
                        vec![],
                    )));
                }
            }
            assert!(!super::allows_hide(&s));
        }
        for status in ["failed", "pending", "unknown", "succeeded"] {
            let mut s = successful_state();
            s.operation.as_mut().unwrap().items[0].status = status.into();
            assert!(super::allows_hide(&s));
            for phase in ["checking", "running", "uncertain"] {
                s.operation.as_mut().unwrap().phase = phase.into();
                assert!(!super::allows_hide(&s));
            }
        }
        let mut s = super::Snapshot {
            error: Some("checkFailed".into()),
            ..Default::default()
        };
        assert!(super::allows_hide(&s));
        s.error = None;
        s.badge_error = Some("checkFailed".into());
        assert!(super::allows_hide(&s));
    }
    #[test]
    fn render_param_edits_restore_snapshots_and_reject_stale_or_locked_edits() {
        use serde_json::json;
        use tauri::Manager;
        let app = tauri::test::mock_builder()
            .manage(super::Inbox::default())
            .invoke_handler(tauri::generate_handler![
                super::set_render_params,
                super::get_entry
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let set = |revision, params: serde_json::Value| {
            tauri::test::get_ipc_response(
                &window,
                tauri::webview::InvokeRequest {
                    cmd: "set_render_params".into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(
                        json!({"expectedEditRevision":revision,"operationId":null,"params":params}),
                    ),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.into(),
                },
            )
        };
        let params = json!({"scale":2.0,"offsetX":-100.0,"offsetY":100.0,"theme":"#EF4444"});
        assert!(set(0, params.clone()).is_ok());
        let snapshot = super::session(app.handle()).0;
        assert_eq!(serde_json::to_value(&snapshot.params).unwrap(), params);
        assert_eq!(snapshot.edit_revision, 1);
        assert!(set(0, params.clone()).is_err());
        assert!(set(1, json!({"scale":1,"offsetX":0,"offsetY":0,"theme":"#FFF"})).is_err());
        assert_eq!(super::session(app.handle()).0.edit_revision, 1);
        {
            let inbox = app.state::<super::Inbox>();
            inbox.0.lock().unwrap().preparing = true;
        }
        assert!(set(1, params.clone()).is_err());
        {
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            s.preparing = false;
            let input = crate::operation::parse(&json!({"revision":s.revision,"targetIds":["x"],"action":{"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0,"theme":"native"},"images":[]}}).to_string()).unwrap();
            assert!(super::begin_prepare(&mut s, &input).is_err());
            assert_eq!(serde_json::to_value(&s.params).unwrap(), params);
        }
    }
    #[test]
    fn successful_release_waits_for_candidates() {
        for pending in 0..4 {
            let mut s = successful_state();
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
    fn successful_callback_releases_targets_without_hiding_the_panel() {
        use tauri::Listener;
        let state = successful_state();
        let token = state.cleanup.clone().unwrap();
        let app = tauri::test::mock_builder()
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let _window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let visibility_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let received = visibility_events.clone();
        app.listen("panel-visibility", move |event| {
            received.lock().unwrap().push(event.payload().to_owned());
        });
        assert!(!super::finish_success(app.handle(), &token));
        let snapshot = super::session(app.handle()).0;
        assert!(snapshot.items.is_empty());
        assert!(snapshot.operation.is_none());
        assert!(visibility_events.lock().unwrap().is_empty());
    }
    #[test]
    fn queued_success_callback_rechecks_input_before_releasing_targets() {
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
        let mut p = crate::operation::Prepared::new(r, vec![]);
        p.confirmation_ids = Some(vec!["x".into()]);
        let p = std::sync::Arc::new(p);
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
        let (tx, rx) = mpsc::channel();
        tx.send(7).unwrap();
        assert_eq!(
            super::wait_for_write(rx, Duration::ZERO, Duration::ZERO, |_| panic!(
                "an available result needs no slow feedback"
            )),
            Some(7)
        );
        let (tx, rx) = mpsc::channel();
        let mut tx = Some(tx);
        let mut feedback = Vec::new();
        assert_eq!(
            super::wait_for_write(rx, Duration::ZERO, Duration::ZERO, |unknown| {
                feedback.push(unknown);
                tx.take().unwrap().send(9).unwrap();
            }),
            Some(9)
        );
        assert_eq!(feedback, vec![false]);
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
            .manage(crate::language::Language::for_test(
                crate::localization::Locale::En,
            ))
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .invoke_handler(tauri::generate_handler![
                crate::get_session,
                super::begin_edit,
                super::set_render_params,
                super::get_entry,
                super::prepare_operation,
                super::prepare_recovery,
                super::stop_operation,
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
        assert_eq!(
            ipc(
                "prepare_recovery",
                json!({"operationId":"forged","expectedRevision":0,"mode":"retry_failed"})
            )
            .unwrap_err()["code"],
            "target_changed"
        );
        assert!(
            ipc(
                "prepare_recovery",
                json!({"operationId":"forged","expectedRevision":0,"mode":"invalid"})
            )
            .is_err()
        );
        assert_eq!(
            ipc("stop_operation", json!({"operationId":"forged"})).unwrap_err()["code"],
            "target_changed"
        );
        let e = entry();
        assert!(e["items"][0].get("identity").is_none());
        assert!(e.get("prepared").is_none());
        assert!(ipc("prepare_operation",json!({"request":json!({"revision":e["revision"],"targetIds":["forged"],"action":{"type":"restore"}}).to_string()})).is_err());
        use base64::Engine as _;
        let images:Vec<_>=[(16,"16x16"),(32,"16x16@2x"),(64,"32x32@2x"),(128,"128x128"),(256,"128x128@2x"),(512,"256x256@2x"),(1024,"512x512@2x")].into_iter().map(|(size,name)|{
            let file=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../src/folder-templates/GenericFolderIcon.BigSur.iconset/icon_{name}.png"));
            json!({"size":size,"png":base64::engine::general_purpose::STANDARD.encode(std::fs::read(file).unwrap())})
        }).collect();
        let applied=prepare(json!({"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0,"theme":"native"},"images":images})).unwrap();
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
        assert!(prepare(json!({"type":"apply","templateId":"forged","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0,"theme":"native"},"images":[]})).is_err());
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
            prepare(json!({"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0,"theme":"native"},"images":images})).unwrap()
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

        // Every native target source reaches the same registered-ID write path.
        std::fs::write(temp.0.join("body.txt"), b"unchanged body").unwrap();
        for source in [
            "tray-drop",
            "service",
            "window-drop",
            "picker",
            "finder",
            "finder-auto",
        ] {
            super::receive(app.handle(), source, vec![temp.0.clone()]);
            for _ in 0..1000 {
                if entry()["checking"] == false {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(entry()["source"], source);
            let p = apply();
            assert!(p["confirmationToken"].is_null());
            ipc(
                "commit_operation",
                json!({"operationId":p["operationId"],"confirmationToken":null}),
            )
            .unwrap();
            assert_eq!(wait()["progress"]["items"][0]["status"], "succeeded");
            assert!(crate::icon::fingerprint(&temp.0).unwrap().custom);
            let e = entry();
            ipc("begin_edit", json!({"expectedEditRevision": e["editRevision"], "operationId": op()["progress"]["operationId"]})).unwrap();
            let p = prepare(json!({"type":"restore"})).unwrap();
            ipc(
                "commit_operation",
                json!({"operationId":p["operationId"],"confirmationToken":p["confirmationToken"]}),
            )
            .unwrap();
            assert_eq!(wait()["progress"]["items"][0]["status"], "succeeded");
            assert!(!crate::icon::fingerprint(&temp.0).unwrap().custom);
            assert_eq!(
                std::fs::read(temp.0.join("body.txt")).unwrap(),
                b"unchanged body"
            );
        }
    }

    struct BatchDir(std::path::PathBuf);
    impl Drop for BatchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn batch(
        count: usize,
        restore: bool,
    ) -> (
        BatchDir,
        tauri::App<tauri::test::MockRuntime>,
        crate::operation::Prepared,
    ) {
        let root = BatchDir(std::env::temp_dir().join(crate::targets::next_id("batch")));
        std::fs::create_dir(&root.0).unwrap();
        let paths: Vec<_> = (0..count)
            .map(|i| {
                let p = root.0.join(format!("target-{i:03}"));
                std::fs::create_dir(&p).unwrap();
                std::fs::write(p.join("body.txt"), b"unchanged body").unwrap();
                std::fs::canonicalize(p).unwrap()
            })
            .collect();
        let mut state = super::Snapshot::default();
        super::accept(&mut state, "picker", super::check_paths(paths).unwrap());
        let action = if restore {
            serde_json::json!({"type":"restore"})
        } else {
            use base64::Engine;
            let images: Vec<_> = [(16,"16x16"),(32,"16x16@2x"),(64,"32x32@2x"),(128,"128x128"),(256,"128x128@2x"),(512,"256x256@2x"),(1024,"512x512@2x")].into_iter().map(|(size,name)| {
                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../src/folder-templates/GenericFolderIcon.BigSur.iconset/icon_{name}.png"));
                serde_json::json!({"size":size,"png":base64::engine::general_purpose::STANDARD.encode(std::fs::read(path).unwrap())})
            }).collect();
            serde_json::json!({"type":"apply","templateId":"big-sur-light-blue","badgeId":"code","params":{"scale":1,"offsetX":0,"offsetY":0,"theme":"native"},"images":images})
        };
        let input = crate::operation::parse(&serde_json::json!({"revision":state.revision,"targetIds":state.items.iter().map(|i| &i.id).collect::<Vec<_>>(),"action":action}).to_string()).unwrap();
        let selected = super::begin_prepare(&mut state, &input).unwrap();
        let images = crate::operation::validate(&input).unwrap();
        let p = crate::operation::Prepared::build(input, selected, images).unwrap();
        state.preparing = false;
        let app = tauri::test::mock_builder()
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        (root, app, p)
    }
    fn claimed(
        app: &tauri::App<tauri::test::MockRuntime>,
        p: crate::operation::Prepared,
    ) -> std::sync::Arc<crate::operation::Prepared> {
        use tauri::Manager;
        let inbox = app.state::<super::Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let p = std::sync::Arc::new(p);
        s.prepared = Some(p.clone());
        super::claim_commit(&mut s, &p.id, Some(&p.token))
            .unwrap()
            .unwrap()
    }
    fn recovery(
        app: &tauri::App<tauri::test::MockRuntime>,
        mode: super::RecoveryMode,
    ) -> crate::operation::Prepared {
        use tauri::Manager;
        let inbox = app.state::<super::Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let id = s.operation.as_ref().unwrap().operation_id.clone();
        let revision = s.revision;
        let p = super::begin_recovery(&mut s, &id, revision, mode).unwrap();
        let mut current = p.refreshed().unwrap();
        current.confirmation_ids = Some(current.changed_consent(&p));
        s.preparing = false;
        current
    }
    #[test]
    fn batch_failures_retry_only_failed_targets_with_frozen_apply_and_restore() {
        let _serial = crate::icon::NATIVE_TEST_LOCK.lock().unwrap();
        use tauri::Manager;
        for restore in [false, true] {
            let (root, app, p) = batch(3, restore);
            let original_icon = p.icon.clone();
            let second = p.targets[1].path.clone();
            let displaced = root.0.join("displaced");
            if restore {
                let image = std::fs::read(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/32x32.png"),
                )
                .unwrap();
                for target in &p.targets {
                    crate::icon::native(&target.path, Some(&image)).unwrap();
                }
            }
            let mut p = p;
            p.refresh().unwrap();
            std::fs::rename(&second, &displaced).unwrap();
            let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let recorded = calls.clone();
            let writer = move |target: &crate::targets::Target,
                               expected: &crate::icon::Fingerprint,
                               icon: Option<&[u8]>| {
                recorded.lock().unwrap().push(target.id.clone());
                crate::icon::write(target, expected, icon)
            };
            let p = claimed(&app, p);
            let old_id = p.id.clone();
            super::execute_with_writer(
                app.handle().clone(),
                p.clone(),
                writer.clone(),
                super::WRITE_WAIT,
            );
            {
                let inbox = app.state::<super::Inbox>();
                let s = inbox.0.lock().unwrap();
                let progress = s.operation.as_ref().unwrap();
                assert_eq!(
                    progress
                        .items
                        .iter()
                        .map(|i| i.status.as_str())
                        .collect::<Vec<_>>(),
                    ["succeeded", "failed", "succeeded"]
                );
                assert!(super::operation_state(&s).can_retry);
                assert!(!super::operation_state(&s).can_continue);
            }
            // Repeated read failures must not replace the last confirmed fingerprint.
            let unavailable = recovery(&app, super::RecoveryMode::RetryFailed);
            assert!(unavailable.fingerprints[0].is_err());
            let unavailable = claimed(&app, unavailable);
            super::execute_with_writer(
                app.handle().clone(),
                unavailable,
                writer.clone(),
                super::WRITE_WAIT,
            );
            std::fs::rename(&displaced, &second).unwrap();
            let renewed = recovery(&app, super::RecoveryMode::RetryFailed);
            assert!(
                !renewed.needs_confirmation(),
                "unchanged original consent is reusable"
            );
            assert_ne!(renewed.id, old_id);
            assert_eq!(renewed.icon, original_icon);
            assert_eq!(renewed.targets.len(), 1);
            assert!(
                matches!(renewed.request.action, crate::operation::Action::Restore {}) == restore
            );
            let renewed = claimed(&app, renewed);
            super::execute_with_writer(
                app.handle().clone(),
                renewed.clone(),
                writer,
                super::WRITE_WAIT,
            );
            assert_eq!(
                *calls.lock().unwrap(),
                vec![
                    p.targets[0].id.clone(),
                    p.targets[2].id.clone(),
                    p.targets[1].id.clone()
                ]
            );
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            assert!(super::all_succeeded(&s));
            assert!(super::claim_commit(&mut s, &old_id, Some(&p.token)).is_err());
            assert!(
                super::claim_commit(&mut s, &renewed.id, Some(&renewed.token))
                    .unwrap()
                    .is_none()
            );
            for target in &p.targets {
                assert_eq!(
                    std::fs::read(target.path.join("body.txt")).unwrap(),
                    b"unchanged body"
                );
                assert_eq!(
                    crate::icon::fingerprint(&target.path).unwrap().custom,
                    !restore
                );
            }
        }
    }
    #[test]
    fn stopped_batches_continue_without_rewriting_successes_and_cancel_preserves_context() {
        let _serial = crate::icon::NATIVE_TEST_LOCK.lock().unwrap();
        use tauri::Manager;
        for restore in [false, true] {
            let (_root, app, p) = batch(3, restore);
            let p = claimed(&app, p);
            let handle = app.handle().clone();
            let id = p.id.clone();
            super::execute_with_writer(
                app.handle().clone(),
                p.clone(),
                move |target, expected, icon| {
                    let result = crate::icon::write(target, expected, icon);
                    super::stop_operation(handle.clone(), id.clone()).unwrap();
                    result
                },
                super::WRITE_WAIT,
            );
            {
                let inbox = app.state::<super::Inbox>();
                let s = inbox.0.lock().unwrap();
                assert_eq!(
                    s.operation
                        .as_ref()
                        .unwrap()
                        .items
                        .iter()
                        .map(|i| i.status.as_str())
                        .collect::<Vec<_>>(),
                    ["succeeded", "pending", "pending"]
                );
            }
            let renewed = recovery(&app, super::RecoveryMode::ContinuePending);
            assert!(!renewed.needs_confirmation());
            let cancelled_id = renewed.id.clone();
            {
                let inbox = app.state::<super::Inbox>();
                inbox.0.lock().unwrap().prepared = Some(std::sync::Arc::new(renewed));
            }
            super::cancel_prepared(app.handle().clone(), cancelled_id.clone()).unwrap();
            let renewed = recovery(&app, super::RecoveryMode::ContinuePending);
            assert_eq!(renewed.targets.len(), 2);
            let ids: Vec<_> = renewed.targets.iter().map(|t| t.id.clone()).collect();
            let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let recorded = calls.clone();
            let renewed = claimed(&app, renewed);
            super::execute_with_writer(
                app.handle().clone(),
                renewed,
                move |target, expected, icon| {
                    recorded.lock().unwrap().push(target.id.clone());
                    crate::icon::write(target, expected, icon)
                },
                super::WRITE_WAIT,
            );
            assert_eq!(*calls.lock().unwrap(), ids);
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            assert!(super::all_succeeded(&s));
            assert!(super::claim_commit(&mut s, &cancelled_id, None).is_err());
        }
    }
    #[test]
    fn overdue_batches_block_reentry_and_only_explicitly_resume_after_a_late_result() {
        use std::{sync::mpsc, time::Duration};
        use tauri::Manager;
        let _serial = crate::icon::NATIVE_TEST_LOCK.lock().unwrap();
        for restore in [false, true] {
            for outcome in 0..4 {
                let (_root, app, p) = batch(3, restore);
                let p = claimed(&app, p);
                let (release, gate) = mpsc::channel();
                let gate = std::sync::Arc::new(std::sync::Mutex::new(gate));
                let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
                let recorded = calls.clone();
                let handle = app.handle().clone();
                let original = p.clone();
                let worker = std::thread::spawn(move || {
                    super::execute_with_writer(
                        handle,
                        original,
                        move |target, _, icon| {
                            recorded.lock().unwrap().push(target.id.clone());
                            assert_eq!(icon.is_none(), restore);
                            gate.lock()
                                .unwrap()
                                .recv_timeout(Duration::from_secs(5))
                                .unwrap();
                            match outcome {
                                0 => Ok(()),
                                1 => Err(crate::icon::error("write")),
                                2 => Err(crate::icon::error("uncertain")),
                                _ => panic!("simulate a writer dying without a result"),
                            }
                        },
                        [Duration::from_millis(1); 2],
                    );
                });
                let inbox = app.state::<super::Inbox>();
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                loop {
                    let mut s = inbox.0.lock().unwrap();
                    let v = s.operation.as_ref().unwrap();
                    if v.items[0].status == "unknown" {
                        assert_eq!(v.phase, "running");
                        assert!(v.slow && v.stopped);
                        assert_eq!(v.items[1].status, "pending");
                        assert!(!super::operation_state(&s).can_retry);
                        assert!(!super::operation_state(&s).can_continue);
                        assert!(
                            super::claim_commit(&mut s, &p.id, Some(&p.token))
                                .unwrap()
                                .is_none()
                        );
                        for mode in [
                            super::RecoveryMode::RetryFailed,
                            super::RecoveryMode::ContinuePending,
                        ] {
                            let revision = s.revision;
                            assert!(super::begin_recovery(&mut s, &p.id, revision, mode).is_err());
                        }
                        let revision = s.edit_revision;
                        assert!(super::accept_edit(&mut s, revision, Some(&p.id)).is_err());
                        let ids: Vec<_> = s.items.iter().map(|i| i.id.clone()).collect();
                        super::accept(&mut s, "services", targets().items);
                        assert_eq!(
                            s.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
                            ids
                        );
                        assert_eq!(super::plan_wake(&mut s, true), (false, false));
                        break;
                    }
                    drop(s);
                    assert!(
                        std::time::Instant::now() < deadline,
                        "missing overdue feedback"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(!super::can_hide(app.handle()));
                assert_eq!(*calls.lock().unwrap(), vec![p.targets[0].id.clone()]);
                release.send(()).unwrap();
                worker.join().unwrap();
                {
                    let s = inbox.0.lock().unwrap();
                    let v = s.operation.as_ref().unwrap();
                    assert!(!v.slow);
                    assert!(v.stopped);
                    assert_eq!(
                        v.phase,
                        if outcome < 2 {
                            "completed"
                        } else {
                            "uncertain"
                        }
                    );
                    assert_eq!(
                        v.items[0].status,
                        ["succeeded", "failed", "unknown", "unknown"][outcome]
                    );
                    assert_eq!(v.items[1].status, "pending");
                    assert_eq!(v.items[2].status, "pending");
                    assert_eq!(super::operation_state(&s).can_continue, outcome < 2);
                    assert_eq!(super::operation_state(&s).can_retry, outcome == 1);
                    assert_eq!(s.operation_locked(), outcome >= 2);
                }
                if outcome < 2 {
                    let resumed = recovery(&app, super::RecoveryMode::ContinuePending);
                    let ids: Vec<_> = resumed.targets.iter().map(|t| t.id.clone()).collect();
                    assert_eq!(
                        ids,
                        p.targets[1..]
                            .iter()
                            .map(|t| t.id.clone())
                            .collect::<Vec<_>>()
                    );
                    let resumed = claimed(&app, resumed);
                    let recorded = calls.clone();
                    super::execute_with_writer(
                        app.handle().clone(),
                        resumed,
                        move |target, _, icon| {
                            assert_eq!(icon.is_none(), restore);
                            recorded.lock().unwrap().push(target.id.clone());
                            Ok(())
                        },
                        super::WRITE_WAIT,
                    );
                    assert_eq!(
                        *calls.lock().unwrap(),
                        p.targets.iter().map(|t| t.id.clone()).collect::<Vec<_>>()
                    );
                }
                for target in &p.targets {
                    assert_eq!(
                        std::fs::read(target.path.join("body.txt")).unwrap(),
                        b"unchanged body"
                    );
                }
            }
        }
    }
    #[test]
    fn recovery_rechecks_only_changed_consent_and_rejects_stale_busy_or_unknown_results() {
        use tauri::Manager;
        let (_root, app, p) = batch(3, true);
        let p = claimed(&app, p);
        {
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            assert!(
                super::begin_recovery(&mut s, &p.id, 0, super::RecoveryMode::ContinuePending)
                    .is_err()
            );
            assert!(super::stop(&mut s, "forged").is_err());
            super::stop(&mut s, &p.id).unwrap();
            s.prepared = None;
            let progress = s.operation.as_mut().unwrap();
            progress.phase = "completed".into();
            progress.items[0].status = "succeeded".into();
            progress.items[1].status = "failed".into();
            progress.items[2].status = "pending".into();
        }
        let mut renewed = recovery(&app, super::RecoveryMode::RetryFailed);
        let original = renewed.clone();
        renewed.fingerprints[0] = Err(crate::icon::error("fingerprint"));
        assert!(renewed.changed_consent(&original).is_empty());
        renewed.fingerprints = original.fingerprints.clone();
        renewed.fingerprints[0].as_mut().unwrap().custom = true;
        renewed.confirmation_ids = Some(renewed.changed_consent(&original));
        assert!(renewed.needs_confirmation());
        assert_eq!(
            serde_json::to_value(renewed.summary()).unwrap()["targetIds"],
            serde_json::json!([p.targets[1].id])
        );
        let inbox = app.state::<super::Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let original_progress = serde_json::to_value(&s.operation).unwrap();
        s.preparing = true;
        assert!(!super::recovery_available(
            &s,
            super::RecoveryMode::RetryFailed
        ));
        s.preparing = false;
        let revision = s.revision;
        assert!(
            super::begin_recovery(&mut s, "old", revision, super::RecoveryMode::RetryFailed)
                .is_err()
        );
        assert!(
            super::begin_recovery(
                &mut s,
                &p.id,
                revision + 1,
                super::RecoveryMode::RetryFailed
            )
            .is_err()
        );
        assert_eq!(
            serde_json::to_value(&s.operation).unwrap(),
            original_progress
        );
        s.operation.as_mut().unwrap().items[2].status = "unknown".into();
        assert!(!super::recovery_available(
            &s,
            super::RecoveryMode::RetryFailed
        ));
        s.operation.as_mut().unwrap().items[2].status = "pending".into();
        super::edited(&mut s);
        assert!(s.retained.is_none());
        assert!(!super::recovery_available(
            &s,
            super::RecoveryMode::RetryFailed
        ));
    }
    #[test]
    fn changed_icons_during_execution_fail_only_that_item_and_renew_only_its_consent() {
        let _serial = crate::icon::NATIVE_TEST_LOCK.lock().unwrap();
        use tauri::Manager;
        let (_root, app, p) = batch(3, false);
        let p = claimed(&app, p);
        let first_id = p.targets[0].id.clone();
        let changed_path = p.targets[1].path.clone();
        let image =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/32x32.png"))
                .unwrap();
        super::execute_with_writer(
            app.handle().clone(),
            p.clone(),
            move |target, expected, icon| {
                if target.id == first_id {
                    crate::icon::native(&changed_path, Some(&image)).unwrap();
                }
                crate::icon::write(target, expected, icon)
            },
            super::WRITE_WAIT,
        );
        let before = crate::icon::fingerprint(&p.targets[1].path).unwrap();
        {
            let inbox = app.state::<super::Inbox>();
            let s = inbox.0.lock().unwrap();
            let items = &s.operation.as_ref().unwrap().items;
            assert_eq!(
                items.iter().map(|i| i.status.as_str()).collect::<Vec<_>>(),
                ["succeeded", "failed", "succeeded"]
            );
            assert_eq!(
                items[1].error.as_ref().unwrap().params["reason"],
                "confirmation"
            );
        }
        let renewed = recovery(&app, super::RecoveryMode::RetryFailed);
        let summary = serde_json::to_value(renewed.summary()).unwrap();
        assert_eq!(summary["targetIds"], serde_json::json!([p.targets[1].id]));
        assert!(renewed.needs_confirmation());
        let cancelled = renewed.id.clone();
        {
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            s.prepared = Some(std::sync::Arc::new(renewed));
            assert!(super::claim_commit(&mut s, &cancelled, Some("forged")).is_err());
        }
        super::cancel_prepared(app.handle().clone(), cancelled).unwrap();
        assert_eq!(
            crate::icon::fingerprint(&p.targets[1].path).unwrap(),
            before
        );
        let renewed = recovery(&app, super::RecoveryMode::RetryFailed);
        assert!(
            renewed.needs_confirmation(),
            "cancel must not accept changed consent"
        );
        let renewed = claimed(&app, renewed);
        super::execute_operation(app.handle().clone(), renewed);
        assert!(super::all_succeeded(
            &app.state::<super::Inbox>().0.lock().unwrap()
        ));
    }

    #[test]
    fn one_hundred_targets_execute_and_one_hundred_one_are_rejected_without_replacement() {
        let _serial = crate::icon::NATIVE_TEST_LOCK.lock().unwrap();
        use tauri::Manager;
        let (root, app, p) = batch(100, false);
        let p = claimed(&app, p);
        super::execute_operation(app.handle().clone(), p.clone());
        let inbox = app.state::<super::Inbox>();
        let mut s = inbox.0.lock().unwrap();
        assert!(super::all_succeeded(&s));
        assert_eq!(s.operation.as_ref().unwrap().items.len(), 100);
        let extra = root.0.join("extra");
        std::fs::create_dir(&extra).unwrap();
        let mut paths: Vec<_> = p.targets.iter().map(|t| t.path.clone()).collect();
        paths.push(extra);
        assert!(super::check_paths(paths).is_err());
        assert_eq!(s.items.len(), 100);
        assert!(super::all_succeeded(&s));
        let mut ids = p.request.target_ids.clone();
        ids.push("extra".into());
        assert!(crate::operation::resolve(&ids, &p.targets).is_err());
        for target in &p.targets {
            assert_eq!(
                std::fs::read(target.path.join("body.txt")).unwrap(),
                b"unchanged body"
            );
            assert!(crate::icon::fingerprint(&target.path).unwrap().custom);
        }
        let revision = s.edit_revision;
        super::accept_edit(&mut s, revision, Some(&p.id)).unwrap();
        drop(s);
        let request = crate::operation::parse(&serde_json::json!({"revision":1,"targetIds":p.request.target_ids,"action":{"type":"restore"}}).to_string()).unwrap();
        let restored =
            crate::operation::Prepared::build(request, p.targets.clone(), vec![]).unwrap();
        let restored = claimed(&app, restored);
        super::execute_operation(app.handle().clone(), restored);
        assert!(super::all_succeeded(&inbox.0.lock().unwrap()));
        for target in &p.targets {
            assert!(!crate::icon::fingerprint(&target.path).unwrap().custom);
        }
    }

    #[test]
    fn rejected_native_candidates_allow_pending_text_to_finish() {
        use tauri::Manager;
        let root = std::env::temp_dir().join(crate::targets::next_id("pending-text"));
        std::fs::create_dir(&root).unwrap();
        let many: Vec<_> = (0..101)
            .map(|i| {
                let path = root.join(i.to_string());
                std::fs::create_dir(&path).unwrap();
                Ok(path)
            })
            .collect();
        let badge = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/assets/code.svg");
        let app = tauri::test::mock_builder()
            .manage(super::Inbox::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let rejected = [
            vec![Err(crate::localization::AppError::reason(
                "invalid_input",
                "file_url",
            )
            .param("index", 1))],
            vec![Ok(root.join("missing"))],
            vec![Ok(root.clone()), Ok(badge.clone())],
            many,
            vec![Err(crate::localization::AppError::reason(
                "invalid_input",
                "budget",
            ))],
        ];
        for input in rejected {
            let mut s = targets();
            let text_request = super::start_text(&mut s, "中文".into(), 0, None).unwrap();
            let version = s.edit_revision;
            *app.state::<super::Inbox>().0.lock().unwrap() = s;
            super::receive_native(app.handle(), "service", input);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if !super::get_entry(app.handle().clone()).checking {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let inbox = app.state::<super::Inbox>();
            let mut s = inbox.0.lock().unwrap();
            assert!(s.text_pending && s.importing);
            assert!(s.badge_error.is_none());
            assert_eq!(s.edit_revision, version);
            super::finish_text(
                &mut s,
                text_request,
                Ok(crate::targets::Checked {
                    mask: Some(imported_mask()),
                    ..Default::default()
                }),
            );
            assert!(!s.text_pending && !s.importing);
            assert!(s.badge_error.is_none());
            assert_eq!(s.badge.text.as_deref(), Some("中文"));
        }
        let mut s = targets();
        let text_request = super::start_text(&mut s, "AB".into(), 0, None).unwrap();
        s.checking = true;
        s.native_importing = true;
        s.request += 1;
        super::finish_text(
            &mut s,
            text_request,
            Ok(crate::targets::Checked {
                mask: Some(imported_mask()),
                ..Default::default()
            }),
        );
        assert!(s.checking && s.importing);
        let request = s.request;
        super::finish_native_input(
            &mut s,
            request,
            "tray-drop",
            Ok(vec![super::InputItem {
                kind: "badge",
                mask: Some(imported_mask()),
                ..Default::default()
            }]),
        );
        assert!(!s.checking && !s.importing);
        assert!(s.badge.text.is_none());
        let mut s = targets();
        super::start_text(&mut s, "AB".into(), 0, None).unwrap();
        s.checking = true;
        s.request += 1;
        let request = s.request;
        *app.state::<super::Inbox>().0.lock().unwrap() = s;
        super::receive_native(app.handle(), "service", vec![Ok(root.clone())]);
        let s = super::get_entry(app.handle().clone());
        assert_eq!(s.request, request);
        assert!(s.checking && s.text_pending);
        assert_eq!(s.error.unwrap().params["reason"], "busy");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_parse_issues_preserve_valid_targets_and_reject_mixed_badges() {
        let root = std::env::temp_dir().join(crate::targets::next_id("native-input"));
        std::fs::create_dir(&root).unwrap();
        let invalid = || {
            Err(
                crate::localization::AppError::reason("invalid_input", "file_url")
                    .param("index", 2),
            )
        };
        let input = super::check_input_current(vec![Ok(root.clone()), invalid()], || true).unwrap();
        let mut s = targets();
        super::accept(&mut s, "service", input);
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.issues.len(), 1);
        assert_eq!(s.issues[0].error.as_ref().unwrap().params["index"], 2);
        let before = serde_json::to_value(&s.items).unwrap();
        let revision = s.edit_revision;
        let input = super::check_input_current(vec![invalid()], || true).unwrap();
        super::accept(&mut s, "service", input);
        assert_eq!(serde_json::to_value(&s.items).unwrap(), before);
        assert_eq!(s.edit_revision, revision);
        let badge = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/assets/code.svg");
        let error = super::check_input_current(vec![Ok(badge), invalid()], || true)
            .err()
            .unwrap();
        assert_eq!(error.params["reason"], "mixed");
        let error = super::check_input_current((0..4097).map(|_| invalid()).collect(), || true)
            .err()
            .unwrap();
        assert_eq!(error.params["reason"], "budget");
        std::fs::remove_dir_all(root).unwrap();
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
        let request = crate::operation::parse(&serde_json::json!({"revision":s.revision,"targetIds":["any"],"action":{"type":"apply","templateId":"big-sur-light-blue","badgeId":id,"params":{"scale":1,"offsetX":0,"offsetY":0,"theme":"native"},"images":[]}}).to_string()).unwrap();
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
            .manage(crate::language::Language::for_test(
                crate::localization::Locale::En,
            ))
            .invoke_handler(tauri::generate_handler![
                crate::get_session,
                super::get_badge_mask,
                super::select_badge,
                super::set_text_badge
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
        let before = ipc("get_session", json!({})).unwrap();
        let version = before["entry"]["editRevision"].as_u64().unwrap();
        ipc("set_text_badge", json!({"text":"中文", "composing":true, "expectedEditRevision":version, "operationId":null})).unwrap();
        let draft = ipc("get_session", json!({})).unwrap();
        assert_eq!(draft["entry"]["text"], "中文");
        assert_eq!(draft["entry"]["textPending"], true);
        assert!(ipc("set_text_badge", json!({"text":"old", "composing":false, "expectedEditRevision":version, "operationId":null})).is_err());
        ipc("set_text_badge", json!({"text":"中文", "composing":false, "expectedEditRevision":version+1, "operationId":null})).unwrap();
        let start = std::time::Instant::now();
        let ready = loop {
            let snapshot = ipc("get_session", json!({})).unwrap();
            if snapshot["entry"]["textPending"] == false {
                break snapshot;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(ready["entry"]["badgeError"].is_null());
        assert_eq!(ready["entry"]["badge"]["text"], "中文");
        assert!(
            ipc(
                "get_badge_mask",
                json!({"badgeId":ready["entry"]["badge"]["id"]})
            )
            .unwrap()
            .as_str()
            .unwrap()
            .starts_with("iVBOR")
        );
        ipc("set_text_badge", json!({"text":"12345", "composing":false, "expectedEditRevision":version+2, "operationId":null})).unwrap();
        let invalid = ipc("get_session", json!({})).unwrap();
        assert_eq!(invalid["entry"]["badgeError"]["params"]["reason"], "length");
        ipc("select_badge", json!({"badgeId":ready["entry"]["badge"]["id"], "expectedEditRevision":version+3, "operationId":null})).unwrap();
        let restored = ipc("get_session", json!({})).unwrap();
        assert_eq!(restored["entry"]["text"], "中文");
        assert!(restored["entry"]["badgeError"].is_null());
        ipc(
            "select_badge",
            json!({"badgeId":"code", "expectedEditRevision":version+4, "operationId":null}),
        )
        .unwrap();
        assert!(ipc("get_session", json!({})).unwrap()["entry"]["text"].is_null());
    }
    #[test]
    fn text_edits_preserve_assets_reject_stale_results_and_gate_preparation() {
        let mut s = targets();
        let first = super::start_text(&mut s, "中文".into(), 0, None).unwrap();
        assert!(s.text_pending && s.importing);
        let request = crate::operation::parse(&serde_json::json!({"revision":s.revision,"targetIds":[s.items[0].id],"action":{"type":"restore"}}).to_string()).unwrap();
        assert!(super::begin_prepare(&mut s, &request).is_err());
        let revision = s.edit_revision;
        assert!(super::start_text(&mut s, "old".into(), revision - 1, None).is_err());
        assert!(s.text_pending);
        let second = super::start_text(&mut s, "AB".into(), revision, None).unwrap();
        super::finish_text(
            &mut s,
            first,
            Ok(crate::targets::Checked {
                mask: Some(imported_mask()),
                ..Default::default()
            }),
        );
        assert!(s.text_pending);
        assert_eq!(s.badge.id, "code");
        super::finish_text(
            &mut s,
            second,
            Ok(crate::targets::Checked {
                mask: Some(imported_mask()),
                ..Default::default()
            }),
        );
        assert_eq!(s.badge.text.as_deref(), Some("AB"));
        let id = s.badge.id.clone();
        let png = s.mask.as_ref().unwrap().png.clone();
        let revision = s.edit_revision;
        super::start_text(&mut s, "12345".into(), revision, None).unwrap();
        assert_eq!(s.badge_error.as_ref().unwrap().params["reason"], "length");
        assert_eq!(s.badge.id, id);
        assert_eq!(s.mask.as_ref().unwrap().png, png);
        assert_eq!(s.items.len(), 3);
        let snapshot = serde_json::to_value(&s).unwrap();
        assert_eq!(snapshot["text"], "12345");
        assert_eq!(snapshot["textCount"], 5);
        assert_eq!(snapshot["badge"]["text"], "AB");
        assert!(snapshot.get("mask").is_none());
        let revision = s.edit_revision;
        let third = super::start_text(&mut s, "CD".into(), revision, None).unwrap();
        super::cancel_text(&mut s);
        assert!(s.badge_error.is_some());
        super::finish_text(
            &mut s,
            third,
            Ok(crate::targets::Checked {
                mask: Some(imported_mask()),
                ..Default::default()
            }),
        );
        assert_eq!(s.badge.id, id);
        s.checking = true;
        let revision = s.edit_revision;
        assert!(super::start_text(&mut s, "EF".into(), revision, None).is_err());
        assert_eq!(s.text.as_deref(), Some("CD"));
    }
    #[test]
    fn finder_takeover_cancels_text_waiting_on_success_and_failure() {
        for success in [true, false] {
            let mut s = targets();
            let request = super::start_text(&mut s, "中文".into(), 0, None).unwrap();
            assert_eq!(super::plan_wake(&mut s, true), (false, false));
            super::clear_automatic(&mut s, request);
            assert!(s.text_pending);
            let (read_request, generation) = super::claim_finder_read(&mut s, None).unwrap();
            assert!(!s.text_pending && !s.importing);
            assert!(s.badge_error.is_some());
            let result = if success {
                Ok(targets().items)
            } else {
                Err("finderFailed")
            };
            super::complete_read(&mut s, generation, read_request, false, result);
            super::finish_text(
                &mut s,
                request,
                Ok(crate::targets::Checked {
                    mask: Some(imported_mask()),
                    ..Default::default()
                }),
            );
            assert!(!s.text_pending && !s.importing);
            assert!(s.active_read.is_none());
            assert_eq!(s.badge.id, "code");
            let revision = s.edit_revision;
            assert!(super::start_text(&mut s, "AB".into(), revision, None).is_ok());
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
