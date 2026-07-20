//! macOS "open file" plumbing.
//!
//! When a `.kugel` file is double-clicked (or dropped on the Dock icon), macOS
//! does NOT pass the path as `argv[1]` — it delivers it through the
//! `kAEOpenDocuments` Apple Event.
//!
//! We can't use the obvious `application:openFiles:` `NSApplicationDelegate`
//! hook: winit installs its *own* `NSApplicationDelegate` while building the
//! event loop (`app.setDelegate(...)`), overwriting anything we set beforehand,
//! and it panics if the delegate is ever swapped back out. winit's delegate
//! only implements `applicationDidFinishLaunching:`, so the open event is
//! rejected by AppKit's default handler — the "cannot open" alert.
//!
//! So instead we register directly with `NSAppleEventManager` for the
//! open-documents event. The catch is timing: AppKit installs its own default
//! handler during launch and dispatches the queued cold-launch event *before*
//! `applicationDidFinishLaunching:`. To win the last-writer-wins race we install
//! our handler from an `NSApplicationWillFinishLaunchingNotification` observer —
//! Apple's documented spot for Apple Event handlers, which fires after AppKit's
//! default install but before the open event is dispatched. Observing the
//! notification (rather than being the delegate) keeps winit happy.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSObject, NSObjectProtocol, NSString};
use std::path::PathBuf;
use std::sync::Mutex;

// FourCharCode (OSType) constants, as big-endian ASCII packed into u32.
const K_CORE_EVENT_CLASS: u32 = 0x6165_7674; // 'aevt'
const K_AE_OPEN_DOCUMENTS: u32 = 0x6f64_6f63; // 'odoc'
const KEY_DIRECT_OBJECT: u32 = 0x2d2d_2d2d; // '----'
const TYPE_FILE_URL: u32 = 0x6675_726c; // 'furl'

/// Files that macOS asked us to open but the app hasn't consumed yet.
static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KugelOpenHandler"]
    struct OpenHandler;

    unsafe impl NSObjectProtocol for OpenHandler {}

    impl OpenHandler {
        /// Fired during launch, before the open-documents event is dispatched.
        /// This is where we override AppKit's default open-documents handler.
        #[unsafe(method(applicationWillFinishLaunching:))]
        fn app_will_finish_launching(&self, _notification: &AnyObject) {
            let manager = unsafe { NSAppleEventManager::sharedAppleEventManager() };
            let obj: &AnyObject = self;
            unsafe {
                let _: () = msg_send![
                    &*manager,
                    setEventHandler: obj,
                    andSelector: sel!(handleAppleEvent:withReplyEvent:),
                    forEventClass: K_CORE_EVENT_CLASS,
                    andEventID: K_AE_OPEN_DOCUMENTS,
                ];
            }
        }

        /// Apple Event callback: `-handleAppleEvent:withReplyEvent:`.
        #[unsafe(method(handleAppleEvent:withReplyEvent:))]
        fn handle_apple_event(
            &self,
            event: &NSAppleEventDescriptor,
            _reply: &NSAppleEventDescriptor,
        ) {
            // The direct object is a list of file descriptors.
            let list: Option<Retained<NSAppleEventDescriptor>> =
                unsafe { msg_send![event, paramDescriptorForKeyword: KEY_DIRECT_OBJECT] };
            let Some(list) = list else { return };

            let mut paths = Vec::new();
            // Apple Event lists are 1-based.
            for i in 1..=unsafe { list.numberOfItems() } {
                let Some(item) = (unsafe { list.descriptorAtIndex(i) }) else {
                    continue;
                };
                // Coerce to a file-URL descriptor and read the URL from its raw
                // bytes. (Its `stringValue` returns a legacy HFS colon path
                // instead of a `file://` URL, so we can't use that.)
                let url_desc: Option<Retained<NSAppleEventDescriptor>> =
                    unsafe { msg_send![&*item, coerceToDescriptorType: TYPE_FILE_URL] };
                let Some(url_desc) = url_desc else { continue };
                if let Some(url) = descriptor_data_string(&url_desc) {
                    if let Some(path) = file_url_to_path(&url) {
                        paths.push(path);
                    }
                }
            }

            if !paths.is_empty() {
                if let Ok(mut pending) = PENDING.lock() {
                    pending.extend(paths);
                }
            }
        }
    }
);

impl OpenHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// Register for the open-documents Apple Event.
///
/// Must run on the main thread before `run_native`. We don't install the Apple
/// Event handler directly here (AppKit would overwrite it during launch);
/// instead we observe `NSApplicationWillFinishLaunchingNotification` and install
/// it from there.
pub fn register() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let handler = OpenHandler::new(mtm);

    let center: Retained<AnyObject> =
        unsafe { msg_send![class!(NSNotificationCenter), defaultCenter] };
    let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
    let observer: &AnyObject = &handler;
    unsafe {
        let _: () = msg_send![
            &*center,
            addObserver: observer,
            selector: sel!(applicationWillFinishLaunching:),
            name: &*name,
            object: std::ptr::null::<AnyObject>(),
        ];
    }

    // Neither NSNotificationCenter nor NSAppleEventManager retain our handler —
    // leak it so it lives for the whole process.
    std::mem::forget(handler);
}

/// Drain any files macOS has asked us to open since the last call.
pub fn take_pending() -> Vec<PathBuf> {
    PENDING
        .lock()
        .map(|mut p| std::mem::take(&mut *p))
        .unwrap_or_default()
}

/// Read an `NSAppleEventDescriptor`'s raw `data` as a UTF-8 string. For a
/// file-URL descriptor this is the `file://…` URL.
fn descriptor_data_string(desc: &NSAppleEventDescriptor) -> Option<String> {
    let data: Option<Retained<AnyObject>> = unsafe { msg_send![desc, data] };
    let data = data?;
    let len: usize = unsafe { msg_send![&*data, length] };
    let ptr: *const u8 = unsafe { msg_send![&*data, bytes] };
    if ptr.is_null() || len == 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Convert a `file://` URL (as delivered by the open-documents event) into a
/// filesystem path, percent-decoding `%XX` escapes.
fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // Drop an optional authority component (e.g. `localhost`) before the path.
    let path = match rest.find('/') {
        Some(0) => rest,
        Some(slash) => &rest[slash..],
        None => rest,
    };
    Some(PathBuf::from(percent_decode(path)))
}

/// Minimal percent-decoding for URL paths.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
