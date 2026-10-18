//! Configurable application shortcuts such as `ctrl+shift+p`.
use winit::keyboard::{Key, NamedKey};

use super::keys::Modifiers;
use crate::config;

/// One key chord: exact modifier set plus a character key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
	mods: Modifiers,
	key: String,
}

impl Shortcut {
	/// Parses `"ctrl+shift+p"`. Modifier names are `ctrl`, `shift`, `alt`, `super`.
	pub(crate) fn parse(binding: &str) -> Option<Self> {
		let binding = binding.to_ascii_lowercase();
		let mut parts: Vec<&str> = binding.split('+').map(str::trim).collect();
		let key = parts.pop().filter(|key| !key.is_empty())?.to_owned();
		let has = |name: &str| parts.contains(&name);
		Some(Self {
			mods: Modifiers {
				ctrl: has("ctrl"),
				shift: has("shift"),
				alt: has("alt"),
				super_key: has("super"),
			},
			key,
		})
	}

	pub(crate) fn matches(&self, key: &Key, mods: Modifiers) -> bool {
		mods == self.mods
			&& match key {
				Key::Character(value) => value.eq_ignore_ascii_case(&self.key),
				Key::Named(named) => named_key(&self.key) == Some(*named),
				_ => false,
			}
	}
}

/// Named keys accepted in bindings, e.g. `ctrl+shift+up`.
fn named_key(name: &str) -> Option<NamedKey> {
	Some(match name {
		"up" => NamedKey::ArrowUp,
		"down" => NamedKey::ArrowDown,
		"left" => NamedKey::ArrowLeft,
		"right" => NamedKey::ArrowRight,
		"pageup" => NamedKey::PageUp,
		"pagedown" => NamedKey::PageDown,
		"home" => NamedKey::Home,
		"end" => NamedKey::End,
		"tab" => NamedKey::Tab,
		"enter" => NamedKey::Enter,
		"space" => NamedKey::Space,
		_ => return None,
	})
}

/// Shortcut table, read from the config once instead of on every key press.
#[derive(Clone, Debug)]
pub(crate) struct Shortcuts {
	pub new_tab: Shortcut,
	pub command_palette: Shortcut,
	pub cycle_theme: Shortcut,
	pub search: Shortcut,
	pub close_tab: Shortcut,
	pub previous_prompt: Shortcut,
	pub next_prompt: Shortcut,
}

impl Shortcuts {
	/// Builds the table from config text, falling back to platform defaults.
	pub(crate) fn from_config(text: &str) -> Self {
		let mac = cfg!(target_os = "macos");
		let get = |key: &str, default: &str| {
			config::value(text, key)
				.and_then(|binding| Shortcut::parse(&binding))
				.or_else(|| Shortcut::parse(default))
				.expect("default shortcut is valid")
		};
		Self {
			new_tab: get(
				"shortcut_new_tab",
				if mac { "super+t" } else { "ctrl+shift+n" },
			),
			command_palette: get("shortcut_command_palette", "ctrl+shift+p"),
			cycle_theme: get("shortcut_cycle_theme", "ctrl+shift+t"),
			search: get(
				"shortcut_search",
				if mac { "super+f" } else { "ctrl+shift+f" },
			),
			close_tab: get(
				"shortcut_close_tab",
				if mac { "super+w" } else { "ctrl+shift+w" },
			),
			previous_prompt: get(
				"shortcut_previous_prompt",
				if mac { "super+up" } else { "ctrl+shift+up" },
			),
			next_prompt: get(
				"shortcut_next_prompt",
				if mac { "super+down" } else { "ctrl+shift+down" },
			),
		}
	}

	pub(crate) fn load() -> Self {
		Self::from_config(&std::fs::read_to_string(config::config_path()).unwrap_or_default())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn chr(value: &str) -> Key {
		Key::Character(value.into())
	}

	#[test]
	fn modifiers_must_match_exactly() {
		let shortcut = Shortcut::parse("ctrl+shift+p").unwrap();
		let ctrl_shift = Modifiers {
			ctrl: true,
			shift: true,
			..Default::default()
		};
		assert!(shortcut.matches(&chr("P"), ctrl_shift));
		assert!(!shortcut.matches(
			&chr("p"),
			Modifiers {
				ctrl: true,
				..Default::default()
			}
		));
		assert!(!shortcut.matches(&chr("o"), ctrl_shift));
	}

	#[test]
	fn config_overrides_defaults_and_invalid_bindings_fall_back() {
		let shortcuts =
			Shortcuts::from_config("shortcut_search = \"alt+s\"\nshortcut_new_tab = \"\"\n");
		let alt = Modifiers {
			alt: true,
			..Default::default()
		};
		assert!(shortcuts.search.matches(&chr("s"), alt));
		assert_eq!(
			shortcuts.new_tab,
			Shortcut::parse(if cfg!(target_os = "macos") {
				"super+t"
			} else {
				"ctrl+shift+n"
			})
			.unwrap()
		);
	}

	#[test]
	fn named_keys_can_be_bound() {
		use winit::keyboard::NamedKey;
		let up = Shortcut::parse("ctrl+shift+up").unwrap();
		let ctrl_shift = Modifiers {
			ctrl: true,
			shift: true,
			..Default::default()
		};
		assert!(up.matches(&Key::Named(NamedKey::ArrowUp), ctrl_shift));
		assert!(!up.matches(&Key::Named(NamedKey::ArrowDown), ctrl_shift));
		assert!(!up.matches(&chr("u"), ctrl_shift));
		let page = Shortcut::parse("shift+pagedown").unwrap();
		let shift = Modifiers {
			shift: true,
			..Default::default()
		};
		assert!(page.matches(&Key::Named(NamedKey::PageDown), shift));
	}
}
