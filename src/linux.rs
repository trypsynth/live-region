//! Linux backend: Orca's `PresentMessage` D-Bus method, invoked through `gdbus`.

use std::{process::Command, sync::OnceLock, thread};

use wxdragon::prelude::WxWidget;

use crate::Priority;

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
