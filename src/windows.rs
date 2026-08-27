//! Windows backend: UI Automation live regions driven by `NotifyWinEvent`.

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

use crate::Priority;

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
