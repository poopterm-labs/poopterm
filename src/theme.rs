//! Built-in color themes and the xterm 256-color palette.
use std::env;

use crate::config::{read_config_theme, read_config_value};

// ---- Built-in themes --------------------------------------------------------

pub(crate) const DEFAULT_FG: u32 = 0xE8D5B0; // cream
pub(crate) const DEFAULT_BG: u32 = 0x24170B; // dark chocolate
pub(crate) const BORDER_COL: u32 = 0x7A4E1F; // caramel rim
pub(crate) const PALETTE: [u32; 8] = [
	0x1E1409, // black
	0xD9534F, // red
	0x8BC34A, // green
	0xE0B040, // yellow
	0x5C9BD6, // blue
	0xB06AC8, // magenta
	0x4FB6B0, // cyan
	0xE8D5B0, // white
];

#[derive(Clone, Copy)]
pub(crate) struct Theme {
	pub(crate) kind: ThemeKind,
	pub(crate) fg: u32,
	pub(crate) bg: u32,
	pub(crate) border: u32,
	pub(crate) palette: [u32; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThemeKind {
	MacOsDark,
	MacOsLight,
	CatppuccinLatte,
	CatppuccinFrappe,
	CatppuccinMacchiato,
	CatppuccinMocha,
	Poop,
	Custom,
}

impl ThemeKind {
	pub(crate) fn parse(value: &str) -> Option<Self> {
		match value.trim().trim_matches('"').to_ascii_lowercase().as_str() {
			"darkmode" | "macos" | "mac" | "macos-dark" => Some(Self::MacOsDark),
			"light-mode" | "light mode" | "macos-light" | "light" => Some(Self::MacOsLight),
			"catppuccin" | "catppuccin-mocha" => Some(Self::CatppuccinMocha),
			"catppuccin-latte" => Some(Self::CatppuccinLatte),
			"catppuccin-frappe" => Some(Self::CatppuccinFrappe),
			"catppuccin-macchiato" => Some(Self::CatppuccinMacchiato),
			"poop" => Some(Self::Poop),
			"custom" => Some(Self::Custom),
			_ => None,
		}
	}
	pub(crate) fn name(self) -> &'static str {
		match self {
			Self::MacOsDark => "darkmode",
			Self::MacOsLight => "light-mode",
			Self::CatppuccinLatte => "catppuccin-latte",
			Self::CatppuccinFrappe => "catppuccin-frappe",
			Self::CatppuccinMacchiato => "catppuccin-macchiato",
			Self::CatppuccinMocha => "catppuccin-mocha",
			Self::Poop => "poop",
			Self::Custom => "custom",
		}
	}
	pub(crate) fn next(self) -> Self {
		match self {
			Self::MacOsDark => Self::MacOsLight,
			Self::MacOsLight => Self::CatppuccinLatte,
			Self::CatppuccinLatte => Self::CatppuccinFrappe,
			Self::CatppuccinFrappe => Self::CatppuccinMacchiato,
			Self::CatppuccinMacchiato => Self::CatppuccinMocha,
			Self::CatppuccinMocha => Self::Poop,
			Self::Poop => Self::MacOsDark,
			Self::Custom => Self::MacOsDark,
		}
	}
}

impl Theme {
	pub(crate) fn from_kind(kind: ThemeKind) -> Self {
		match kind {
			ThemeKind::MacOsDark => Self {
				kind,
				fg: 0xE6E6E6,
				bg: 0x1E1E1E,
				border: 0x3A3A3C,
				palette: [
					0x000000, 0xC23621, 0x25BC26, 0xADAD27, 0x492EE1, 0xD338D3, 0x33BBC8, 0xBBBBBB,
				],
			},
			ThemeKind::MacOsLight => Self {
				kind,
				fg: 0x1D1D1F,
				bg: 0xF5F5F7,
				border: 0xD2D2D7,
				palette: [
					0x6E6E73, 0xC41E3A, 0x248A3D, 0x9A6700, 0x0066CC, 0x8944AB, 0x007A82, 0x1D1D1F,
				],
			},
			ThemeKind::CatppuccinLatte => Self {
				kind,
				fg: 0x4C4F69,
				bg: 0xEFF1F5,
				border: 0xCCD0DA,
				palette: [
					0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xACB0BE,
				],
			},
			ThemeKind::CatppuccinFrappe => Self {
				kind,
				fg: 0xC6D0F5,
				bg: 0x303446,
				border: 0x414559,
				palette: [
					0x51576D, 0xE78284, 0xA6D189, 0xE5C890, 0x8CAAEE, 0xCA9EE6, 0x81C8BE, 0xB5BFE2,
				],
			},
			ThemeKind::CatppuccinMacchiato => Self {
				kind,
				fg: 0xCAD3F5,
				bg: 0x24273A,
				border: 0x363A4F,
				palette: [
					0x494D64, 0xED8796, 0xA6DA95, 0xEED49F, 0x8AADF4, 0xF5BDE6, 0x8BD5CA, 0xB8C0E0,
				],
			},
			ThemeKind::CatppuccinMocha => Self {
				kind,
				fg: 0xCDD6F4,
				bg: 0x1E1E2E,
				border: 0x45475A,
				palette: [
					0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xBAC2DE,
				],
			},
			ThemeKind::Poop => Self {
				kind,
				fg: DEFAULT_FG,
				bg: DEFAULT_BG,
				border: BORDER_COL,
				palette: PALETTE,
			},
			ThemeKind::Custom => {
				let mut theme = Self::from_kind(ThemeKind::Poop);
				theme.kind = ThemeKind::Custom;
				theme
			}
		}
	}

	pub(crate) fn load() -> Self {
		let kind = env::var("POOPTERM_THEME")
			.ok()
			.and_then(|s| ThemeKind::parse(&s))
			.or_else(read_config_theme)
			.unwrap_or(ThemeKind::Poop);
		let mut theme = Self::from_kind(kind);
		for (key, target) in [
			("foreground", &mut theme.fg),
			("background", &mut theme.bg),
			("border", &mut theme.border),
		] {
			if let Some(color) = read_config_value(key).and_then(|value| parse_color(&value)) {
				*target = color;
			}
		}
		for (index, color) in theme.palette.iter_mut().enumerate() {
			if let Some(value) =
				read_config_value(&format!("color{index}")).and_then(|value| parse_color(&value))
			{
				*color = value;
			}
		}
		theme
	}
}

pub(crate) fn parse_color(value: &str) -> Option<u32> {
	let value = value.trim().trim_matches('"');
	let value = value
		.strip_prefix('#')
		.or_else(|| value.strip_prefix("0x"))?;
	(value.len() == 6)
		.then(|| u32::from_str_radix(value, 16).ok())
		.flatten()
}

pub(crate) fn bright_color(index: usize) -> u32 {
	[
		0x808080, 0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
	][index]
}

pub(crate) fn xterm_color(index: u8, palette: [u32; 8]) -> u32 {
	if index < 8 {
		return palette[index as usize];
	}
	if index < 16 {
		return bright_color((index - 8) as usize);
	}
	if index < 232 {
		let n = index - 16;
		let component = |v: u8| if v == 0 { 0 } else { 55 + 40 * v as u32 };
		return (component(n / 36) << 16) | (component((n / 6) % 6) << 8) | component(n % 6);
	}
	let gray = 8 + 10 * (index as u32 - 232);
	(gray << 16) | (gray << 8) | gray
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn theme_labels_and_catppuccin_palette_match_expected_values() {
		assert_eq!(ThemeKind::MacOsDark.name(), "darkmode");
		assert_eq!(ThemeKind::MacOsLight.name(), "light-mode");
		assert_eq!(ThemeKind::parse("macos-dark"), Some(ThemeKind::MacOsDark));
		assert_eq!(ThemeKind::parse("macos-light"), Some(ThemeKind::MacOsLight));

		let flavors = [
			(
				ThemeKind::CatppuccinLatte,
				0x4C4F69,
				0xEFF1F5,
				0xCCD0DA,
				[
					0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xACB0BE,
				],
			),
			(
				ThemeKind::CatppuccinFrappe,
				0xC6D0F5,
				0x303446,
				0x414559,
				[
					0x51576D, 0xE78284, 0xA6D189, 0xE5C890, 0x8CAAEE, 0xCA9EE6, 0x81C8BE, 0xB5BFE2,
				],
			),
			(
				ThemeKind::CatppuccinMacchiato,
				0xCAD3F5,
				0x24273A,
				0x363A4F,
				[
					0x494D64, 0xED8796, 0xA6DA95, 0xEED49F, 0x8AADF4, 0xF5BDE6, 0x8BD5CA, 0xB8C0E0,
				],
			),
			(
				ThemeKind::CatppuccinMocha,
				0xCDD6F4,
				0x1E1E2E,
				0x45475A,
				[
					0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xBAC2DE,
				],
			),
		];
		for (kind, fg, bg, border, palette) in flavors {
			let theme = Theme::from_kind(kind);
			assert_eq!(
				(theme.fg, theme.bg, theme.border, theme.palette),
				(fg, bg, border, palette)
			);
		}
	}
}
