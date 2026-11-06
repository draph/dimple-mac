use objc2::runtime::AnyObject;
use objc2::{MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSPasteboard, NSView, NSWorkspace};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSString, NSURL};
use std::{path::PathBuf, sync::OnceLock};
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
pub fn finder_is_frontmost() -> bool {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|a| a.bundleIdentifier())
        .is_some_and(|s| s.to_string() == "com.apple.finder")
}
fn paths(board: &NSPasteboard) -> Vec<PathBuf> {
    // Finder supplies file URLs; never split or trim decoded filesystem paths.
    let mut paths = Vec::new();
    if let Some(items) = board.pasteboardItems() {
        for item in items {
            if let Some(value) = item.stringForType(&NSString::from_str("public.file-url"))
                && let Some(url) = NSURL::URLWithString(&value)
                && url.isFileURL()
                && let Some(path) = url.path()
            {
                paths.push(PathBuf::from(path.to_string()));
            } else {
                return Vec::new();
            }
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
            if let Some(app) = APP.get() { crate::entry::receive(app, "service", paths(board)); }
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
            if let Some(app) = APP.get() { crate::entry::receive(app, "tray-drop", paths(&board)); }
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
