pub mod localization;
mod native_menu;

use localization::Locale;
use tauri::Manager;

#[derive(Clone, serde::Serialize)]
struct Session {
    version: &'static str,
    locale: Locale,
}

#[tauri::command]
fn get_session(session: tauri::State<'_, Session>) -> Session {
    session.inner().clone()
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let locale = Locale::current();
            app.set_menu(native_menu::build(app.handle(), locale)?)?;
            app.manage(Session {
                version: env!("CARGO_PKG_VERSION"),
                locale,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_session])
        .run(tauri::generate_context!())
        .expect("Failed to start Folderify Desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_serializes_the_running_version_and_locale() {
        for (locale, name) in [(Locale::En, "en"), (Locale::ZhHans, "zh-Hans")] {
            let value = serde_json::to_value(Session {
                version: env!("CARGO_PKG_VERSION"),
                locale,
            })
            .unwrap();
            assert_eq!(
                value,
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"), "locale": name
                })
            );
        }
    }
}
