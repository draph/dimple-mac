use objc2::runtime::AnyObject;
use objc2::{MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSPasteboard, NSView, NSWorkspace};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSString, NSURL};
use std::{path::PathBuf, sync::OnceLock};
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

#[tauri::command]
pub fn open_project_github() -> Result<(), crate::localization::AppError> {
    let url = NSURL::URLWithString(&NSString::from_str("https://github.com/draph/dimple-mac"))
        .expect("Project URL must be valid");
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(crate::localization::AppError::reason("about_error", "open"))
    }
}

pub fn finder_is_frontmost() -> bool {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|a| a.bundleIdentifier())
        .is_some_and(|s| s.to_string() == "com.apple.finder")
}
fn file_path(
    value: Option<&NSString>,
    index: usize,
) -> Result<PathBuf, crate::localization::AppError> {
    if value.is_some_and(|value| value.len() > 1024 * 1024) {
        return Err(crate::localization::AppError::reason(
            "invalid_input",
            "budget",
        ));
    }
    value
        .and_then(NSURL::URLWithString)
        .filter(|url| {
            url.isFileURL()
                && url
                    .host()
                    .is_none_or(|host| host.is_empty() || host.to_string() == "localhost")
        })
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .filter(|path| path.is_absolute())
        .ok_or_else(|| {
            crate::localization::AppError::reason("invalid_input", "file_url")
                .param("index", index + 1)
        })
}
fn paths(board: &NSPasteboard) -> Vec<Result<PathBuf, crate::localization::AppError>> {
    // Finder supplies file URLs; never split or trim decoded filesystem paths.
    let mut paths = Vec::new();
    if let Some(items) = board.pasteboardItems() {
        if items.len() > 4096 {
            return vec![Err(crate::localization::AppError::reason(
                "invalid_input",
                "budget",
            ))];
        }
        let mut length = 0usize;
        for (index, item) in items.iter().enumerate() {
            let value = item.stringForType(&NSString::from_str("public.file-url"));
            length = length.saturating_add(value.as_ref().map_or(0, |value| value.len()));
            if length > 1024 * 1024 {
                return vec![Err(crate::localization::AppError::reason(
                    "invalid_input",
                    "budget",
                ))];
            }
            paths.push(file_path(value.as_deref(), index));
        }
    }
    paths
}
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct Service;
    impl Service {
        #[unsafe(method(folderify:userData:error:))]
        fn serve(&self, board: &NSPasteboard, _data: Option<&NSString>, _error: *mut *mut NSString) {
            if let Some(app) = APP.get() { crate::entry::receive_native(app, "service", paths(board)); }
        }
    }
);
define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct DropView;
    impl DropView {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: objc2_foundation::NSPoint) -> Option<&NSView> { None }
        #[unsafe(method(draggingEntered:))]
        fn entered(&self, _sender: &AnyObject) -> usize { 1 }
        #[unsafe(method(performDragOperation:))]
        fn perform(&self, sender: &AnyObject) -> bool {
            // SAFETY: AppKit supplies an NSDraggingInfo object for this selector.
            let board: objc2::rc::Retained<NSPasteboard> = unsafe { msg_send![sender, draggingPasteboard] };
            if let Some(app) = APP.get() { crate::entry::receive_native(app, "tray-drop", paths(&board)); }
            true
        }
    }
);
pub fn install(app: &tauri::AppHandle, tray: &tauri::tray::TrayIcon) -> tauri::Result<()> {
    let _ = APP.set(app.clone());
    let mtm = MainThreadMarker::new().expect("Native setup requires main thread");
    // SAFETY: Service is an NSObject subclass with a valid default initializer.
    let service: objc2::rc::Retained<Service> = unsafe { msg_send![Service::alloc(mtm), init] };
    unsafe {
        NSApplication::sharedApplication(mtm).setServicesProvider(Some(&service));
    }
    objc2_app_kit::NSUpdateDynamicServices();
    tray.with_inner_tray_icon(|tray| {
        let mtm = MainThreadMarker::new().unwrap();
        if let Some(item) = tray.ns_status_item()
            && let Some(button) = item.button(mtm)
        {
            // SAFETY: The view is initialized on the main thread; the button retains it.
            let view: objc2::rc::Retained<DropView> =
                unsafe { msg_send![DropView::alloc(mtm), initWithFrame: button.bounds()] };
            view.registerForDraggedTypes(&NSArray::from_retained_slice(&[NSString::from_str(
                "public.file-url",
            )]));
            button.addSubview(&view);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_file_urls_preserve_paths_and_report_each_invalid_item() {
        objc2::rc::autoreleasepool(|_| {
            let path = "/tmp/ 中文\nquote\"\\ space ";
            let url = NSURL::fileURLWithPath(&NSString::from_str(path));
            let value = url.absoluteString().unwrap();
            assert_eq!(file_path(Some(&value), 0).unwrap(), PathBuf::from(path));
            for value in [
                None,
                Some(NSString::from_str("https://example.com/file")),
                Some(NSString::from_str("file://remote.example/tmp/folder")),
                Some(NSString::from_str("not a file URL")),
            ] {
                let error = file_path(value.as_deref(), 2).unwrap_err();
                assert_eq!(error.params["reason"], "file_url");
                assert_eq!(error.params["index"], 3);
            }
        });
    }
}
