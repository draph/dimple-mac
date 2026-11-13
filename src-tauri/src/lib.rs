mod assets;
mod builtins;
mod config;
mod entry;
mod icon;
mod instance;
mod language;
pub mod localization;
mod macos;
mod native_menu;
mod operation;
mod panel;
mod settings;
mod targets;
mod text;

use localization::Locale;
use tauri::Manager;

#[derive(Default)]
struct TrayWake(std::sync::Mutex<Option<bool>>);

fn wake_from_tray(app: &tauri::AppHandle) {
    let finder = app
        .state::<TrayWake>()
        .0
        .lock()
        .unwrap()
        .take()
        .unwrap_or_else(macos::finder_is_frontmost);
    entry::wake_at(app, finder, panel::tray_anchor(app));
}

#[derive(Clone, serde::Serialize)]
struct Session {
    version: &'static str,
    locale: Locale,
}

#[derive(serde::Serialize)]
struct SessionSnapshot {
    #[serde(flatten)]
    info: Session,
    entry: entry::Snapshot,
    operation: entry::OperationState,
}

fn session_snapshot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    info: Session,
) -> SessionSnapshot {
    let (entry, operation) = entry::session(app);
    SessionSnapshot {
        info,
        entry,
        operation,
    }
}

#[tauri::command]
fn get_session<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> SessionSnapshot {
    let locale = app.state::<language::Language>().locale();
    session_snapshot(
        &app,
        Session {
            version: env!("CARGO_PKG_VERSION"),
            locale,
        },
    )
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    if !entry::request_quit(&app) {
        app.exit(0);
    }
}

#[tauri::command]
fn panel_visible(window: tauri::WebviewWindow) -> bool {
    window.is_visible().unwrap_or(false)
}

pub fn run() {
    if targets::worker() {
        return;
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed
                        && settings::handle(app, shortcut)
                    {
                        entry::wake(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .manage(entry::Inbox::default())
        .manage(TrayWake::default())
        .manage(settings::Settings::default())
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "hide-panel" {
                panel::hide(app);
            }
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                panel::hide(window.app_handle());
            }
            tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                entry::receive(window.app_handle(), "window-drop", paths.clone())
            }
            _ => {}
        })
        .setup(|app| {
            instance::install(app.handle())?;
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let language = language::Language::load(app.handle());
            let locale = language.locale();
            app.manage(language);
            app.set_menu(native_menu::build(app.handle(), locale)?)?;
            settings::init(app.handle());
            let messages = locale.messages();
            let tray = tauri::tray::TrayIconBuilder::with_id("main-tray")
                .icon(tauri::include_image!("icons/tray-folder.png"))
                .icon_as_template(true)
                .tooltip(&messages["app.name"])
                .menu(&native_menu::tray(app.handle(), locale)?)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| {
                    if e.id.as_ref() == "settings" {
                        panel::show(app, panel::tray_anchor(app));
                        settings::open(app);
                    }
                    if e.id.as_ref() == "show" {
                        wake_from_tray(app);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button,
                        button_state,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        match button_state {
                            tauri::tray::MouseButtonState::Down => {
                                *app.state::<TrayWake>().0.lock().unwrap() =
                                    Some(macos::finder_is_frontmost());
                            }
                            tauri::tray::MouseButtonState::Up
                                if button == tauri::tray::MouseButton::Left =>
                            {
                                wake_from_tray(app)
                            }
                            _ => {}
                        }
                    }
                })
                .build(app)?;
            macos::install(app.handle(), &tray)?;
            entry::show(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_session,
            language::get_language,
            language::set_language,
            settings::get_shortcut_settings,
            settings::save_shortcut_settings,
            settings::set_shortcut_capture,
            settings::open_settings,
            settings::close_settings,
            quit_app,
            macos::open_project_github,
            entry::get_entry,
            entry::begin_edit,
            entry::set_render_params,
            panel_visible,
            entry::choose_targets,
            entry::choose_badge,
            entry::select_badge,
            entry::set_text_badge,
            entry::get_badge_mask,
            entry::finder_selection,
            entry::remove_target,
            entry::prepare_operation,
            entry::prepare_recovery,
            entry::stop_operation,
            entry::cancel_prepared,
            entry::get_operation,
            entry::commit_operation
        ])
        .build(tauri::generate_context!())
        .expect("Failed to start Dimple")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = &event
                && entry::request_quit(app)
            {
                api.prevent_exit();
            }
            if matches!(event, tauri::RunEvent::Reopen { .. }) {
                entry::show(app);
            }
        });
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
