#![warn(clippy::all, clippy::cargo, clippy::nursery, clippy::pedantic, clippy::perf, unused_crate_dependencies)]
#![deny(warnings)]

use wxdragon::prelude::{StaticText, WxWidget};

/// How insistently a screen reader should deliver an announcement.
///
/// Acted on only by macOS `VoiceOver`. Windows screen readers ignore UIA notification priorities,
/// so the Windows implementation was omitted, the concept is absent on Linux entirely.
///
/// Note that Voice Over's speech queuing implementation has significant issues. After extensive testing,
/// I found that it seems to be governed by the following rules:
///
/// 1. The currently spoken announcement is not considered to be part of the speech queue. The queue only contains future announcements.
/// 2. If an incoming announcement has the same priority as the current one, the current announcement is interrupted prior to the new one being enqueued.
/// 3. If the new announcement has priority high, the current announcement is also interrupted, regardless of its priority.
/// 4. The queue is flushed if, and only if, both the old and new announcements have priority high.
///
/// This leads to the following counterintuitive consequences:
///
/// 1. High interrupts low (because of rule 3), low also interrupts low (because of rule 2), but medium does not interrupt low.
/// 2. If we have a low currently speaking and a medium in the queue, a new low or high
// interrupts the current low (rules 2 and 3), but it doesn't flush the queue (rule 4). This
/// means that what we hear after the interruption is the previously-enqueued medium, not the newly-arriving low/high. This is utterly cursed behavior.
/// 3. Under these rules, it is impossible to implement a polite / assertive system. There is no
// combination of priorities that guarantees queuing. always using high guarantees
/// no queuing. This does not make priorities useless, you can
/// still use them to queue announcements after standard VoiceOver speech, emitted in response to focus events and such, but it does
/// significantly limit their usefullness.
///
/// Note that all of this is likely due to Apple bugs and may change from version to version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[repr(isize)]
pub enum Priority {
	/// macOS `NSAccessibilityPriorityLow`. Lowest urgency.
	Low = 10,
	/// macOS `NSAccessibilityPriorityMedium`.
	Medium = 50,
	/// macOS `NSAccessibilityPriorityHigh`. The level used by [`announce`].
	High = 90,
}

fn sanitize_message(message: &str) -> String {
	let mut cleaned = String::new();
	for ch in message.chars() {
		if matches!(ch, '\n' | '\r' | '\t') {
			cleaned.push(' ');
		} else if !ch.is_control() {
			cleaned.push(ch);
		}
	}
	let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
	collapsed.chars().take(512).collect()
}

#[cfg(target_os = "windows")]
mod platform_impl {
	use std::{cell::RefCell, mem::ManuallyDrop};

	use windows::Win32::{
		Foundation::{HWND, RPC_E_CHANGED_MODE},
		System::{
			Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx},
			Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4},
		},
		UI::{
			Accessibility::{CLSID_AccPropServices, IAccPropServices, LiveSetting_Property_GUID, NotifyWinEvent},
			WindowsAndMessaging::{CHILDID_SELF, EVENT_OBJECT_LIVEREGIONCHANGED, OBJID_CLIENT},
		},
	};
	use wxdragon::prelude::WxWidget;

	use super::Priority;

	thread_local! {
		static ACC_PROP_SERVICES: RefCell<Option<IAccPropServices>> = const { RefCell::new(None) };
	}

	const LIVE_REGION_POLITE: u32 = 1;

	pub fn set_live_region(window: &impl WxWidget) -> bool {
		let Some(acc_prop) = acc_prop_services() else {
			return false;
		};
		let Some(hwnd) = hwnd_from_widget(window) else {
			return false;
		};
		let variant = VARIANT {
			Anonymous: VARIANT_0 {
				Anonymous: ManuallyDrop::new(VARIANT_0_0 {
					vt: VT_I4,
					wReserved1: 0,
					wReserved2: 0,
					wReserved3: 0,
					Anonymous: VARIANT_0_0_0 { lVal: LIVE_REGION_POLITE.cast_signed() },
				}),
			},
		};
		unsafe {
			acc_prop
				.SetHwndProp(hwnd, OBJID_CLIENT.0.cast_unsigned(), CHILDID_SELF, LiveSetting_Property_GUID, &variant)
				.is_ok()
		}
	}

	// `_priority` is intentionally ignored: a UI Automation live region carries a single
	// politeness setting (here, polite), not a per-announcement priority, and explicit testing
	// with Windows screen readers (NVDA/JAWS) showed they ignore per-announcement priority — so
	// there is nothing useful to act on.
	pub fn announce(window: &impl WxWidget, _message: &str, _priority: Priority) -> bool {
		notify_live_region_changed(window)
	}

	fn notify_live_region_changed(window: &impl WxWidget) -> bool {
		let Some(hwnd) = hwnd_from_widget(window) else {
			return false;
		};
		unsafe {
			NotifyWinEvent(EVENT_OBJECT_LIVEREGIONCHANGED, hwnd, OBJID_CLIENT.0, CHILDID_SELF.cast_signed());
		}
		true
	}

	fn acc_prop_services() -> Option<IAccPropServices> {
		ACC_PROP_SERVICES.with(|cell| {
			if cell.borrow().is_none()
				&& let Some(service) = init_acc_prop_services()
			{
				*cell.borrow_mut() = Some(service);
			}
			cell.borrow().clone()
		})
	}

	fn init_acc_prop_services() -> Option<IAccPropServices> {
		unsafe {
			let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
			if hr.is_err() && hr != RPC_E_CHANGED_MODE {
				return None;
			}
			CoCreateInstance(&CLSID_AccPropServices, None, CLSCTX_INPROC_SERVER).ok()
		}
	}

	fn hwnd_from_widget(widget: &impl WxWidget) -> Option<HWND> {
		let handle = widget.get_handle();
		if handle.is_null() {
			return None;
		}
		Some(HWND(handle))
	}
}

#[cfg(target_os = "macos")]
mod platform_impl {
	use std::ffi::{CString, c_void};

	use objc::{class, msg_send, rc::autoreleasepool, runtime::Object, sel, sel_impl};
	use wxdragon::prelude::WxWidget;

	use super::Priority;

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
}

#[cfg(target_os = "linux")]
mod platform_impl {
	use std::{process::Command, sync::OnceLock, thread};

	use wxdragon::prelude::WxWidget;

	use super::Priority;

	fn has_gdbus() -> bool {
		static HAS_GDBUS: OnceLock<bool> = OnceLock::new();
		*HAS_GDBUS.get_or_init(|| Command::new("gdbus").arg("--version").output().is_ok())
	}

	fn present_with_orca(message: &str) -> bool {
		if !has_gdbus() {
			return false;
		}
		Command::new("gdbus")
			.arg("call")
			.arg("--session")
			.arg("--dest")
			.arg("org.gnome.Orca.Service")
			.arg("--object-path")
			.arg("/org/gnome/Orca/Service")
			.arg("--timeout")
			.arg("1")
			.arg("--method")
			.arg("org.gnome.Orca.Service.PresentMessage")
			.arg(message)
			.output()
			.map(|output| output.status.success())
			.unwrap_or(false)
	}

	pub fn set_live_region(_window: &impl WxWidget) -> bool {
		false
	}

	// Orca's `PresentMessage` D-Bus method takes no priority, so `_priority` is ignored.
	pub fn announce(_window: &impl WxWidget, message: &str, _priority: Priority) -> bool {
		if !has_gdbus() {
			return false;
		}
		let spoken = message.to_string();
		thread::spawn(move || {
			let _ = present_with_orca(&spoken);
		});
		true
	}
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform_impl {
	use wxdragon::prelude::WxWidget;

	use super::Priority;

	pub fn set_live_region(_window: &impl WxWidget) -> bool {
		false
	}

	pub fn announce(_window: &impl WxWidget, _message: &str, _priority: Priority) -> bool {
		false
	}
}

pub fn set_live_region(window: &impl WxWidget) -> bool {
	platform_impl::set_live_region(window)
}

/// Announce `message` via the screen reader at the default ([`Priority::High`]) priority.
pub fn announce(label: StaticText, message: &str) {
	announce_with_priority(label, message, Priority::High);
}

/// Announce `message` via the screen reader at an explicit [`Priority`].
///
/// Only macOS acts on `priority`. See the documentation of [`Priority`] for rationale and caveats.
///
/// On macOS the announcement is delivered asynchronously, on the next main run loop iteration,
/// so it isn't drowned out by accessibility notifications (such as caret moves) that the calling
/// event handler produces after announcing.
pub fn announce_with_priority(label: StaticText, message: &str, priority: Priority) {
	let message = sanitize_message(message);
	if message.is_empty() {
		return;
	}
	#[cfg(target_os = "windows")]
	label.set_label(&message);
	let _ = platform_impl::announce(&label, &message, priority);
}
