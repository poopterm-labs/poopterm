//! Reads the release codename from `Cargo.toml`. Shared by `build.rs`, and
//! compiled into the test build so its tests run with `cargo test`.

/// The `codename` in `[package.metadata.poopterm]`, if any.
pub(crate) fn package_codename(manifest: &str) -> Option<&str> {
	let metadata = manifest.split_once("[package.metadata.poopterm]")?.1;
	let section = metadata
		.split_once("\n[")
		.map_or(metadata, |(section, _)| section);
	section
		.lines()
		.find_map(|line| line.trim().strip_prefix("codename = \"")?.strip_suffix('"'))
}

#[cfg(test)]
mod tests {
	use super::package_codename;

	#[test]
	fn reads_the_codename_from_the_poopterm_metadata_section() {
		let manifest = "[package.metadata.poopterm]\ncodename = \"Giga Fly\"\n\n[dependencies]\n";
		assert_eq!(package_codename(manifest), Some("Giga Fly"));
	}

	#[test]
	fn does_not_read_a_codename_from_another_section() {
		let manifest =
			"[package.metadata.poopterm]\nname = \"poopterm\"\n\n[other]\ncodename = \"wrong\"\n";
		assert_eq!(package_codename(manifest), None);
	}

	#[test]
	fn the_manifest_names_a_codename() {
		assert!(package_codename(include_str!("../Cargo.toml")).is_some());
	}
}
