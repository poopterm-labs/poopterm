//! Boundary tests for vertical tab sidebars.
use super::tabs::{VerticalTabOptions, vertical_tab_layout};

fn side(
	height: usize,
	count: usize,
	scale: usize,
	options: VerticalTabOptions,
) -> super::tabs::VerticalTabLayout {
	vertical_tab_layout(height, count, scale, options)
}
#[test]
fn zero_vertical_tabs_keep_a_valid_empty_layout() {
	let layout = side(
		640,
		0,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 99,
			active_tab: 0,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.visible_count >= 1);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.plus_y, layout.settings_y);
}

#[test]
fn one_vertical_compact_tab_keeps_the_rail_geometry() {
	let layout = side(
		640,
		1,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 0,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.tab_width, 52);
	assert_eq!(layout.row_height, 40);
	assert_eq!(layout.row_height_for(0), 40);
	assert_eq!(layout.first_visible, 0);
}

#[test]
fn zero_vertical_scale_uses_one_pixel_scale_for_sidebar_dimensions() {
	let layout = side(
		640,
		20,
		0,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 0,
			compact_tabs: true,
			show_plus: true,
			show_settings: false,
		},
	);
	assert_eq!(layout.scale, 1);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.button_width, 24);
	assert_eq!(layout.button_height, 24);
}

#[test]
fn hidden_vertical_controls_leave_the_tab_area_open_to_the_bottom() {
	let layout = side(
		640,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 0,
			compact_tabs: true,
			show_plus: false,
			show_settings: false,
		},
	);
	assert_eq!(layout.plus_y, None);
	assert_eq!(layout.settings_y, None);
	assert!(layout.visible_count > 0);
	assert_eq!(layout.sidebar_width, 64);
}

#[test]
fn vertical_buttons_share_one_bottom_anchor_when_both_are_visible() {
	let layout = side(
		640,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 0,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert_eq!(layout.plus_y, Some(580));
	assert_eq!(layout.settings_y, Some(580));
	assert_eq!(layout.settings_x - layout.plus_x, 28);
	assert!(layout.tab_start_y < layout.plus_y.unwrap());
}

#[test]
fn vertical_plus_button_configuration_has_no_phantom_control() {
	let layout = side(
		640,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 1,
			compact_tabs: true,
			show_plus: true,
			show_settings: false,
		},
	);
	assert!(layout.plus_y.is_some());
	assert!(layout.settings_y.is_none());
	assert!(layout.button_width > 0);
	assert!(layout.button_height > 0);
}

#[test]
fn vertical_settings_button_configuration_has_no_phantom_control() {
	let layout = side(
		640,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 1,
			compact_tabs: true,
			show_plus: false,
			show_settings: true,
		},
	);
	assert!(layout.plus_y.is_none());
	assert!(layout.settings_y.is_some());
	assert!(layout.button_width > 0);
	assert!(layout.button_height > 0);
}

#[test]
fn vertical_hidden_button_configuration_has_no_phantom_control() {
	let layout = side(
		640,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 1,
			compact_tabs: true,
			show_plus: false,
			show_settings: false,
		},
	);
	assert!(layout.plus_y.is_none());
	assert!(layout.settings_y.is_none());
	assert!(layout.button_width > 0);
	assert!(layout.button_height > 0);
}

#[test]
fn vertical_scroll_0_never_starts_past_the_last_tab() {
	let layout = side(
		320,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 0,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert!(layout.first_visible < 20);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert!(layout.visible_count >= 1);
}

#[test]
fn vertical_scroll_1_never_starts_past_the_last_tab() {
	let layout = side(
		320,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 1,
			active_tab: 1,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert!(layout.first_visible < 20);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert!(layout.visible_count >= 1);
}

#[test]
fn vertical_scroll_5_never_starts_past_the_last_tab() {
	let layout = side(
		320,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 5,
			active_tab: 5,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert!(layout.first_visible < 20);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert!(layout.visible_count >= 1);
}

#[test]
fn vertical_scroll_10_never_starts_past_the_last_tab() {
	let layout = side(
		320,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 10,
			active_tab: 10,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert!(layout.first_visible < 20);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert!(layout.visible_count >= 1);
}

#[test]
fn vertical_scroll_19_never_starts_past_the_last_tab() {
	let layout = side(
		320,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 19,
			active_tab: 19,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert!(layout.first_visible < 20);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert!(layout.visible_count >= 1);
}

#[test]
fn vertical_scroll_beyond_tab_count_clamps_safely() {
	let layout = side(
		320,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: usize::MAX,
			active_tab: 19,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert_eq!(layout.first_visible, 20 - layout.visible_count);
	assert!(layout.first_visible + layout.visible_count <= 20);
}

#[test]
fn a_short_vertical_window_keeps_bottom_buttons_inside_the_window() {
	let layout = side(
		40,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 0,
			compact_tabs: true,
			show_plus: true,
			show_settings: true,
		},
	);
	assert_eq!(layout.plus_y, Some(0));
	assert_eq!(layout.settings_y, Some(0));
	assert!(layout.visible_count >= 1);
}

#[test]
fn regular_vertical_rows_do_not_compact_when_the_compact_option_is_off() {
	let layout = side(
		640,
		20,
		1,
		VerticalTabOptions {
			top_reserve: 52,
			status_height: 26,
			scroll: 0,
			active_tab: 9,
			compact_tabs: false,
			show_plus: true,
			show_settings: true,
		},
	);
	assert!(!layout.compact_rail);
	assert_eq!(layout.row_height_for(0), 32);
	assert_eq!(layout.row_height_for(9), 32);
	assert_eq!(layout.row_height_for(19), 32);
}
