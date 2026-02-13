#![warn(clippy::all, clippy::cargo, clippy::nursery, clippy::pedantic, clippy::perf, unused_crate_dependencies)]
#![deny(warnings)]

use wxdragon::prelude::{StaticText, WxWidget};

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

	pub fn announce(window: &impl WxWidget, _message: &str) -> bool {
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
	use std::ffi::CString;

	use objc::{class, msg_send, runtime::Object, sel, sel_impl};
	use wxdragon::prelude::WxWidget;

	#[link(name = "AppKit", kind = "framework")]
	unsafe extern "C" {
		fn NSAccessibilityPostNotificationWithUserInfo(
			element: *mut Object,
			notification: *mut Object,
			user_info: *mut Object,
		);
	}

	pub fn set_live_region(_window: &impl WxWidget) -> bool {
		true
	}

	pub fn announce(_window: &impl WxWidget, message: &str) -> bool {
		unsafe {
			let nsapp: *mut Object = msg_send![class!(NSApplication), sharedApplication];
			let cls_nsstring = class!(NSString);
			let notif_cstr = CString::new("AXAnnouncementRequested").unwrap();
			let notification: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: notif_cstr.as_ptr()];
			let key_cstr = CString::new("AXAnnouncementKey").unwrap();
			let key: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: key_cstr.as_ptr()];
			let msg_cstr = CString::new(message).unwrap();
			let msg_obj: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: msg_cstr.as_ptr()];
			let dict: *mut Object = msg_send![class!(NSDictionary), dictionaryWithObject:msg_obj forKey:key];
			NSAccessibilityPostNotificationWithUserInfo(nsapp, notification, dict);
		}
		true
	}
}

#[cfg(target_os = "linux")]
mod platform_impl {
	use std::{process::Command, sync::OnceLock, thread};

	use wxdragon::prelude::WxWidget;

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

	pub fn announce(_window: &impl WxWidget, message: &str) -> bool {
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

	pub fn set_live_region(_window: &impl WxWidget) -> bool {
		false
	}

	pub fn announce(_window: &impl WxWidget, _message: &str) -> bool {
		false
	}
}

pub fn set_live_region(window: &impl WxWidget) -> bool {
	platform_impl::set_live_region(window)
}

pub fn announce(label: StaticText, message: &str) {
	let message = sanitize_message(message);
	if message.is_empty() {
		return;
	}
	#[cfg(target_os = "windows")]
	label.set_label(&message);
	let _ = platform_impl::announce(&label, &message);
}
