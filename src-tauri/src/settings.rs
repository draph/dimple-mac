use crate::localization::AppError;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutSettings {
    enabled: bool,
    shortcut: String,
}
impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            shortcut: "alt+KeyF".into(),
        }
    }
}
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    settings: ShortcutSettings,
    active: Vec<String>,
    uncertain: Vec<String>,
    saved: bool,
    error: Option<AppError>,
    open: bool,
    capturing: bool,
}
#[derive(Default)]
pub struct Settings(Mutex<Status>);
fn error(reason: &str) -> AppError {
    AppError::reason("shortcut_error", reason)
}
fn normalize(value: &str) -> Result<String, AppError> {
    if value.len() > 100 {
        return Err(error("invalid"));
    }
    let key: Shortcut = value.parse().map_err(|_| error("invalid"))?;
    let name = key.key.to_string();
    let supported = name
        .strip_prefix("Key")
        .is_some_and(|v| v.len() == 1 && v.as_bytes()[0].is_ascii_uppercase())
        || name
            .strip_prefix("Digit")
            .is_some_and(|v| v.len() == 1 && v.as_bytes()[0].is_ascii_digit())
        || name
            .strip_prefix('F')
            .and_then(|v| v.parse::<u8>().ok())
            .is_some_and(|v| (1..=24).contains(&v))
        || [
            "Space",
            "Enter",
            "Tab",
            "Backspace",
            "Delete",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "Comma",
            "Period",
            "Slash",
            "Semicolon",
            "Quote",
            "BracketLeft",
            "BracketRight",
            "Backslash",
            "Minus",
            "Equal",
            "Backquote",
        ]
        .contains(&name.as_str());
    if !supported
        || !key
            .mods
            .intersects(Modifiers::ALT | Modifiers::CONTROL | Modifiers::SUPER)
    {
        return Err(error("invalid"));
    }
    Ok(key.into_string())
}
fn load(path: &Path) -> (ShortcutSettings, Option<AppError>) {
    let bytes = fs::File::open(path).and_then(|file| {
        let mut bytes = Vec::new();
        file.take(1025).read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    match bytes {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (ShortcutSettings::default(), None),
        result => {
            let parsed = result
                .ok()
                .filter(|v| v.len() <= 1024)
                .and_then(|v| serde_json::from_slice::<ShortcutSettings>(&v).ok())
                .and_then(|mut v| {
                    v.shortcut = normalize(&v.shortcut).ok()?;
                    Some(v)
                });
            match parsed {
                Some(v) => (v, None),
                None => (ShortcutSettings::default(), Some(error("damaged"))),
            }
        }
    }
}
fn persist(path: &Path, value: &ShortcutSettings) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| error("save"))?;
    fs::create_dir_all(parent).map_err(|_| error("save"))?;
    let temp = parent.join(format!(".shortcut-{}.tmp", std::process::id()));
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| error("save"))?;
    let result = (|| {
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }

    result.map_err(|_| error("save"))
}

trait Backend {
    fn register(&mut self, key: &str) -> Result<(), AppError>;
    fn unregister(&mut self, key: &str) -> Result<(), AppError>;
    fn save(&mut self, value: &ShortcutSettings) -> Result<(), AppError>;
}
fn register(b: &mut impl Backend, s: &mut Status, key: &str) -> Result<(), AppError> {
    if !s.active.iter().any(|k| k == key) {
        b.register(key)?;
        s.active.push(key.into());
    }
    Ok(())
}
fn unregister(b: &mut impl Backend, s: &mut Status, key: &str) -> Result<(), AppError> {
    let result = b.unregister(key);
    s.active.retain(|k| k != key);
    if result.is_err() {
        // An OS release error cannot prove registration state; require restart.
        s.uncertain.push(key.into());
    }
    result?;
    Ok(())
}
fn save(b: &mut impl Backend, s: &mut Status, mut desired: ShortcutSettings) {
    if !s.uncertain.is_empty() {
        s.saved = false;
        s.error = Some(error("restart"));
        return;
    }
    s.error = None;
    let previous = s.active.clone();
    let result = (|| {
        desired.shortcut = normalize(&desired.shortcut)?;
        if desired.enabled {
            register(b, s, &desired.shortcut)?;
        }
        for key in s.active.clone() {
            if !desired.enabled || key != desired.shortcut {
                unregister(b, s, &key)?;
            }
        }
        b.save(&desired)
    })();
    if let Err(e) = result {
        let mut restored = true;
        // Restore old registrations before removing the replacement.
        for key in &previous {
            if s.uncertain.contains(key) || register(b, s, key).is_err() {
                restored = false;
            }
        }
        for key in s.active.clone() {
            if !previous.contains(&key) && unregister(b, s, &key).is_err() {
                restored = false;
            }
        }
        s.saved = false;
        s.error = Some(if !s.uncertain.is_empty() {
            error("restart")
        } else if restored {
            e
        } else {
            error("rollback")
        });
    } else {
        s.settings = desired;
        s.saved = true;
    }
}
struct Native<'a>(&'a tauri::AppHandle);
impl Backend for Native<'_> {
    fn register(&mut self, key: &str) -> Result<(), AppError> {
        self.0
            .global_shortcut()
            .register(key)
            .map_err(|_| error("register"))
    }
    fn unregister(&mut self, key: &str) -> Result<(), AppError> {
        self.0
            .global_shortcut()
            .unregister(key)
            .map_err(|_| error("unregister"))
    }
    fn save(&mut self, value: &ShortcutSettings) -> Result<(), AppError> {
        let dir = self.0.path().app_config_dir().map_err(|_| error("save"))?;
        persist(&dir.join("shortcut.json"), value)
    }
}
pub fn init(app: &tauri::AppHandle) {
    let (settings, warning) = match app.path().app_config_dir() {
        Ok(dir) => load(&dir.join("shortcut.json")),
        Err(_) => (ShortcutSettings::default(), Some(error("save"))),
    };
    let mut s = app.state::<Settings>().0.lock().unwrap().clone();
    s.settings = settings.clone();
    s.error = warning;
    s.saved = s.error.is_none();
    if settings.enabled
        && let Err(e) = register(&mut Native(app), &mut s, &settings.shortcut)
    {
        s.error = Some(e);
        s.saved = false;
    }
    *app.state::<Settings>().0.lock().unwrap() = s;
}
pub fn handle(app: &tauri::AppHandle, key: &Shortcut) -> bool {
    let state = app.state::<Settings>();
    let s = state.0.lock().unwrap();
    let key = key.into_string();
    if s.capturing {
        drop(s);
        let _ = app.emit("shortcut-captured", key);
        false
    } else {
        s.active.contains(&key) || s.uncertain.contains(&key)
    }
}
pub fn protects_panel<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    app.try_state::<Settings>()
        .is_some_and(|s| s.0.lock().unwrap().capturing)
}
pub(crate) fn set_open<R: tauri::Runtime>(app: &tauri::AppHandle<R>, open: bool) {
    if let Some(state) = app.try_state::<Settings>() {
        let mut s = state.0.lock().unwrap();
        if s.open != open {
            s.open = open;
            drop(s);
            let _ = app.emit("shortcut-settings", ());
        }
    }
}
pub fn open<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    crate::entry::open_settings(app);
}
#[tauri::command]
pub fn get_shortcut_settings<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Status {
    app.state::<Settings>().0.lock().unwrap().clone()
}
#[tauri::command]
pub async fn set_shortcut_capture(app: tauri::AppHandle, capturing: bool) -> Result<(), AppError> {
    let (tx, mut rx) = tauri::async_runtime::channel(1);
    let on_main = app.clone();
    app.run_on_main_thread(move || {
        let locale = on_main.state::<crate::Session>().locale;
        let result = (|| {
            let menu = if capturing {
                crate::native_menu::recording(&on_main, locale)
            } else {
                crate::native_menu::build(&on_main, locale)
            }?;
            on_main.set_menu(menu)?;
            on_main.state::<Settings>().0.lock().unwrap().capturing = capturing;
            Ok::<_, tauri::Error>(())
        })();
        let _ = tx.try_send(result.map_err(|_| error("capture")));
    })
    .map_err(|_| error("capture"))?;
    rx.recv().await.ok_or_else(|| error("capture"))?
}
#[tauri::command]
pub fn close_settings(app: tauri::AppHandle) {
    app.state::<Settings>().0.lock().unwrap().open = false;
}
#[tauri::command]
pub fn open_settings<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    open(&app);
}
#[tauri::command]
pub async fn save_shortcut_settings(
    app: tauri::AppHandle,
    settings: ShortcutSettings,
) -> Result<Status, AppError> {
    let (tx, rx) = tauri::async_runtime::channel(1);
    let on_main = app.clone();
    app.run_on_main_thread(move || {
        let state = on_main.state::<Settings>();
        let mut s = state.0.lock().unwrap();
        s.capturing = false;
        save(&mut Native(&on_main), &mut s, settings);
        let _ = tx.try_send(s.clone());
    })
    .map_err(|_| error("save"))?;
    let mut rx = rx;
    rx.recv().await.ok_or_else(|| error("save"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake {
        active: Vec<String>,
        fail_register: Vec<String>,
        fail_unregister: Vec<String>,
        fail_save: bool,
        disk: Option<ShortcutSettings>,
    }
    impl Backend for Fake {
        fn register(&mut self, key: &str) -> Result<(), AppError> {
            if self.fail_register.contains(&key.into()) {
                return Err(error("register"));
            }
            assert!(!self.active.contains(&key.into()));
            self.active.push(key.into());
            Ok(())
        }
        fn unregister(&mut self, key: &str) -> Result<(), AppError> {
            if self.fail_unregister.contains(&key.into()) {
                return Err(error("unregister"));
            }
            assert!(self.active.contains(&key.into()));
            self.active.retain(|k| k != key);
            Ok(())
        }
        fn save(&mut self, s: &ShortcutSettings) -> Result<(), AppError> {
            if self.fail_save {
                return Err(error("save"));
            }
            self.disk = Some(s.clone());
            Ok(())
        }
    }
    fn initial() -> (Fake, Status) {
        let s = Status {
            active: vec!["alt+KeyF".into()],
            saved: true,
            ..Default::default()
        };
        (
            Fake {
                active: s.active.clone(),
                ..Default::default()
            },
            s,
        )
    }
    fn changed() -> ShortcutSettings {
        ShortcutSettings {
            enabled: true,
            shortcut: "Ctrl+Shift+KeyG".into(),
        }
    }
    #[test]
    fn changes_disable_repeat_and_conflicts_report_actual_registration() {
        let (mut b, mut s) = initial();
        save(&mut b, &mut s, changed());
        assert!(s.saved);
        assert_eq!(s.active, ["shift+control+KeyG"]);
        save(&mut b, &mut s, changed());
        assert!(s.saved);
        let mut disabled = changed();
        disabled.enabled = false;
        save(&mut b, &mut s, disabled);
        assert!(s.active.is_empty());
        assert!(!s.settings.enabled);
        b.fail_register.push("alt+KeyF".into());
        save(&mut b, &mut s, ShortcutSettings::default());
        assert!(!s.saved);
        assert!(s.active.is_empty());
        assert!(!s.settings.enabled);
    }
    #[test]
    fn failures_roll_back_and_uncertain_unregistration_requires_restart() {
        for reason in ["save", "unregister", "rollback"] {
            let (mut b, mut s) = initial();
            if reason == "unregister" {
                b.fail_unregister.push("alt+KeyF".into());
            } else {
                b.fail_save = true;
            }
            if reason == "rollback" {
                b.fail_register.push("alt+KeyF".into());
            }
            save(&mut b, &mut s, changed());
            assert!(!s.saved);
            assert_eq!(
                s.error.as_ref().unwrap().params["reason"],
                if reason == "unregister" {
                    "restart"
                } else {
                    reason
                }
            );
            let mut actual = s.active.clone();
            actual.extend(s.uncertain.clone());
            actual.sort();
            let mut native = b.active.clone();
            native.sort();
            assert_eq!(actual, native);
            assert_eq!(s.settings, ShortcutSettings::default());
            assert!(b.disk.is_none());
            if reason == "unregister" {
                assert_eq!(s.uncertain, ["alt+KeyF"]);
                // Model the platform losing its handle: a second unregister would falsely succeed.
                b.fail_unregister.clear();
                let mut disabled = changed();
                disabled.enabled = false;
                save(&mut b, &mut s, disabled);
                assert!(!s.saved);
                assert_eq!(s.uncertain, ["alt+KeyF"]);
                assert_eq!(b.active, ["alt+KeyF"]);
            } else if reason == "save" {
                assert_eq!(s.active, ["alt+KeyF"]);
            } else {
                assert!(s.active.is_empty());
            }
        }
    }
    #[test]
    fn disabling_save_failure_restores_old_registration_or_reports_rollback_failure() {
        for fail_restore in [false, true] {
            let (mut b, mut s) = initial();
            b.fail_save = true;
            if fail_restore {
                b.fail_register.push("alt+KeyF".into());
            }
            let disabled = ShortcutSettings {
                enabled: false,
                ..ShortcutSettings::default()
            };
            save(&mut b, &mut s, disabled);
            assert!(!s.saved);
            assert!(s.settings.enabled);
            assert!(b.disk.is_none());
            assert_eq!(s.active.is_empty(), fail_restore);
            assert_eq!(
                s.error.as_ref().unwrap().params["reason"],
                if fail_restore { "rollback" } else { "save" }
            );
        }
    }
    #[test]
    fn validates_and_normalizes_shortcuts() {
        assert_eq!(normalize("Option+F").unwrap(), "alt+KeyF");
        for key in [
            "",
            "F",
            "Shift+KeyF",
            "Alt+Escape",
            "Alt+KeyF+KeyG",
            "Alt+Bad",
            "Alt+AltLeft",
        ] {
            assert!(normalize(key).is_err(), "{key}");
        }
    }
    #[test]
    fn config_round_trip_is_small_and_damaged_data_is_preserved() {
        let dir =
            std::env::temp_dir().join(format!("folderify-settings-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shortcut.json");
        assert_eq!(load(&path), (ShortcutSettings::default(), None));
        let mut disabled = changed();
        disabled.enabled = false;
        persist(&path, &disabled).unwrap();
        let (value, warning) = load(&path);
        assert!(!value.enabled);
        assert!(warning.is_none());
        assert_eq!(value.shortcut, "shift+control+KeyG");
        let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 2);
        for bytes in [
            b"broken".as_slice(),
            br#"{"enabled":true,"shortcut":"KeyF"}"#,
            br#"{"enabled":true,"shortcut":"Alt+F","targets":[]}"#,
        ] {
            fs::write(&path, bytes).unwrap();
            let (_, warning) = load(&path);
            assert!(warning.is_some());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let temp = dir.join(format!(".shortcut-{}.tmp", std::process::id()));
        fs::write(&temp, b"unowned").unwrap();
        assert!(persist(&path, &ShortcutSettings::default()).is_err());
        assert_eq!(fs::read(&temp).unwrap(), b"unowned");
        fs::remove_file(&temp).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(persist(&path, &ShortcutSettings::default()).is_err());
        assert!(!temp.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
