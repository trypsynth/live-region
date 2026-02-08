use live_region::{announce, set_live_region};
use wxdragon::prelude::*;

fn main() {
	wxdragon::main(|_app| {
		let frame = Frame::builder().with_title("Live Region Test").with_size(Size { width: 300, height: 200 }).build();

		let panel = Panel::builder(&frame).build();
		let sizer = BoxSizer::builder(Orientation::Vertical).build();

		let button = Button::builder(&panel).with_label("Speak").build();

		let label = StaticText::builder(&panel).with_label("").build();

		set_live_region(&label);

		sizer.add(&button, 0, SizerFlag::All, 10);
		sizer.add(&label, 0, SizerFlag::All, 10);
		panel.set_sizer(sizer, true);

		button.on_click(move |_| {
			announce(label, "Hello from the live region!");
		});

		frame.show(true);
	})
	.unwrap();
}
