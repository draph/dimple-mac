use tauri::{Emitter, LogicalPosition, LogicalSize, Manager};

#[derive(Clone, Copy, PartialEq)]
pub enum HideReason {
    Blur,
    Explicit,
    Success,
}

pub fn hide<R: tauri::Runtime>(app: &tauri::AppHandle<R>, reason: HideReason) {
    let on_main = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = on_main.get_webview_window("main") {
            if reason == HideReason::Blur && w.is_focused().unwrap_or(true) {
                return;
            }
            crate::entry::hide_if_allowed(&on_main, reason);
        }
    });
}
#[tauri::command]
pub fn request_hide_panel(app: tauri::AppHandle) {
    hide(&app, HideReason::Explicit);
}

#[derive(Debug, PartialEq)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
fn fit(area: Frame, desired: Frame, scale: f64) -> Frame {
    let margin = (8.0 * scale).min(area.width / 4.0).min(area.height / 4.0);
    let width = desired.width.min(area.width - margin * 2.0).max(1.0);
    let height = desired.height.min(area.height - margin * 2.0).max(1.0);
    Frame {
        x: desired
            .x
            .clamp(area.x + margin, area.x + area.width - margin - width),
        y: desired
            .y
            .clamp(area.y + margin, area.y + area.height - margin - height),
        width,
        height,
    }
}
fn logical(frame: Frame, scale: f64) -> Frame {
    Frame {
        x: frame.x / scale,
        y: frame.y / scale,
        width: frame.width / scale,
        height: frame.height / scale,
    }
}
fn from_appkit(x: f64, y: f64, width: f64, height: f64, top: f64) -> Frame {
    Frame {
        x,
        y: top - y - height,
        width,
        height,
    }
}
fn contains(frame: &Frame, point: (f64, f64)) -> bool {
    point.0 >= frame.x
        && point.0 < frame.x + frame.width
        && point.1 >= frame.y
        && point.1 < frame.y + frame.height
}
// AppKit and Tauri logical positions share a global top-left origin after flipping Y.
pub fn show<R: tauri::Runtime>(app: &tauri::AppHandle<R>, anchor: Option<(f64, f64)>) {
    let on_main = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(w) = on_main.get_webview_window("main") else {
            return;
        };
        place(&w, anchor);
        if w.show().is_ok() {
            let _ = on_main.emit("panel-visibility", true);
        }
        let _ = w.set_focus();
    });
}
fn place<R: tauri::Runtime>(w: &tauri::WebviewWindow<R>, anchor: Option<(f64, f64)>) -> Option<()> {
    use objc2_app_kit::{NSEvent, NSScreen};
    let mtm = objc2_foundation::MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let primary = screens.firstObject()?;
    let top = primary.frame().origin.y + primary.frame().size.height;
    let mouse = NSEvent::mouseLocation();
    let cursor = (mouse.x, top - mouse.y);
    let visible = w.is_visible().unwrap_or(false);
    let scale = w.scale_factor().unwrap_or(1.0);
    let current = w
        .outer_position()
        .ok()
        .zip(w.outer_size().ok())
        .map(|(p, s)| {
            logical(
                Frame {
                    x: p.x as f64,
                    y: p.y as f64,
                    width: s.width as f64,
                    height: s.height as f64,
                },
                scale,
            )
        });
    let point = if visible {
        current
            .as_ref()
            .map(|f| (f.x + f.width / 2.0, f.y + f.height / 2.0))
            .unwrap_or(cursor)
    } else {
        anchor.unwrap_or(cursor)
    };
    let frames: Vec<_> = screens
        .iter()
        .map(|screen| {
            let f = screen.frame();
            let a = screen.visibleFrame();
            (
                from_appkit(f.origin.x, f.origin.y, f.size.width, f.size.height, top),
                from_appkit(a.origin.x, a.origin.y, a.size.width, a.size.height, top),
            )
        })
        .collect();
    let index = frames
        .iter()
        .position(|(f, _)| contains(f, point))
        .or_else(|| frames.iter().position(|(f, _)| contains(f, cursor)))
        .unwrap_or(0);
    let (_, area) = &frames[index];
    let chrome = w
        .inner_size()
        .ok()
        .zip(w.outer_size().ok())
        .map(|(i, o)| {
            (
                (o.width.saturating_sub(i.width)) as f64 / scale,
                (o.height.saturating_sub(i.height)) as f64 / scale,
            )
        })
        .unwrap_or((0.0, 0.0));
    let desired = if visible {
        current
    } else {
        Some(Frame {
            x: point.0 - 240.0,
            y: if anchor.is_some() {
                point.1
            } else {
                area.y + 24.0
            },
            width: 480.0 + chrome.0,
            height: 560.0 + chrome.1,
        })
    };
    if let Some(desired) = desired {
        let frame = fit(Frame { ..*area }, desired, 1.0);
        // The usable area takes precedence over minimum size on small/removed displays.
        let _ = w.set_min_size(None::<LogicalSize<f64>>);
        let _ = w.set_size(LogicalSize::new(
            (frame.width - chrome.0).max(1.0),
            (frame.height - chrome.1).max(1.0),
        ));
        let _ = w.set_position(LogicalPosition::new(frame.x, frame.y));
    }
    Some(())
}
pub fn tray_anchor(app: &tauri::AppHandle) -> Option<(f64, f64)> {
    app.tray_by_id("main-tray")?
        .with_inner_tray_icon(|tray| {
            use objc2_app_kit::NSScreen;
            let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
            let screen = NSScreen::screens(mtm).firstObject()?;
            let top = screen.frame().origin.y + screen.frame().size.height;
            let item = tray.ns_status_item()?;
            let window = item.button(mtm)?.window()?;
            let f = window.frame();
            Some((f.origin.x + f.size.width / 2.0, top - f.origin.y))
        })
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_and_tauri_sources_agree_across_mixed_scales() {
        let expected = Frame {
            x: -1440.0,
            y: -300.0,
            width: 480.0,
            height: 560.0,
        };
        for scale in [1.0, 1.5, 2.0] {
            let actual = logical(
                Frame {
                    x: expected.x * scale,
                    y: expected.y * scale,
                    width: expected.width * scale,
                    height: expected.height * scale,
                },
                scale,
            );
            assert_eq!(actual, expected);
        }
        assert_eq!(from_appkit(-1440.0, 640.0, 480.0, 560.0, 900.0), expected);
        assert!(contains(&expected, (-1400.0, -200.0)));
        assert!(!contains(&expected, (0.0, 0.0)));
    }
    #[test]
    fn frames_fit_negative_scaled_and_removed_displays() {
        for scale in [1.0, 1.5, 2.0] {
            for (x, y) in [(-1920.0, -1080.0), (0.0, 0.0), (2560.0, -500.0)] {
                let a = Frame {
                    x,
                    y,
                    width: 800.0 * scale,
                    height: 400.0 * scale,
                };
                for (dx, dy) in [(-9000.0, -9000.0), (9000.0, 9000.0), (x, y)] {
                    let f = fit(
                        Frame { ..a },
                        Frame {
                            x: dx,
                            y: dy,
                            width: 480.0 * scale,
                            height: 600.0 * scale,
                        },
                        scale,
                    );
                    assert!(f.x >= a.x && f.y >= a.y);
                    assert!(f.x + f.width <= a.x + a.width && f.y + f.height <= a.y + a.height);
                }
            }
        }
        let a = Frame {
            x: -1000.0,
            y: 20.0,
            width: 1000.0,
            height: 800.0,
        };
        let d = Frame {
            x: -900.0,
            y: 100.0,
            width: 480.0,
            height: 560.0,
        };
        assert_eq!(fit(a, Frame { ..d }, 1.0), d);
    }
}
