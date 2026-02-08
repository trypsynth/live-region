fn main() {
	if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
		// wxWidgets (via wxdragon-sys) is compiled against Homebrew's GNU libiconv,
		// which uses libiconv-prefixed symbols. Tell the linker where to find it.
		if let Some(prefix) = homebrew_prefix() {
			let iconv_lib = format!("{prefix}/opt/libiconv/lib");
			if std::path::Path::new(&iconv_lib).exists() {
				println!("cargo:rustc-link-search=native={iconv_lib}");
			}
		}
	}
}

fn homebrew_prefix() -> Option<String> {
	let output = std::process::Command::new("brew").arg("--prefix").output().ok()?;
	if output.status.success() { Some(String::from_utf8_lossy(&output.stdout).trim().to_string()) } else { None }
}
