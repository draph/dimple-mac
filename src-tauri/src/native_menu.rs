use crate::localization::Locale;
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as Item, Submenu};

pub fn build(app: &tauri::AppHandle, locale: Locale) -> tauri::Result<Menu<tauri::Wry>> {
    let messages = locale.messages();
    let t = |key: &str| messages[key].as_str();
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                t("app.name"),
                true,
                &[
                    &Item::about(
                        app,
                        Some(t("menu.about")),
                        Some(AboutMetadata {
                            name: Some(t("app.name").into()),
                            version: Some(env!("CARGO_PKG_VERSION").into()),
                            ..Default::default()
                        }),
                    )?,
                    &Item::separator(app)?,
                    &Item::services(app, Some(t("menu.services")))?,
                    &Item::separator(app)?,
                    &MenuItem::with_id(app, "hide-panel", t("menu.hide"), true, Some("Cmd+H"))?,
                    &Item::hide_others(app, Some(t("menu.hideOthers")))?,
                    &Item::show_all(app, Some(t("menu.showAll")))?,
                    &Item::separator(app)?,
                    &Item::quit(app, Some(t("menu.quit")))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t("menu.file"),
                true,
                &[&Item::close_window(app, Some(t("menu.close")))?],
            )?,
            &Submenu::with_items(
                app,
                t("menu.edit"),
                true,
                &[
                    &Item::undo(app, Some(t("menu.undo")))?,
                    &Item::redo(app, Some(t("menu.redo")))?,
                    &Item::separator(app)?,
                    &Item::cut(app, Some(t("menu.cut")))?,
                    &Item::copy(app, Some(t("menu.copy")))?,
                    &Item::paste(app, Some(t("menu.paste")))?,
                    &Item::select_all(app, Some(t("menu.selectAll")))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t("menu.view"),
                true,
                &[&Item::fullscreen(app, Some(t("menu.fullscreen")))?],
            )?,
            &Submenu::with_id_and_items(
                app,
                tauri::menu::WINDOW_SUBMENU_ID,
                t("menu.window"),
                true,
                &[
                    &Item::maximize(app, Some(t("menu.maximize")))?,
                    &Item::separator(app)?,
                    &Item::close_window(app, Some(t("menu.close")))?,
                ],
            )?,
            &Submenu::with_id_and_items(
                app,
                tauri::menu::HELP_SUBMENU_ID,
                t("menu.help"),
                true,
                &[],
            )?,
        ],
    )
}

pub fn recording(app: &tauri::AppHandle, locale: Locale) -> tauri::Result<Menu<tauri::Wry>> {
    let messages = locale.messages();
    Menu::with_items(
        app,
        &[&Submenu::with_items(
            app,
            &messages["app.name"],
            true,
            &[&MenuItem::new(
                app,
                &messages["settings.recording"],
                false,
                None::<&str>,
            )?],
        )?],
    )
}

pub fn tray(app: &tauri::AppHandle, locale: Locale) -> tauri::Result<Menu<tauri::Wry>> {
    let messages = locale.messages();
    let show = MenuItem::with_id(app, "show", &messages["entry.show"], true, None::<&str>)?;
    let settings = MenuItem::with_id(
        app,
        "settings",
        &messages["settings.title"],
        true,
        None::<&str>,
    )?;
    let check = MenuItem::with_id(
        app,
        "check-updates",
        &messages["menu.checkUpdates"],
        true,
        None::<&str>,
    )?;
    let before_check = Item::separator(app)?;
    let before_quit = Item::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", &messages["menu.quit"], true, None::<&str>)?;
    Menu::with_items(
        app,
        &[&show, &settings, &before_check, &check, &before_quit, &quit],
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MenuMode {
    Full,
    Recording,
}

fn menu_mode(capturing: bool) -> MenuMode {
    if capturing {
        MenuMode::Recording
    } else {
        MenuMode::Full
    }
}

pub fn apply(app: &tauri::AppHandle, locale: Locale) -> tauri::Result<()> {
    let menu = match menu_mode(crate::settings::capturing(app)) {
        MenuMode::Recording => recording(app, locale)?,
        MenuMode::Full => build(app, locale)?,
    };
    app.set_menu(menu)?;
    if let Some(icon) = app.tray_by_id("main-tray") {
        icon.set_menu(Some(tray(app, locale)?))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_capture_selects_the_recording_menu() {
        assert_eq!(menu_mode(true), MenuMode::Recording);
        assert_eq!(menu_mode(false), MenuMode::Full);
    }
}
