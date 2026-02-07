#![warn(clippy::all, clippy::cargo, clippy::nursery, clippy::pedantic, clippy::perf, unused_crate_dependencies)]
#![deny(warnings)]

use wxdragon::prelude::{StaticText, WxWidget};

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

	pub fn notify_live_region_changed(window: &impl WxWidget) -> bool {
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
		fn NSAccessibilityPostNotification(element: *mut Object, notification: *mut Object);
	}

	pub fn set_live_region(window: &impl WxWidget) -> bool {
		let handle = window.get_handle();
		if handle.is_null() {
			return false;
		}
		let view = handle as *mut Object;

		unsafe {
			let cls_nsstring = class!(NSString);

			let key_str = CString::new("AXLiveRegion").unwrap();
			let key: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: key_str.as_ptr()];

			let val_str = CString::new("Polite").unwrap();
			let val: *mut Object = msg_send![cls_nsstring, stringWithUTF8String: val_str.as_ptr()];

			let _: () = msg_send![view, accessibilitySetValue: val forAttribute: key];
		}
		true
	}

	pub fn notify_live_region_changed(window: &impl WxWidget) -> bool {
		let handle = window.get_handle();
		if handle.is_null() {
			return false;
		}
		let view = handle as *mut Object;

		unsafe {
			let cls_nsstring = class!(NSString);
			let notification_str = CString::new("AXLiveRegionChanged").unwrap();
			let notification_name: *mut Object =
				msg_send![cls_nsstring, stringWithUTF8String: notification_str.as_ptr()];

			NSAccessibilityPostNotification(view, notification_name);
		}
		true
	}
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform_impl {
	use wxdragon::prelude::WxWidget;

	pub fn set_live_region(_window: &impl WxWidget) -> bool {
		false
	}

	pub fn notify_live_region_changed(_window: &impl WxWidget) -> bool {
		false
	}
}

pub fn set_live_region(window: &impl WxWidget) -> bool {
	platform_impl::set_live_region(window)
}

pub fn announce(label: StaticText, message: &str) {
	label.set_label(message);
	let _ = platform_impl::notify_live_region_changed(&label);
}
