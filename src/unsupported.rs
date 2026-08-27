//! Fallback backend for platforms with no screen reader integration. Every call is a no-op.

// These stubs are kept signature-compatible with the other backends. Marking them `const` only
// pushes `missing_const_for_fn` up into the public wrappers in `lib.rs`, which cannot be `const`
// on the platforms whose backends call into FFI.
#![allow(clippy::missing_const_for_fn)]

use wxdragon::prelude::WxWidget;

use crate::Priority;

pub fn set_live_region(_window: &impl WxWidget) -> bool {
	false
}

pub fn announce(_window: &impl WxWidget, _message: &str, _priority: Priority) -> bool {
	false
}
