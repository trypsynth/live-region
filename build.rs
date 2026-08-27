fn main() {
	match std::env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
		"macos" => link_macos_iconv(),
		"linux" => link_linux_gtk(),
		_ => {}
	}
}

// wxWidgets (via wxdragon-sys) is compiled against Homebrew's GNU libiconv,
// which uses libiconv-prefixed symbols. Tell the linker where to find it.
fn link_macos_iconv() {
	if let Some(prefix) = homebrew_prefix() {
		let iconv_lib = format!("{prefix}/opt/libiconv/lib");
		if std::path::Path::new(&iconv_lib).exists() {
			println!("cargo:rustc-link-search=native={iconv_lib}");
		}
	}
}

// The Linux backend calls GTK, GObject and ATK directly. wxdragon-sys already links all
// three, but probe explicitly so we don't silently depend on another crate's build script.
fn link_linux_gtk() {
	if let Err(error) = pkg_config::Config::new().probe("gtk+-3.0") {
		println!("cargo:warning=Could not probe gtk+-3.0 ({error}); relying on wxdragon-sys link flags.");
	}
}

fn homebrew_prefix() -> Option<String> {
	let output = std::process::Command::new("brew").arg("--prefix").output().ok()?;
	if output.status.success() { Some(String::from_utf8_lossy(&output.stdout).trim().to_string()) } else { None }
}
