//! Windows backend: UI Automation notification events.
//!
//! A notification carries its text in the event itself, so nothing has to be written into the
//! announcer window first, and it carries a processing mode that screen readers act on. NVDA
//! calls `speech.cancelSpeech()` for `ImportantMostRecent` and `MostRecent` (and, while a say
//! all is running, speaks at `Spri.NOW` instead of cancelling), which is what makes
//! [`Priority::High`] genuinely interrupt here. An MSAA live region cannot do that: its
//! politeness is one setting on the window, not something a single announcement can carry.
//!
//! Raising the event needs a real server-side provider on the window. The provider handed back
//! by `UiaHostProviderFromHwnd` is not one: raising on it returns `S_OK` and reaches no UIA
//! client at all, with no error anywhere. So the announcer window gets subclassed to answer
//! `WM_GETOBJECT` with a provider of our own, which defers to the host provider for everything
//! except being a valid event source.

// The code `#[implement]` expands to trips these; they are not about anything written here.
#![allow(clippy::ref_as_ptr, clippy::inline_always)]

use std::{cell::RefCell, collections::HashMap, ffi::c_void};

use windows::{
	Win32::{
		Foundation::{E_NOTIMPL, HWND, LPARAM, LRESULT, WPARAM},
		System::Variant::VARIANT,
		UI::{
			Accessibility::{
				IRawElementProviderSimple, IRawElementProviderSimple_Impl, NotificationKind_Other,
				NotificationProcessing, NotificationProcessing_All, NotificationProcessing_CurrentThenMostRecent,
				NotificationProcessing_ImportantMostRecent, ProviderOptions, ProviderOptions_ServerSideProvider,
				ProviderOptions_UseComThreading, UIA_PATTERN_ID, UIA_PROPERTY_ID, UiaClientsAreListening,
				UiaHostProviderFromHwnd, UiaRaiseNotificationEvent, UiaReturnRawElementProvider, UiaRootObjectId,
			},
			Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
			WindowsAndMessaging::{WM_GETOBJECT, WM_NCDESTROY},
		},
	},
	core::{BSTR, IUnknown, Result, implement},
};
use wxdragon::prelude::WxWidget;

use crate::Priority;

/// Only has to be unique among subclasses this crate installs on a given window.
const SUBCLASS_ID: usize = 0x6c76_7267;

thread_local! {
	/// One provider per announcer window. The subclass proc has to hand the same provider back
	/// on every `WM_GETOBJECT`, and UIA holds references to it across events, so these live for
	/// as long as the thread does.
	static PROVIDERS: RefCell<HashMap<isize, IRawElementProviderSimple>> = RefCell::new(HashMap::new());
}

/// The minimum a server-side provider can be. It identifies itself, supports no patterns, and
/// defers every property to the HWND host provider, so the announcer window keeps whatever
/// identity wxWidgets already gave it and only gains the ability to raise events.
#[implement(IRawElementProviderSimple)]
struct Announcer {
	hwnd: isize,
}

impl IRawElementProviderSimple_Impl for Announcer_Impl {
	fn ProviderOptions(&self) -> Result<ProviderOptions> {
		Ok(ProviderOptions(ProviderOptions_ServerSideProvider.0 | ProviderOptions_UseComThreading.0))
	}

	fn GetPatternProvider(&self, _pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
		// The convention is S_OK with a null out pointer, but `IUnknown` is a non-null pointer
		// so the generated shim cannot express that. Reporting the pattern as unimplemented is
		// the honest alternative, and an announcer supports no patterns anyway.
		Err(E_NOTIMPL.into())
	}

	fn GetPropertyValue(&self, _property: UIA_PROPERTY_ID) -> Result<VARIANT> {
		// VT_EMPTY makes UIA fall back to the host provider for every property.
		Ok(VARIANT::default())
	}

	fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
		unsafe { UiaHostProviderFromHwnd(hwnd_from_key(self.hwnd)) }
	}
}

/// How a screen reader should fit an announcement around whatever it is already saying.
///
/// The three levels map to genuinely different behaviour rather than collapsing into two, which
/// is why [`Priority::Medium`] uses `CurrentThenMostRecent` rather than `All`.
const fn processing(priority: Priority) -> NotificationProcessing {
	match priority {
		// Queue behind everything already pending, preserving order.
		Priority::Low => NotificationProcessing_All,
		// Let the current utterance finish, then speak this and drop anything staler.
		Priority::Medium => NotificationProcessing_CurrentThenMostRecent,
		// Cut off whatever is being spoken and say this instead.
		Priority::High => NotificationProcessing_ImportantMostRecent,
	}
}

const fn hwnd_from_key(key: isize) -> HWND {
	HWND(key as *mut c_void)
}

unsafe extern "system" fn subclass_proc(
	hwnd: HWND,
	msg: u32,
	wparam: WPARAM,
	lparam: LPARAM,
	_subclass_id: usize,
	_ref_data: usize,
) -> LRESULT {
	unsafe {
		if msg == WM_GETOBJECT
			&& lparam.0 == UiaRootObjectId as isize
			&& let Some(provider) = PROVIDERS.with(|cell| cell.borrow().get(&(hwnd.0 as isize)).cloned())
		{
			return UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider);
		}
		// Last message the window will see, so let go of the provider and the subclass rather
		// than keeping a dead window's entry alive for the life of the thread.
		if msg == WM_NCDESTROY {
			PROVIDERS.with(|cell| cell.borrow_mut().remove(&(hwnd.0 as isize)));
			let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
		}
		DefSubclassProc(hwnd, msg, wparam, lparam)
	}
}

/// Installs the provider and the subclass that serves it, once per window. Returns whether the
/// window can raise notifications afterwards.
fn ensure_provider(hwnd: HWND) -> bool {
	let key = hwnd.0 as isize;
	PROVIDERS.with(|cell| {
		let mut providers = cell.borrow_mut();
		if providers.contains_key(&key) {
			return true;
		}
		let provider: IRawElementProviderSimple = Announcer { hwnd: key }.into();
		// The provider has to be reachable from the subclass proc before the subclass is
		// installed, because installing it can itself draw a WM_GETOBJECT.
		providers.insert(key, provider);
		// SAFETY: wx creates and owns its widgets on the thread this runs on, which is the
		// thread the window's messages are dispatched on.
		if unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) }.as_bool() {
			true
		} else {
			providers.remove(&key);
			false
		}
	})
}

fn hwnd_from_widget(widget: &impl WxWidget) -> Option<HWND> {
	let handle = widget.get_handle();
	if handle.is_null() { None } else { Some(HWND(handle)) }
}

/// Prepares `window` to be used as an announcer. Announcing does this on demand too, so calling
/// this up front is only worth it to find out early whether announcements can work at all.
pub fn set_live_region(window: &impl WxWidget) -> bool {
	hwnd_from_widget(window).is_some_and(ensure_provider)
}

pub fn announce(window: &impl WxWidget, message: &str, priority: Priority) -> bool {
	let Some(hwnd) = hwnd_from_widget(window) else {
		return false;
	};
	if !ensure_provider(hwnd) {
		return false;
	}
	// Nothing is listening, so the raise would succeed and reach nobody. Report that honestly
	// rather than claiming the message was delivered.
	if !unsafe { UiaClientsAreListening() }.as_bool() {
		return false;
	}
	let Some(provider) = PROVIDERS.with(|cell| cell.borrow().get(&(hwnd.0 as isize)).cloned()) else {
		return false;
	};
	// `sanitize_message` has already stripped control characters, so this cannot contain a NUL.
	let display = BSTR::from(message);
	let activity = BSTR::new();
	unsafe {
		UiaRaiseNotificationEvent(&provider, NotificationKind_Other, processing(priority), &display, &activity).is_ok()
	}
}
