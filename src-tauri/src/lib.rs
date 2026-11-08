mod assets;
mod builtins;
mod entry;
mod icon;
mod instance;
pub mod localization;
mod macos;
mod native_menu;
mod operation;
mod targets;
mod text;

use localization::Locale;
use tauri::Manager;

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
fn get_session<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session: tauri::State<'_, Session>,
) -> SessionSnapshot {
    session_snapshot(&app, session.inner().clone())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    if !entry::request_quit(&app) {
        app.exit(0);
    }
}

pub fn run() {
    if targets::worker() {
        return;
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        entry::wake(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .manage(entry::Inbox::default())
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if entry::can_hide(window.app_handle()) {
                    let _ = window.hide();
                }
            }
            tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                entry::receive(window.app_handle(), "window-drop", paths.clone())
            }
            _ => {}
        })
        .setup(|app| {
            instance::install(app.handle())?;
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let locale = Locale::current();
            app.set_menu(native_menu::build(app.handle(), locale)?)?;
            app.manage(Session {
                version: env!("CARGO_PKG_VERSION"),
                locale,
            });
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            if app.global_shortcut().register("Alt+KeyF").is_err() {
                entry::error(app.handle(), "shortcutFailed");
            }
            let messages = locale.messages();
            let show = tauri::menu::MenuItem::with_id(
                app,
                "show",
                &messages["entry.show"],
                true,
                None::<&str>,
            )?;
            let quit = tauri::menu::PredefinedMenuItem::quit(app, Some(&messages["menu.quit"]))?;
            let about =
                tauri::menu::PredefinedMenuItem::about(app, Some(&messages["menu.about"]), None)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &about, &quit])?;
            let tray = tauri::tray::TrayIconBuilder::new()
                .icon(tauri::include_image!("icons/tray-folder.png"))
                .icon_as_template(true)
                .tooltip(&messages["app.name"])
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| {
                    if e.id.as_ref() == "show" {
                        entry::wake(app);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        }
                    ) {
                        entry::wake(tray.app_handle());
                    }
                })
                .build(app)?;
            macos::install(app.handle(), &tray)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_session,
            quit_app,
            entry::get_entry,
            entry::begin_edit,
            entry::set_pinned,
            entry::choose_targets,
            entry::choose_badge,
            entry::select_badge,
            entry::set_text_badge,
            entry::get_badge_mask,
            entry::finder_selection,
            entry::remove_target,
            entry::prepare_operation,
            entry::cancel_prepared,
            entry::get_operation,
            entry::commit_operation
        ])
        .build(tauri::generate_context!())
        .expect("Failed to start Folderify Desktop")
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
