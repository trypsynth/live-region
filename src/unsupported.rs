//! Fallback backend for platforms with no screen reader integration. Every call is a no-op.

use wxdragon::prelude::WxWidget;

use crate::Priority;

pub fn set_live_region(_window: &impl WxWidget) -> bool {
	false
}

pub fn announce(_window: &impl WxWidget, _message: &str, _priority: Priority) -> bool {
	false
}
