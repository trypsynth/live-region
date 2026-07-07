//! Example for manually testing how announcement priorities interact on
//! macOS VoiceOver.
//!
//! Type one directive per line in the text box, then press Speak:
//!   h:<text>   announce <text> at Priority::High
//!   m:<text>   announce <text> at Priority::Medium
//!   l:<text>   announce <text> at Priority::Low
//!   w:<secs>   wait <secs> seconds before processing the next line (e.g. `w:0.1`)
//!
//! Each post is logged to stderr with a monotonic timestamp, the posted priority, and the
//! message, so you can correlate what you hear with exactly what was posted and when. Lines that
//! don't parse (including a malformed `w:`) are logged as skipped — if a wait silently drops, the
//! two neighboring announcements post back-to-back with no gap, which would otherwise look like a
//! VoiceOver quirk. The wait is scheduled on a one-shot timer rather than a blocking sleep so the
//! wx run loop keeps spinning and VoiceOver actually speaks between steps.

use std::{cell::RefCell, rc::Rc, time::Instant};

use live_region::{Priority, announce_with_priority, set_live_region};
use wxdragon::{prelude::*, timer::Timer};

#[derive(Clone)]
enum Step {
	Speak(Priority, String),
	/// Milliseconds to wait before the next step.
	Wait(i32),
}

struct Playback {
	steps: Vec<Step>,
	index: usize,
	/// Count of announcements posted in the current run, for labeling the log.
	posted: usize,
}

fn parse(text: &str) -> Vec<Step> {
	let mut steps = Vec::new();
	for (lineno, raw) in text.lines().enumerate() {
		let line = raw.trim();
		if line.is_empty() {
			continue;
		}
		let Some((tag, rest)) = line.split_once(':') else {
			eprintln!("line {}: skipped (no ':'): {raw:?}", lineno + 1);
			continue;
		};
		let rest = rest.trim();
		match tag.trim() {
			"h" => steps.push(Step::Speak(Priority::High, rest.to_string())),
			"m" => steps.push(Step::Speak(Priority::Medium, rest.to_string())),
			"l" => steps.push(Step::Speak(Priority::Low, rest.to_string())),
			"w" => match rest.parse::<f64>() {
				Ok(secs) => steps.push(Step::Wait((secs * 1000.0) as i32)),
				Err(_) => eprintln!("line {}: skipped (bad wait): {raw:?}", lineno + 1),
			},
			other => eprintln!("line {}: skipped (unknown tag {other:?}): {raw:?}", lineno + 1),
		}
	}
	steps
}

/// Speak steps back-to-back until a `Wait` is hit, then arm the one-shot timer to resume later.
fn run_steps(playback: &Rc<RefCell<Playback>>, timer: &Rc<Timer<Frame>>, label: StaticText, start: Instant) {
	loop {
		let (step, n) = {
			let mut pb = playback.borrow_mut();
			if pb.index >= pb.steps.len() {
				return;
			}
			let step = pb.steps[pb.index].clone();
			pb.index += 1;
			if matches!(step, Step::Speak(..)) {
				pb.posted += 1;
			}
			(step, pb.posted)
		};
		let t = start.elapsed().as_secs_f64();
		match step {
			Step::Speak(priority, message) => {
				eprintln!("[+{t:7.3}s] post #{n} {priority:?} {message:?}");
				announce_with_priority(label, &message, priority);
			}
			Step::Wait(ms) => {
				eprintln!("[+{t:7.3}s] wait {ms}ms");
				timer.start(ms, true);
				return;
			}
		}
	}
}

fn main() {
	wxdragon::main(|_app| {
		let start = Instant::now();

		let frame = Frame::builder()
			.with_title("live-region priority scratch")
			.with_size(Size { width: 480, height: 360 })
			.build();

		let panel = Panel::builder(&frame).build();
		let sizer = BoxSizer::builder(Orientation::Vertical).build();

		let input = TextCtrl::builder(&panel)
			.with_style(TextCtrlStyle::MultiLine)
			.with_value(
				"m:medium one this is a deliberately long sentence\nw:0.1\nl:low two also a deliberately long sentence\nw:0.1\nh:high three the final long sentence",
			)
			.with_size(Size { width: 440, height: 220 })
			.build();

		let button = Button::builder(&panel).with_label("Speak").build();
		let label = StaticText::builder(&panel).with_label("").build();
		set_live_region(&label);

		let timer = Rc::new(Timer::new(&frame));
		let playback = Rc::new(RefCell::new(Playback { steps: Vec::new(), index: 0, posted: 0 }));

		let timer_for_tick = timer.clone();
		let playback_for_tick = playback.clone();
		timer.on_tick(move |_| run_steps(&playback_for_tick, &timer_for_tick, label, start));

		let timer_for_click = timer.clone();
		let playback_for_click = playback.clone();
		button.on_click(move |_| {
			// Cancel any sequence still in flight so a stale timer tick can't interleave with the
			// fresh run and post out of order.
			timer_for_click.stop();
			eprintln!("--- Speak ---");
			{
				let mut pb = playback_for_click.borrow_mut();
				pb.steps = parse(&input.get_value());
				pb.index = 0;
				pb.posted = 0;
			}
			run_steps(&playback_for_click, &timer_for_click, label, start);
		});

		sizer.add(&input, 1, SizerFlag::Expand | SizerFlag::All, 10);
		sizer.add(&button, 0, SizerFlag::All, 10);
		sizer.add(&label, 0, SizerFlag::All, 10);
		panel.set_sizer(sizer, true);

		frame.show(true);
	})
	.unwrap();
}
