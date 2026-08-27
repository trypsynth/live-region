//! Linux backend: ATK's `notification` signal, delivered to screen readers over AT-SPI.

use std::{
	ffi::{CStr, CString, c_char, c_int, c_void},
	sync::OnceLock,
};

use wxdragon::prelude::WxWidget;

use crate::Priority;

// GTK, GObject and ATK are already linked into every wxGTK binary. `build.rs` probes
// `gtk+-3.0` so these resolve without relying on wxdragon-sys's link flags.
unsafe extern "C" {
	fn gtk_widget_get_accessible(widget: *mut c_void) -> *mut c_void;
	/// Returns a `GType`, which is `gsize`.
	fn atk_object_get_type() -> usize;
	fn g_signal_lookup(name: *const c_char, itype: usize) -> u32;
	fn g_signal_emit_by_name(instance: *mut c_void, detailed_signal: *const c_char, ...);
}

// `AtkLive`. `ATK_LIVE_NONE` is deliberately absent: we never emit an announcement we
// don't want spoken.
const ATK_LIVE_POLITE: c_int = 1;
const ATK_LIVE_ASSERTIVE: c_int = 2;

/// Which announcement signal the running ATK understands.
#[derive(Clone, Copy)]
enum Announcer {
	/// `notification`, ATK 2.50 and later. Carries an `AtkLive` politeness with the message.
	Notification,
	/// `announcement`, ATK 2.46 and later. Message only; deprecated in 2.50 by `notification`.
	Announcement,
}

/// ATK gained `notification` in 2.50, so fall back to the older `announcement` signal rather
/// than emitting a signal this ATK never registered. Resolved once, since it cannot change
/// while the process runs.
fn announcer() -> Option<Announcer> {
	static ANNOUNCER: OnceLock<Option<Announcer>> = OnceLock::new();
	*ANNOUNCER.get_or_init(|| {
		let atk_object = unsafe { atk_object_get_type() };
		if signal_exists(c"notification", atk_object) {
			Some(Announcer::Notification)
		} else if signal_exists(c"announcement", atk_object) {
			Some(Announcer::Announcement)
		} else {
			None
		}
	})
}

fn signal_exists(name: &CStr, itype: usize) -> bool {
	unsafe { g_signal_lookup(name.as_ptr(), itype) != 0 }
}

const fn live_setting(priority: Priority) -> c_int {
	match priority {
		Priority::Low | Priority::Medium => ATK_LIVE_POLITE,
		Priority::High => ATK_LIVE_ASSERTIVE,
	}
}

fn accessible(window: &impl WxWidget) -> Option<*mut c_void> {
	let handle = window.get_handle();
	if handle.is_null() {
		return None;
	}
	let accessible = unsafe { gtk_widget_get_accessible(handle) };
	if accessible.is_null() { None } else { Some(accessible) }
}

// AT-SPI has no persistent live-region flag to set: politeness travels with each individual
// announcement, so there is nothing to configure up front. Report whether announcing will
// actually be able to do anything.
pub fn set_live_region(window: &impl WxWidget) -> bool {
	announcer().is_some() && accessible(window).is_some()
}

pub fn announce(window: &impl WxWidget, message: &str, priority: Priority) -> bool {
	let Some(announcer) = announcer() else {
		return false;
	};
	let Some(accessible) = accessible(window) else {
		return false;
	};
	// `sanitize_message` strips control characters, so an interior NUL cannot reach us.
	let Ok(message) = CString::new(message) else {
		return false;
	};
	unsafe {
		match announcer {
			Announcer::Notification => {
				g_signal_emit_by_name(accessible, c"notification".as_ptr(), message.as_ptr(), live_setting(priority));
			}
			Announcer::Announcement => {
				g_signal_emit_by_name(accessible, c"announcement".as_ptr(), message.as_ptr());
			}
		}
	}
	true
}
