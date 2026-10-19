//! Hit-testing for the Settings window: which control a click lands on.
use super::settings_view::*;
use crate::input::shortcuts::ShortcutAction;

/// The page the Settings window shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SettingsPage {
	#[default]
	Main,
	Layout,
	Shortcuts,
	About,
}

/// A clickable control in the Settings window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsTarget {
	/// Opens a page from the main page's header.
	Open(SettingsPage),
	/// The "Settings" button on the other pages.
	Back,
	Save,
	ChooseWallpaper,
	ClearWallpaper,
	ToggleRestore,
	ToggleTabs,
	ToggleVerticalTabs,
	ToggleNewTabButton,
	ToggleSettingsButton,
	ToggleBottomBar,
	SetShortcut(ShortcutAction),
	/// A stepper button: the row constant and -1 or +1.
	Step(usize, isize),
	/// A theme arrow: -1 or +1.
	Theme(isize),
}

/// Header buttons of the main page: page, distance of the left edge from
/// the window's right edge, and width, in logical pixels.
pub(super) const HEADER_BUTTONS: [(SettingsPage, usize, usize, &str); 3] = [
	(SettingsPage::Layout, 412, 124, "Layout"),
	(SettingsPage::Shortcuts, 280, 124, "Shortcuts"),
	(SettingsPage::About, 140, 116, "About"),
];

/// Toggles on the Layout page.
pub(super) const LAYOUT_TOGGLES: [(usize, SettingsTarget); 5] = [
	(SHOW_TABS_ROW, SettingsTarget::ToggleTabs),
	(VERTICAL_TABS_ROW, SettingsTarget::ToggleVerticalTabs),
	(NEW_TAB_BUTTON_ROW, SettingsTarget::ToggleNewTabButton),
	(SETTINGS_BUTTON_ROW, SettingsTarget::ToggleSettingsButton),
	(BOTTOM_BAR_ROW, SettingsTarget::ToggleBottomBar),
];

/// Maps a click at physical `(x, y)` to the control under it.
pub(super) fn settings_target(
	(x, y): (usize, usize),
	(width, height): (usize, usize),
	scale: usize,
	page: SettingsPage,
) -> Option<SettingsTarget> {
	let s = scale;
	let in_row = |row: usize| (row * s..(row + ROW_HEIGHT) * s).contains(&y);
	let in_header = (20 * s..56 * s).contains(&y);
	let right = |offset: usize| width.saturating_sub(offset * s);
	let (save_x, save_y) = (right(140), height.saturating_sub(52 * s));
	let on_save = (save_x..save_x + 120 * s).contains(&x) && (save_y..save_y + 36 * s).contains(&y);
	if page != SettingsPage::Main {
		if in_header && (right(164)..right(24)).contains(&x) {
			return Some(SettingsTarget::Back);
		}
		let rows: &[(usize, SettingsTarget)] = match page {
			SettingsPage::Layout => &LAYOUT_TOGGLES,
			SettingsPage::Shortcuts => &SHORTCUT_TARGETS,
			_ => return None,
		};
		if let Some(&(_, target)) = rows
			.iter()
			.find(|(row, _)| in_row(*row) && (right(240)..right(24)).contains(&x))
		{
			return Some(target);
		}
		return on_save.then_some(SettingsTarget::Save);
	}
	if in_header {
		return HEADER_BUTTONS
			.iter()
			.find(|(_, offset, w, _)| (right(*offset)..right(*offset) + w * s).contains(&x))
			.map(|&(page, ..)| SettingsTarget::Open(page));
	}
	if on_save {
		return Some(SettingsTarget::Save);
	}
	if (WALLPAPER_ROW * s..(WALLPAPER_ROW + 32) * s).contains(&y) {
		if (24 * s..176 * s).contains(&x) {
			return Some(SettingsTarget::ChooseWallpaper);
		}
		if (188 * s..284 * s).contains(&x) {
			return Some(SettingsTarget::ClearWallpaper);
		}
	}
	if in_row(RESTORE_ROW) && (right(240)..right(24)).contains(&x) {
		return Some(SettingsTarget::ToggleRestore);
	}
	let step = if (right(240)..right(192)).contains(&x) {
		-1
	} else if (right(72)..right(24)).contains(&x) {
		1
	} else {
		0
	};
	if step != 0 {
		let mut rows = vec![FONT_ROW, SCROLLBACK_ROW, STRENGTH_ROW];
		if cfg!(target_os = "macos") {
			rows.push(OPACITY_ROW);
		}
		if let Some(row) = rows.into_iter().find(|&row| in_row(row)) {
			return Some(SettingsTarget::Step(row, step));
		}
	}
	if in_row(THEME_ROW) {
		let (content_x, button_w) = (24 * s, 48 * s);
		let content_end = content_x + width.saturating_sub(48 * s);
		if x < content_x + button_w {
			return Some(SettingsTarget::Theme(-1));
		}
		if x >= content_end - button_w {
			return Some(SettingsTarget::Theme(1));
		}
	}
	None
}

/// [`SHORTCUT_ROWS`] as click targets.
const SHORTCUT_TARGETS: [(usize, SettingsTarget); 7] = {
	let mut targets = [(0, SettingsTarget::Save); 7];
	let mut i = 0;
	while i < SHORTCUT_ROWS.len() {
		targets[i] = (
			SHORTCUT_ROWS[i].1,
			SettingsTarget::SetShortcut(SHORTCUT_ROWS[i].0),
		);
		i += 1;
	}
	targets
};

#[cfg(test)]
mod tests {
	use super::*;

	const SIZE: (usize, usize) = (760, 640);

	#[test]
	fn clicks_map_to_main_page_controls() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Main);
		assert_eq!(
			target(630, 30),
			Some(SettingsTarget::Open(SettingsPage::About))
		);
		assert_eq!(
			target(550, 30),
			Some(SettingsTarget::Open(SettingsPage::Shortcuts))
		);
		assert_eq!(
			target(400, 30),
			Some(SettingsTarget::Open(SettingsPage::Layout))
		);
		assert_eq!(target(630, 600), Some(SettingsTarget::Save));
		assert_eq!(target(30, 240), Some(SettingsTarget::ChooseWallpaper));
		assert_eq!(target(200, 240), Some(SettingsTarget::ClearWallpaper));
		assert_eq!(target(530, 390), Some(SettingsTarget::ToggleRestore));
		assert_eq!(target(530, 280), Some(SettingsTarget::Step(FONT_ROW, -1)));
		assert_eq!(
			target(700, 330),
			Some(SettingsTarget::Step(SCROLLBACK_ROW, 1))
		);
		assert_eq!(target(30, 150), Some(SettingsTarget::Theme(-1)));
		assert_eq!(target(720, 150), Some(SettingsTarget::Theme(1)));
		assert_eq!(target(300, 100), None);
	}

	#[test]
	fn the_main_page_fits_a_small_window() {
		// Every row and the Save button stay inside the minimum window size.
		let last_row = [OPACITY_ROW, STRENGTH_ROW, RESTORE_ROW]
			.into_iter()
			.max()
			.unwrap();
		assert!(last_row + ROW_HEIGHT <= MIN_SIZE.1 - 52);
	}

	#[test]
	fn layout_page_toggles_are_clickable() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Layout);
		assert_eq!(target(690, 30), Some(SettingsTarget::Back));
		assert_eq!(
			target(530, SHOW_TABS_ROW + 10),
			Some(SettingsTarget::ToggleTabs)
		);
		assert_eq!(
			target(530, VERTICAL_TABS_ROW + 10),
			Some(SettingsTarget::ToggleVerticalTabs)
		);
		assert_eq!(
			target(530, BOTTOM_BAR_ROW + 10),
			Some(SettingsTarget::ToggleBottomBar)
		);
		assert_eq!(target(630, 600), Some(SettingsTarget::Save));
		// Main-page controls are not reachable from here.
		assert_eq!(target(30, 240), None);
		assert!(BOTTOM_BAR_ROW + ROW_HEIGHT <= MIN_SIZE.1 - 52);
	}

	#[test]
	fn shortcut_bindings_are_clickable() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Shortcuts);
		assert_eq!(target(690, 30), Some(SettingsTarget::Back));
		assert_eq!(
			target(600, 112),
			Some(SettingsTarget::SetShortcut(ShortcutAction::NewTab))
		);
		assert_eq!(
			target(600, 164),
			Some(SettingsTarget::SetShortcut(ShortcutAction::CommandPalette))
		);
	}

	#[test]
	fn about_page_only_exposes_the_back_button() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::About);
		assert_eq!(target(630, 600), None);
		assert_eq!(target(750, 30), None);
		assert_eq!(target(610, 30), Some(SettingsTarget::Back));
	}

	#[test]
	fn hit_testing_scales_with_the_window() {
		assert_eq!(
			settings_target(
				(530 * 2, 280 * 2),
				(SIZE.0 * 2, SIZE.1 * 2),
				2,
				SettingsPage::Main
			),
			Some(SettingsTarget::Step(FONT_ROW, -1))
		);
	}
}
