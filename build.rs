//! Pass the release codename from Cargo metadata into the application.
use std::fs;

fn package_codename(manifest: &str) -> Option<&str> {
	let metadata = manifest.split_once("[package.metadata.poopterm]")?.1;
	let section = metadata
		.split_once("\n[")
		.map_or(metadata, |(section, _)| section);
	section
		.lines()
		.find_map(|line| line.trim().strip_prefix("codename = \"")?.strip_suffix('"'))
}

fn main() {
	println!("cargo:rerun-if-changed=Cargo.toml");
	let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
	let codename = package_codename(&manifest).expect("package metadata must define a codename");
	println!("cargo:rustc-env=POOPTERM_CODENAME={codename}");
}

#[cfg(test)]
mod tests {
	use super::package_codename;

	#[test]
	fn reads_the_codename_from_the_pooptterm_metadata_section() {
		let manifest = "[package.metadata.poopterm]\ncodename = \"Giga Fly\"\n\n[dependencies]\n";
		assert_eq!(package_codename(manifest), Some("Giga Fly"));
	}

	#[test]
	fn does_not_read_a_codename_from_another_section() {
		let manifest =
			"[package.metadata.poopterm]\nname = \"poopterm\"\n\n[other]\ncodename = \"wrong\"\n";
		assert_eq!(package_codename(manifest), None);
	}
}
