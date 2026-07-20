//! macOS "open file" plumbing.
//!
//! When a `.kugel` file is double-clicked (or dropped on the Dock icon), macOS
//! does NOT pass the path as `argv[1]` — it delivers it through the
//! `application:openFiles:` Apple Event. winit intentionally does not register
//! an `NSApplicationDelegate`, so we register our own to catch that event and
//! queue the paths for the egui app to pick up on its next frame.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString};
use std::path::PathBuf;
use std::sync::Mutex;

/// Files that macOS asked us to open but the app hasn't consumed yet.
static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KugelAppDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(application:openFiles:))]
        fn application_open_files(&self, _app: &NSApplication, files: &NSArray<NSString>) {
            if let Ok(mut pending) = PENDING.lock() {
                for file in files.iter() {
                    pending.push(PathBuf::from(file.to_string()));
                }
            }
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// Install the delegate. Must run on the main thread before the winit/eframe
/// event loop starts, so a cold-launch open event is not missed.
pub fn register() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let delegate = AppDelegate::new(mtm);
    let app = NSApplication::sharedApplication(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // NSApplication holds its delegate weakly — leak ours so it lives for the
    // whole process instead of being deallocated at the end of this function.
    std::mem::forget(delegate);
}

/// Drain any files macOS has asked us to open since the last call.
pub fn take_pending() -> Vec<PathBuf> {
    PENDING
        .lock()
        .map(|mut p| std::mem::take(&mut *p))
        .unwrap_or_default()
}
