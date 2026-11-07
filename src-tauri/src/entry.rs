use crate::{localization::AppError, operation, targets};
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
    error: Option<AppError>,
    issues: Vec<InputItem>,
    preparing: bool,
    #[serde(skip)]
    prepared: Option<std::sync::Arc<operation::Prepared>>,
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
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputItem {
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
    drop(state);
    let _ = app.emit("entry-changed", ());
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
    show(app);
    let app = app.clone();
    let source = source.to_owned();
    let revision = {
        let state = app.state::<Inbox>();
        let mut s = state.0.lock().unwrap();
        if s.choosing || s.checking || s.preparing {
            drop(s);
            error(&app, "busy");
            return;
        }
        invalidate_read(&mut s);
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
    let checked = targets::check(targets::Request::Inspect(paths)).map_err(|e| {
        match e.params.get("reason").and_then(|v| v.as_str()) {
            Some("limit") => "limit",
            Some("mixed") => "mixed",
            Some("badge_unavailable") => "badgeUnavailable",
            Some("empty") => "empty",
            Some("budget") => "budget",
            _ => "checkFailed",
        }
    })?;
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
    if s.choosing || s.preparing {
        s.error = Some("busy".into());
        return;
    }
    if items.is_empty() {
        s.error = Some("empty".into());
        return;
    }
    if items.iter().any(|i| i.kind == "badge") {
        s.error = Some(
            if items.iter().any(|i| i.kind == "directory") {
                "mixed"
            } else {
                "badgeUnavailable"
            }
            .into(),
        );
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
    s.prepared = None;
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
    if s.choosing || s.checking || s.active_read.is_some() || s.preparing {
        s.error = Some("busy".into());
        return;
    }
    s.request += 1;
    invalidate_read(s);
    s.items.remove(index);
    s.prepared = None;
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
        if s.choosing || s.active_read.is_some() || s.checking || s.preparing {
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
        if state.active_read.is_some()
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
fn read_is_current<R: tauri::Runtime>(app: &tauri::AppHandle<R>, generation: u64) -> bool {
    let state = app.state::<Inbox>();
    let s = state.0.lock().unwrap();
    s.active_read == Some(generation)
}
fn clear_automatic(s: &mut Snapshot, expected_request: u64) {
    if s.request != expected_request
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
    s.prepared = None;
    s.issues.clear();
    s.source.clear();
    s.error = Some("finderInactive".into());
}
fn plan_wake(s: &mut Snapshot, hidden: bool) -> (bool, bool) {
    let automatic = !s.preparing && hidden && (s.items.is_empty() || s.source == "finder-auto");
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
    if automatic && !matches!(&result, Err("limit" | "mixed" | "budget")) {
        s.items.clear();
        s.prepared = None;
        s.issues.clear();
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

fn begin_prepare(
    s: &mut Snapshot,
    input: &operation::Request,
) -> Result<Vec<targets::Target>, AppError> {
    if s.preparing || s.choosing || s.checking || s.active_read.is_some() {
        return Err(AppError::reason("busy", "busy"));
    }
    if s.revision != input.revision {
        return Err(AppError::reason("target_changed", "changed"));
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
    s.preparing = false;
    if s.request != token {
        return Err(AppError::reason("target_changed", "changed"));
    }
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
    let (selected, token) = {
        let inbox = app.state::<Inbox>();
        let mut s = inbox.0.lock().unwrap();
        let selected = begin_prepare(&mut s, &input)?;
        let token = s.request;
        commit_update(&app, s);
        (selected, token)
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let images = operation::validate(&input)?;
        targets::check(targets::Request::Revalidate(selected.clone()))?;
        Ok::<_, AppError>(operation::Prepared::new(input, selected, images))
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
    if s.preparing {
        return Err(AppError::reason("busy", "busy"));
    }
    if s.prepared.as_ref().is_none_or(|p| p.id != operation_id) {
        return Err(AppError::reason("target_changed", "operation"));
    }
    s.prepared = None;
    commit_update(&app, s);
    Ok(())
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
        let prepared = crate::operation::Prepared::new(r, vec![], vec![]);
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
    fn tauri_ipc_prepares_cancels_recovers_and_invalidates_operations() {
        use serde_json::{Value, json};
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let temp = Temp(std::env::temp_dir().join(crate::targets::next_id("ipc")));
        std::fs::create_dir(&temp.0).unwrap();
        let mut state = super::Snapshot::default();
        super::accept(
            &mut state,
            "picker",
            super::check_paths(vec![temp.0.clone()]).unwrap(),
        );
        let app = tauri::test::mock_builder()
            .manage(super::Inbox(std::sync::Mutex::new(state)))
            .invoke_handler(tauri::generate_handler![
                super::get_entry,
                super::prepare_operation,
                super::cancel_prepared,
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
        assert_eq!(applied["commitAvailable"], false);
        assert!(entry().get("prepared").is_none());
        ipc(
            "cancel_prepared",
            json!({"operationId":applied["operationId"]}),
        )
        .unwrap();
        let prepared = prepare(json!({"type":"restore"})).unwrap();
        assert_eq!(prepared["commitAvailable"], false);
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
        assert!(entry()["items"].as_array().unwrap().is_empty());
        assert!(
            ipc(
                "cancel_prepared",
                json!({"operationId":prepared["operationId"]})
            )
            .is_err()
        );
        super::receive(app.handle(), "service", vec![temp.0.clone()]);
        for _ in 0..200 {
            if entry()["checking"] == false {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(entry()["items"].as_array().unwrap().len(), 1);
        let prepared = prepare(json!({"type":"restore"})).unwrap();
        super::receive(app.handle(), "service", vec![temp.0.clone()]);
        for _ in 0..200 {
            if entry()["checking"] == false {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            ipc(
                "cancel_prepared",
                json!({"operationId":prepared["operationId"]})
            )
            .is_err()
        );
        assert!(entry().get("prepared").is_none());
        assert!(entry()["items"][0].get("aliases").is_none());
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
                        ..Default::default()
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
