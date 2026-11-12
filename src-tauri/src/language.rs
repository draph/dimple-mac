use crate::localization::{AppError, Locale};
use std::{path::Path, sync::Mutex};
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Preference {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-Hans")]
    ZhHans,
    #[serde(rename = "en")]
    En,
}
impl Preference {
    fn resolve(self, system: Locale) -> Locale {
        match self {
            Self::System => system,
            Self::ZhHans => Locale::ZhHans,
            Self::En => Locale::En,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    preference: Preference,
    locale: Locale,
    revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<AppError>,
}
pub struct Language(Mutex<Status>);

fn error(reason: &str) -> AppError {
    AppError::reason("language_error", reason)
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    language: Preference,
}
fn load(path: &Path) -> (Preference, Option<AppError>) {
    match crate::config::read_limited(path, 1024) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Preference::System, None),
        result => {
            let parsed = result
                .ok()
                .filter(|v| v.len() <= 1024)
                .and_then(|v| serde_json::from_slice::<Saved>(&v).ok())
                .map(|v| v.language);
            match parsed {
                Some(value) => (value, None),
                None => (Preference::System, Some(error("damaged"))),
            }
        }
    }
}
fn persist(path: &Path, preference: Preference) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(&Saved {
        language: preference,
    })
    .map_err(|_| error("save"))?;
    crate::config::write_atomic(path, "language", &bytes).map_err(|_| error("save"))
}

trait Backend {
    fn apply(&mut self, locale: Locale) -> Result<(), AppError>;
    fn save(&mut self, preference: Preference) -> Result<(), AppError>;
}
fn change(
    backend: &mut impl Backend,
    status: &mut Status,
    desired: Preference,
    system: Locale,
) -> Result<(), AppError> {
    let previous = status.locale;
    let next = desired.resolve(system);
    if desired == status.preference && status.error.is_none() && next == status.locale {
        return Ok(());
    }
    if backend.apply(next).is_err() {
        let _ = backend.apply(previous);
        return Err(error("apply"));
    }
    if let Err(e) = backend.save(desired) {
        return Err(if backend.apply(previous).is_ok() {
            e
        } else {
            error("apply")
        });
    }
    status.preference = desired;
    status.locale = next;
    status.revision += 1;
    status.error = None;
    Ok(())
}
struct Native<'a>(&'a tauri::AppHandle);
impl Backend for Native<'_> {
    fn apply(&mut self, locale: Locale) -> Result<(), AppError> {
        crate::native_menu::apply(self.0, locale).map_err(|_| error("apply"))
    }
    fn save(&mut self, preference: Preference) -> Result<(), AppError> {
        let dir = self.0.path().app_config_dir().map_err(|_| error("save"))?;
        persist(&dir.join("language.json"), preference)
    }
}

impl Language {
    pub fn load(app: &tauri::AppHandle) -> Self {
        let (preference, error) = match app.path().app_config_dir() {
            Ok(dir) => load(&dir.join("language.json")),
            Err(_) => (Preference::System, Some(error("damaged"))),
        };
        Self::new(preference, preference.resolve(Locale::current()), error)
    }
    fn new(preference: Preference, locale: Locale, error: Option<AppError>) -> Self {
        Self(Mutex::new(Status {
            preference,
            locale,
            revision: 0,
            error,
        }))
    }
    pub fn locale(&self) -> Locale {
        self.0.lock().unwrap().locale
    }
    fn status(&self) -> Status {
        self.0.lock().unwrap().clone()
    }
    #[cfg(test)]
    pub(crate) fn for_test(locale: Locale) -> Self {
        Self::new(Preference::System, locale, None)
    }
}

#[tauri::command]
pub fn get_language<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Status {
    app.state::<Language>().status()
}
#[tauri::command]
pub async fn set_language(
    app: tauri::AppHandle,
    preference: Preference,
) -> Result<Status, AppError> {
    let (tx, mut rx) = tauri::async_runtime::channel(1);
    let on_main = app.clone();
    app.run_on_main_thread(move || {
        let value = {
            let state = on_main.state::<Language>();
            let mut status = state.0.lock().unwrap();
            let mut backend = Native(&on_main);
            change(&mut backend, &mut status, preference, Locale::current()).map(|_| status.clone())
        };
        if let Ok(status) = &value {
            let _ = on_main.emit("language-changed", status);
        }
        let _ = tx.try_send(value);
    })
    .map_err(|_| error("apply"))?;
    rx.recv().await.ok_or_else(|| error("apply"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn initial() -> Status {
        Status {
            preference: Preference::System,
            locale: Locale::En,
            revision: 0,
            error: None,
        }
    }
    #[derive(Default)]
    struct Fake {
        applied: Vec<Locale>,
        fail_apply: Vec<Locale>,
        fail_save: bool,
        saved: Option<Preference>,
    }
    impl Backend for Fake {
        fn apply(&mut self, locale: Locale) -> Result<(), AppError> {
            if self.fail_apply.contains(&locale) {
                return Err(error("apply"));
            }
            self.applied.push(locale);
            Ok(())
        }
        fn save(&mut self, preference: Preference) -> Result<(), AppError> {
            if self.fail_save {
                return Err(error("save"));
            }
            self.saved = Some(preference);
            Ok(())
        }
    }

    #[test]
    fn preferences_resolve_and_serialize_to_stable_values() {
        assert_eq!(Preference::System.resolve(Locale::ZhHans), Locale::ZhHans);
        assert_eq!(Preference::ZhHans.resolve(Locale::En), Locale::ZhHans);
        assert_eq!(Preference::En.resolve(Locale::ZhHans), Locale::En);
        for (value, name) in [
            (Preference::System, "system"),
            (Preference::ZhHans, "zh-Hans"),
            (Preference::En, "en"),
        ] {
            assert_eq!(
                serde_json::to_value(value).unwrap(),
                serde_json::Value::String(name.into())
            );
        }
        for damaged in ["fr", "zh-Hant", "system "] {
            let json = format!(r#"{{"language":"{damaged}"}}"#);
            assert!(serde_json::from_str::<Saved>(&json).is_err());
        }
    }

    #[test]
    fn changes_apply_menus_persist_and_advance_the_revision() {
        let mut backend = Fake::default();
        let mut status = initial();
        change(&mut backend, &mut status, Preference::ZhHans, Locale::En).unwrap();
        assert_eq!(status.preference, Preference::ZhHans);
        assert_eq!(status.locale, Locale::ZhHans);
        assert_eq!(status.revision, 1);
        assert_eq!(backend.applied, [Locale::ZhHans]);
        assert_eq!(backend.saved, Some(Preference::ZhHans));
        change(&mut backend, &mut status, Preference::ZhHans, Locale::En).unwrap();
        assert_eq!(status.revision, 1);
        assert_eq!(backend.applied, [Locale::ZhHans]);
        assert_eq!(Preference::System.resolve(Locale::ZhHans), Locale::ZhHans);
        change(&mut backend, &mut status, Preference::System, Locale::En).unwrap();
        assert_eq!(status.preference, Preference::System);
        assert_eq!(status.locale, Locale::En);
        assert_eq!(status.revision, 2);
        assert_eq!(backend.applied, [Locale::ZhHans, Locale::En]);
        change(
            &mut backend,
            &mut status,
            Preference::System,
            Locale::ZhHans,
        )
        .unwrap();
        assert_eq!(status.preference, Preference::System);
        assert_eq!(status.locale, Locale::ZhHans);
        assert_eq!(status.revision, 3);
        assert_eq!(
            backend.applied,
            [Locale::ZhHans, Locale::En, Locale::ZhHans]
        );
    }

    #[test]
    fn failures_keep_the_previous_choice_and_restore_menus() {
        for (failure, reason) in [("apply", "apply"), ("save", "save")] {
            let mut backend = Fake::default();
            let mut status = initial();
            if failure == "apply" {
                backend.fail_apply.push(Locale::ZhHans);
            } else {
                backend.fail_save = true;
            }
            let result = change(&mut backend, &mut status, Preference::ZhHans, Locale::En);
            assert_eq!(result.unwrap_err().params["reason"], reason);
            assert_eq!(status, initial());
            assert_eq!(backend.saved, None);
            assert_eq!(
                backend.applied,
                if failure == "apply" {
                    vec![Locale::En]
                } else {
                    vec![Locale::ZhHans, Locale::En]
                }
            );
        }
        let mut backend = Fake::default();
        let mut status = initial();
        status.error = Some(error("damaged"));
        change(&mut backend, &mut status, Preference::System, Locale::En).unwrap();
        assert_eq!(status.error, None);
        assert_eq!(status.revision, 1);
        assert_eq!(backend.saved, Some(Preference::System));
    }

    #[test]
    fn language_config_round_trip_preserves_damaged_files() {
        let dir = std::env::temp_dir().join(format!("dimple-language-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("language.json");
        assert_eq!(load(&path), (Preference::System, None));
        persist(&path, Preference::ZhHans).unwrap();
        assert_eq!(load(&path), (Preference::ZhHans, None));
        let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({ "language": "zh-Hans" }));
        for bytes in [
            b"broken".as_slice(),
            br#"{"language":"fr"}"#,
            br#"{"language":"en","extra":true}"#,
            b"{\"language\":\"en\"} trailing",
        ] {
            fs::write(&path, bytes).unwrap();
            let (value, warning) = load(&path);
            assert_eq!(value, Preference::System);
            assert!(warning.is_some());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(persist(&path, Preference::En).is_err());
        assert!(
            !dir.join(format!(".language-{}.tmp", std::process::id()))
                .exists()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_state_uses_the_requested_locale() {
        let language = Language::for_test(Locale::ZhHans);
        assert_eq!(language.locale(), Locale::ZhHans);
        assert_eq!(language.status().preference, Preference::System);
    }
}
