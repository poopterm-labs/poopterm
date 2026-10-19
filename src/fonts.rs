//! Terminal font loading with bundled and system symbol-font fallbacks.
use std::{env, ffi::OsString, fs, path::PathBuf};

use fontdue::{Font, FontSettings};

use crate::{
	config::{read_config_value, xdg_base},
	session_store::default_cwd,
};

pub(crate) fn load_fonts() -> (Font, Vec<Font>) {
	let bundled = Font::from_bytes(
		include_bytes!("../assets/font.ttf") as &[u8],
		FontSettings::default(),
	)
	.expect("bundled font is valid");
	let configured = env::var("POOPTERM_FONT")
		.ok()
		.or_else(|| read_config_value("font_path"));
	let (font, mut fallback_fonts) = if let Some(path) = configured {
		let path = path
			.strip_prefix("~/")
			.map_or(PathBuf::from(&path), |tail| default_cwd().join(tail));
		match fs::read(&path)
			.map_err(|error| error.to_string())
			.and_then(|bytes| {
				Font::from_bytes(bytes, FontSettings::default()).map_err(|error| error.to_string())
			}) {
			Ok(font) => (font, vec![bundled]),
			Err(error) => {
				eprintln!(
					"poopterm: couldn't load font {}: {error}; using bundled font",
					path.display()
				);
				(bundled, Vec::new())
			}
		}
	} else {
		(bundled, Vec::new())
	};

	for path in installed_symbol_font_paths().into_iter().take(8) {
		let Ok(bytes) = fs::read(&path) else {
			continue;
		};
		if bytes.len() > 64 * 1024 * 1024 {
			continue;
		}
		if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) {
			fallback_fonts.push(font);
		}
	}
	(font, fallback_fonts)
}

pub(crate) fn installed_symbol_font_paths() -> Vec<PathBuf> {
	pub(crate) fn scan(path: &PathBuf, depth: usize, budget: &mut usize, out: &mut Vec<PathBuf>) {
		if depth > 5 || *budget == 0 {
			return;
		}
		let Ok(entries) = fs::read_dir(path) else {
			return;
		};
		for entry in entries.flatten() {
			if *budget == 0 {
				break;
			}
			*budget -= 1;
			let path = entry.path();
			if path.is_dir() {
				scan(&path, depth + 1, budget, out);
				continue;
			}
			let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
				continue;
			};
			let filename = filename.to_ascii_lowercase();
			let symbol_font =
				filename.contains("nerd") || filename.contains("symbol") || filename.contains("awesome");
			if symbol_font
				&& path
					.extension()
					.is_some_and(|ext| ext.eq_ignore_ascii_case("ttf"))
			{
				out.push(path);
			}
		}
	}

	let roots = font_roots(
		env::var_os("HOME"),
		env::var_os("XDG_DATA_HOME"),
		env::var_os("XDG_DATA_DIRS"),
	);
	let mut out = Vec::new();
	let mut budget = 2048;
	for root in roots {
		scan(&root, 0, &mut budget, &mut out);
		if budget == 0 {
			break;
		}
	}
	out.sort();
	out.dedup();
	out
}

/// Font directories: `fonts` in each XDG data directory, then the macOS and
/// legacy (`~/.fonts`) locations.
fn font_roots(
	home: Option<OsString>,
	data_home: Option<OsString>,
	data_dirs: Option<OsString>,
) -> Vec<PathBuf> {
	let home = home.map(PathBuf::from);
	let data_dirs = data_dirs.filter(|dirs| !dirs.is_empty());
	let mut roots: Vec<PathBuf> = env::split_paths(
		data_dirs
			.as_deref()
			.unwrap_or("/usr/local/share:/usr/share".as_ref()),
	)
	.chain(xdg_base(data_home, home.as_deref(), ".local/share"))
	.filter(|dir| dir.is_absolute())
	.map(|dir| dir.join("fonts"))
	.collect();
	roots.extend(["/Library/Fonts", "/System/Library/Fonts"].map(PathBuf::from));
	if let Some(home) = home {
		roots.extend([home.join(".fonts"), home.join("Library/Fonts")]);
	}
	let mut seen = std::collections::HashSet::new();
	roots.retain(|dir| seen.insert(dir.clone()));
	roots
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn font_directories_follow_xdg_data_variables() {
		let roots = font_roots(
			Some("/h".into()),
			Some("/data".into()),
			Some("/a:relative::/usr/share".into()),
		);
		for dir in ["/a/fonts", "/usr/share/fonts", "/data/fonts", "/h/.fonts"] {
			assert!(roots.contains(&PathBuf::from(dir)), "{dir} in {roots:?}");
		}
		assert!(!roots.iter().any(|dir| dir.is_relative()), "{roots:?}");
		assert_eq!(
			roots.len(),
			roots
				.iter()
				.collect::<std::collections::BTreeSet<_>>()
				.len()
		);
		let defaults = font_roots(Some("/h".into()), None, None);
		for dir in [
			"/usr/share/fonts",
			"/usr/local/share/fonts",
			"/h/.local/share/fonts",
		] {
			assert!(
				defaults.contains(&PathBuf::from(dir)),
				"{dir} in {defaults:?}"
			);
		}
	}
}
