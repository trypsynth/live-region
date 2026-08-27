//! macOS backend: `AXAnnouncementRequested` notifications posted to `VoiceOver`.

use std::ffi::{CString, c_void};

use objc::{class, msg_send, rc::autoreleasepool, runtime::Object, sel, sel_impl};
use wxdragon::prelude::WxWidget;

use crate::Priority;

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
	fn NSAccessibilityPostNotificationWithUserInfo(
		element: *mut Object,
		notification: *mut Object,
		user_info: *mut Object,
	);
}

// libdispatch lives in libSystem, which every macOS binary links implicitly.
// `dispatch_get_main_queue()` is a C macro that expands to `&_dispatch_main_q`, so we
// declare the queue object itself and take its address, like the `dispatch` crate does.
#[repr(C)]
struct DispatchQueue {
	_private: [u8; 0],
}

unsafe extern "C" {
	static _dispatch_main_q: DispatchQueue;
	fn dispatch_async_f(queue: *const DispatchQueue, context: *mut c_void, work: unsafe extern "C" fn(*mut c_void));
}

struct PendingAnnouncement {
	message: String,
	priority: isize,
}

// A no-op stub kept signature-compatible with the other backends. Marking it `const` only
// pushes `missing_const_for_fn` up into the public wrapper in `lib.rs`, which cannot be
// `const` on the platforms whose backends call into FFI.
#[allow(clippy::missing_const_for_fn)]
pub fn set_live_region(_window: &impl WxWidget) -> bool {
	true
}

// Posting the announcement synchronously would let it race with other VoiceOver
// speech the current runloop iteration produces (e.g. a caret move right after
// announcing). Deferring the post to the next main run loop iteration makes it more likely to work.
/// There's still a race condition in VO somewhere, so it only works correctly about 90% of the time, but I haven't found an approach that's 100% bullet-proof.
pub fn announce(_window: &impl WxWidget, message: &str, priority: Priority) -> bool {
	let pending = Box::new(PendingAnnouncement { message: message.to_string(), priority: priority as isize });
	unsafe {
		dispatch_async_f(&raw const _dispatch_main_q, Box::into_raw(pending).cast(), post_announcement);
	}
	true
}

unsafe extern "C" fn post_announcement(context: *mut c_void) {
	let pending = unsafe { Box::from_raw(context.cast::<PendingAnnouncement>()) };
	autoreleasepool(|| unsafe {
		let nsapp: *mut Object = msg_send![class!(NSApplication), sharedApplication];
		let cls_nsstring = class!(NSString);
		let notif_cstr = CString::new("AXAnnouncementRequested").unwrap();
		let notification: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: notif_cstr.as_ptr()];
		let key_cstr = CString::new("AXAnnouncementKey").unwrap();
		let key: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: key_cstr.as_ptr()];
		let msg_cstr = CString::new(&*pending.message).unwrap();
		let msg_obj: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: msg_cstr.as_ptr()];
		let priority_key_cstr = CString::new("AXPriorityKey").unwrap();
		let priority_key: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: priority_key_cstr.as_ptr()];
		let priority_num: *mut Object = msg_send![class!(NSNumber), numberWithInteger: pending.priority];
		let dict: *mut Object = msg_send![class!(NSMutableDictionary), dictionary];
		let _: () = msg_send![dict, setObject: msg_obj forKey: key];
		let _: () = msg_send![dict, setObject: priority_num forKey: priority_key];
		NSAccessibilityPostNotificationWithUserInfo(nsapp, notification, dict);
	});
}
