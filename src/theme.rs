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

/// Selected text's foreground and background, unless a program sets them.
pub(crate) const SELECTION_COLORS: (u32, u32) = (0x1E1E2E, 0x89B4FA);

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
	Bark,
	AmoledBlack,
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
			"bark" => Some(Self::Bark),
			"amoled-black" | "amoled" => Some(Self::AmoledBlack),
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
			Self::Bark => "bark",
			Self::AmoledBlack => "amoled-black",
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
			Self::CatppuccinMocha => Self::Bark,
			Self::Bark => Self::AmoledBlack,
			Self::AmoledBlack => Self::Poop,
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
			ThemeKind::Bark => Self {
				kind,
				fg: 0xD0D0D0,
				bg: 0x101010,
				border: 0x2B2B2B,
				palette: [
					0x101010, 0xAB4642, 0xA1B56C, 0xF7CA88, 0x7CAFC2, 0xBA8BAF, 0x86C1B9, 0xD0D0D0,
				],
			},
			ThemeKind::AmoledBlack => Self {
				kind,
				fg: 0xE9E4D4,
				bg: 0x000000,
				border: 0x1B1B1B,
				palette: [
					0x121212, 0xFF5C57, 0x8CE88A, 0xF2C14E, 0xD4AF37, 0xC792EA, 0x6EDBD2, 0xE9E4D4,
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
		if kind == ThemeKind::AmoledBlack {
			return theme;
		}
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

	fn relative_luminance(color: u32) -> f64 {
		let linear_channel = |shift: u32| {
			let srgb = ((color >> shift) & 0xFF) as f64 / 255.0;
			if srgb <= 0.04045 {
				srgb / 12.92
			} else {
				((srgb + 0.055) / 1.055).powf(2.4)
			}
		};

		0.2126 * linear_channel(16) + 0.7152 * linear_channel(8) + 0.0722 * linear_channel(0)
	}

	fn contrast_ratio(foreground: u32, background: u32) -> f64 {
		let foreground = relative_luminance(foreground);
		let background = relative_luminance(background);
		(foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
	}

	#[test]
	fn built_in_theme_foreground_background_meet_wcag_aa() {
		let kinds = [
			ThemeKind::MacOsDark,
			ThemeKind::MacOsLight,
			ThemeKind::CatppuccinLatte,
			ThemeKind::CatppuccinFrappe,
			ThemeKind::CatppuccinMacchiato,
			ThemeKind::CatppuccinMocha,
			ThemeKind::Bark,
			ThemeKind::AmoledBlack,
			ThemeKind::Poop,
		];

		for kind in kinds {
			let theme = Theme::from_kind(kind);
			let ratio = contrast_ratio(theme.fg, theme.bg);
			assert!(
				ratio >= 4.5,
				"{} contrast is {ratio:.2}:1, below WCAG AA's 4.5:1",
				theme.kind.name()
			);
		}
	}

	#[test]
	fn contrast_ratio_uses_wcag_relative_luminance() {
		assert!((contrast_ratio(0x000000, 0xFFFFFF) - 21.0).abs() < f64::EPSILON);
		assert!(contrast_ratio(0x767676, 0xFFFFFF) >= 4.5);
		assert!(contrast_ratio(0x777777, 0xFFFFFF) < 4.5);
		assert!((contrast_ratio(0xFFFFFF, 0x000000) - 21.0).abs() < f64::EPSILON);
	}

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

	#[test]
	fn bark_theme_matches_its_terminal_colors() {
		assert_eq!(ThemeKind::parse("Bark"), Some(ThemeKind::Bark));
		assert_eq!(ThemeKind::Bark.name(), "bark");
		assert_eq!(ThemeKind::CatppuccinMocha.next(), ThemeKind::Bark);
		assert_eq!(ThemeKind::Bark.next(), ThemeKind::AmoledBlack);
		let theme = Theme::from_kind(ThemeKind::Bark);
		assert_eq!(theme.kind.name(), "bark");
		assert_eq!(
			(theme.fg, theme.bg, theme.border),
			(0xD0D0D0, 0x101010, 0x2B2B2B)
		);
		assert_eq!(
			theme.palette,
			[
				0x101010, 0xAB4642, 0xA1B56C, 0xF7CA88, 0x7CAFC2, 0xBA8BAF, 0x86C1B9, 0xD0D0D0,
			]
		);
	}
}
