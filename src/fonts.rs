//! Terminal font loading with bundled and system symbol-font fallbacks.
use std::{env, fs, path::PathBuf};

use fontdue::{Font, FontSettings};

use crate::{config::read_config_value, session_store::default_cwd};

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

	let mut roots = vec![
		PathBuf::from("/usr/share/fonts"),
		PathBuf::from("/usr/local/share/fonts"),
		PathBuf::from("/Library/Fonts"),
		PathBuf::from("/System/Library/Fonts"),
	];
	if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
		roots.extend([
			home.join(".local/share/fonts"),
			home.join(".fonts"),
			home.join("Library/Fonts"),
		]);
	}
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
