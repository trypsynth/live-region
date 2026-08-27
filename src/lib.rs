#![warn(clippy::all, clippy::cargo, clippy::nursery, clippy::pedantic, clippy::perf, unused_crate_dependencies)]
#![deny(warnings)]

use wxdragon::prelude::{StaticText, WxWidget};

#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(not(any(target_os = "windows", target_os = "macos", target_os = "linux")), path = "unsupported.rs")]
mod platform_impl;

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
