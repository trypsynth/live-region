fn main() {
	if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "linux" {
		link_linux_gtk();
	}
}

// The Linux backend calls GTK, GObject and ATK directly. wxdragon-sys already links all
// three, but probe explicitly so we don't silently depend on another crate's build script.
fn link_linux_gtk() {
	if let Err(error) = pkg_config::Config::new().probe("gtk+-3.0") {
		println!("cargo:warning=Could not probe gtk+-3.0 ({error}); relying on wxdragon-sys link flags.");
	}
}
