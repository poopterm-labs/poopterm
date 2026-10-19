//! Focused tests for vertical tab sidebar geometry.
use super::tabs::{VerticalTabLayout, VerticalTabOptions, vertical_tab_layout};

fn options(
	scale: usize,
	scroll: usize,
	active_tab: usize,
	compact_tabs: bool,
	show_plus: bool,
	show_settings: bool,
) -> VerticalTabOptions {
	VerticalTabOptions {
		top_reserve: 52 * scale,
		status_height: 26 * scale,
		scroll,
		active_tab,
		compact_tabs,
		show_plus,
		show_settings,
	}
}

fn sidebar(
	height: usize,
	count: usize,
	scale: usize,
	options: VerticalTabOptions,
) -> VerticalTabLayout {
	vertical_tab_layout(height, count, scale, options)
}

fn assert_sidebar_geometry(layout: &VerticalTabLayout, count: usize) {
	assert!(layout.first_visible + layout.visible_count <= count);
	for index in layout.first_visible..layout.first_visible + layout.visible_count.saturating_sub(1) {
		assert_eq!(
			layout.row_y(index + 1),
			layout.row_y(index) + layout.row_height_for(index),
		);
	}
}

#[test]
fn compact_rail_active_tab_0_uses_an_icon_row() {
	let layout = sidebar(640, 30, 1, options(1, 0, 0, true, true, true));
	assert!(layout.compact_rail);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.tab_width, 52);
	assert_eq!(layout.row_height, 40);
	assert_eq!(layout.row_height_for(0), 40);
	assert_eq!(layout.row_y(0), layout.tab_start_y + 0 * 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_active_tab_1_uses_an_icon_row() {
	let layout = sidebar(640, 30, 1, options(1, 0, 1, true, true, true));
	assert!(layout.compact_rail);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.tab_width, 52);
	assert_eq!(layout.row_height, 40);
	assert_eq!(layout.row_height_for(1), 40);
	assert_eq!(layout.row_y(1), layout.tab_start_y + 1 * 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_active_tab_2_uses_an_icon_row() {
	let layout = sidebar(640, 30, 1, options(1, 0, 2, true, true, true));
	assert!(layout.compact_rail);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.tab_width, 52);
	assert_eq!(layout.row_height, 40);
	assert_eq!(layout.row_height_for(2), 40);
	assert_eq!(layout.row_y(2), layout.tab_start_y + 2 * 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_active_tab_3_uses_an_icon_row() {
	let layout = sidebar(640, 30, 1, options(1, 0, 3, true, true, true));
	assert!(layout.compact_rail);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.tab_width, 52);
	assert_eq!(layout.row_height, 40);
	assert_eq!(layout.row_height_for(3), 40);
	assert_eq!(layout.row_y(3), layout.tab_start_y + 3 * 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_active_tab_4_uses_an_icon_row() {
	let layout = sidebar(640, 30, 1, options(1, 0, 4, true, true, true));
	assert!(layout.compact_rail);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.tab_width, 52);
	assert_eq!(layout.row_height, 40);
	assert_eq!(layout.row_height_for(4), 40);
	assert_eq!(layout.row_y(4), layout.tab_start_y + 4 * 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_active_tab_5_uses_an_icon_row() {
	let layout = sidebar(640, 30, 1, options(1, 0, 5, true, true, true));
	assert!(layout.compact_rail);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.tab_width, 52);
	assert_eq!(layout.row_height, 40);
	assert_eq!(layout.row_height_for(5), 40);
	assert_eq!(layout.row_y(5), layout.tab_start_y + 5 * 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scale_1_scales_rows_and_bottom_buttons() {
	let layout = sidebar(640 * 1, 30, 1, options(1, 0, 4, true, true, true));
	assert_eq!(layout.sidebar_width, 64 * 1);
	assert_eq!(layout.tab_width, 52 * 1);
	assert_eq!(layout.row_height, 40 * 1);
	assert_eq!(layout.button_width, 24 * 1);
	assert_eq!(layout.button_height, 24 * 1);
	assert_eq!(layout.plus_y, Some(640 * 1 - 60 * 1));
	assert_eq!(layout.settings_y, layout.plus_y);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scale_2_scales_rows_and_bottom_buttons() {
	let layout = sidebar(640 * 2, 30, 2, options(2, 0, 4, true, true, true));
	assert_eq!(layout.sidebar_width, 64 * 2);
	assert_eq!(layout.tab_width, 52 * 2);
	assert_eq!(layout.row_height, 40 * 2);
	assert_eq!(layout.button_width, 24 * 2);
	assert_eq!(layout.button_height, 24 * 2);
	assert_eq!(layout.plus_y, Some(640 * 2 - 60 * 2));
	assert_eq!(layout.settings_y, layout.plus_y);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scale_3_scales_rows_and_bottom_buttons() {
	let layout = sidebar(640 * 3, 30, 3, options(3, 0, 4, true, true, true));
	assert_eq!(layout.sidebar_width, 64 * 3);
	assert_eq!(layout.tab_width, 52 * 3);
	assert_eq!(layout.row_height, 40 * 3);
	assert_eq!(layout.button_width, 24 * 3);
	assert_eq!(layout.button_height, 24 * 3);
	assert_eq!(layout.plus_y, Some(640 * 3 - 60 * 3));
	assert_eq!(layout.settings_y, layout.plus_y);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scale_4_scales_rows_and_bottom_buttons() {
	let layout = sidebar(640 * 4, 30, 4, options(4, 0, 4, true, true, true));
	assert_eq!(layout.sidebar_width, 64 * 4);
	assert_eq!(layout.tab_width, 52 * 4);
	assert_eq!(layout.row_height, 40 * 4);
	assert_eq!(layout.button_width, 24 * 4);
	assert_eq!(layout.button_height, 24 * 4);
	assert_eq!(layout.plus_y, Some(640 * 4 - 60 * 4));
	assert_eq!(layout.settings_y, layout.plus_y);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_control_mask_0_keeps_controls_at_the_bottom() {
	let layout = sidebar(640, 30, 1, options(1, 0, 3, true, false, false));
	assert_eq!(layout.plus_y, None);
	assert_eq!(layout.settings_y, None);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.button_width, 24);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_control_mask_1_keeps_controls_at_the_bottom() {
	let layout = sidebar(640, 30, 1, options(1, 0, 3, true, true, false));
	assert_eq!(layout.plus_y, Some(580));
	assert_eq!(layout.settings_y, None);
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.button_width, 24);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_control_mask_2_keeps_controls_at_the_bottom() {
	let layout = sidebar(640, 30, 1, options(1, 0, 3, true, false, true));
	assert_eq!(layout.plus_y, None);
	assert_eq!(layout.settings_y, Some(580));
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.button_width, 24);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_control_mask_3_keeps_controls_at_the_bottom() {
	let layout = sidebar(640, 30, 1, options(1, 0, 3, true, true, true));
	assert_eq!(layout.plus_y, Some(580));
	assert_eq!(layout.settings_y, Some(580));
	assert_eq!(layout.sidebar_width, 64);
	assert_eq!(layout.button_width, 24);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_height_200_computes_a_valid_page() {
	let layout = sidebar(200, 30, 1, options(1, 0, 0, true, true, true));
	assert!(layout.visible_count >= 1);
	assert!(layout.visible_count <= 30);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.tab_start_y >= 60);
	assert!(
		layout.tab_start_y + layout.visible_count * layout.row_height <= layout.plus_y.unwrap() - 6
	);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_height_320_computes_a_valid_page() {
	let layout = sidebar(320, 30, 1, options(1, 0, 0, true, true, true));
	assert!(layout.visible_count >= 1);
	assert!(layout.visible_count <= 30);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.tab_start_y >= 60);
	assert!(
		layout.tab_start_y + layout.visible_count * layout.row_height <= layout.plus_y.unwrap() - 6
	);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_height_480_computes_a_valid_page() {
	let layout = sidebar(480, 30, 1, options(1, 0, 0, true, true, true));
	assert!(layout.visible_count >= 1);
	assert!(layout.visible_count <= 30);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.tab_start_y >= 60);
	assert!(
		layout.tab_start_y + layout.visible_count * layout.row_height <= layout.plus_y.unwrap() - 6
	);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_height_640_computes_a_valid_page() {
	let layout = sidebar(640, 30, 1, options(1, 0, 0, true, true, true));
	assert!(layout.visible_count >= 1);
	assert!(layout.visible_count <= 30);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.tab_start_y >= 60);
	assert!(
		layout.tab_start_y + layout.visible_count * layout.row_height <= layout.plus_y.unwrap() - 6
	);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_height_800_computes_a_valid_page() {
	let layout = sidebar(800, 30, 1, options(1, 0, 0, true, true, true));
	assert!(layout.visible_count >= 1);
	assert!(layout.visible_count <= 30);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.tab_start_y >= 60);
	assert!(
		layout.tab_start_y + layout.visible_count * layout.row_height <= layout.plus_y.unwrap() - 6
	);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_height_1080_computes_a_valid_page() {
	let layout = sidebar(1080, 30, 1, options(1, 0, 0, true, true, true));
	assert!(layout.visible_count >= 1);
	assert!(layout.visible_count <= 30);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.tab_start_y >= 60);
	assert!(
		layout.tab_start_y + layout.visible_count * layout.row_height <= layout.plus_y.unwrap() - 6
	);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scroll_0_keeps_the_active_tab_visible() {
	let layout = sidebar(640, 30, 1, options(1, 0, 0, true, true, true));
	assert_eq!(layout.first_visible, 0);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 30);
	assert_eq!(layout.row_y(0), layout.tab_start_y);
	assert_eq!(layout.row_height_for(0), 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scroll_1_keeps_the_active_tab_visible() {
	let layout = sidebar(640, 30, 1, options(1, 1, 1, true, true, true));
	assert_eq!(layout.first_visible, 1);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 30);
	assert_eq!(layout.row_y(1), layout.tab_start_y);
	assert_eq!(layout.row_height_for(1), 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scroll_5_keeps_the_active_tab_visible() {
	let layout = sidebar(640, 30, 1, options(1, 5, 5, true, true, true));
	assert_eq!(layout.first_visible, 5);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 30);
	assert_eq!(layout.row_y(5), layout.tab_start_y);
	assert_eq!(layout.row_height_for(5), 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scroll_10_keeps_the_active_tab_visible() {
	let layout = sidebar(640, 30, 1, options(1, 10, 10, true, true, true));
	assert_eq!(layout.first_visible, 10);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 30);
	assert_eq!(layout.row_y(10), layout.tab_start_y);
	assert_eq!(layout.row_height_for(10), 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn compact_rail_scroll_18_keeps_the_active_tab_visible() {
	let layout = sidebar(640, 30, 1, options(1, 18, 18, true, true, true));
	assert_eq!(layout.first_visible, 18);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 30);
	assert_eq!(layout.row_y(18), layout.tab_start_y);
	assert_eq!(layout.row_height_for(18), 40);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn regular_sidebar_scale_1_keeps_full_width_rows() {
	let layout = sidebar(640 * 1, 30, 1, options(1, 0, 5, false, true, true));
	assert!(!layout.compact_rail);
	assert_eq!(layout.sidebar_width, 184 * 1);
	assert_eq!(layout.tab_width, 172 * 1);
	assert_eq!(layout.row_height, 32 * 1);
	assert_eq!(layout.row_height_for(5), 32 * 1);
	assert_eq!(layout.button_width, 32 * 1);
	assert_eq!(layout.button_height, 28 * 1);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn regular_sidebar_scale_2_keeps_full_width_rows() {
	let layout = sidebar(640 * 2, 30, 2, options(2, 0, 5, false, true, true));
	assert!(!layout.compact_rail);
	assert_eq!(layout.sidebar_width, 184 * 2);
	assert_eq!(layout.tab_width, 172 * 2);
	assert_eq!(layout.row_height, 32 * 2);
	assert_eq!(layout.row_height_for(5), 32 * 2);
	assert_eq!(layout.button_width, 32 * 2);
	assert_eq!(layout.button_height, 28 * 2);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn regular_sidebar_scale_3_keeps_full_width_rows() {
	let layout = sidebar(640 * 3, 30, 3, options(3, 0, 5, false, true, true));
	assert!(!layout.compact_rail);
	assert_eq!(layout.sidebar_width, 184 * 3);
	assert_eq!(layout.tab_width, 172 * 3);
	assert_eq!(layout.row_height, 32 * 3);
	assert_eq!(layout.row_height_for(5), 32 * 3);
	assert_eq!(layout.button_width, 32 * 3);
	assert_eq!(layout.button_height, 28 * 3);
	assert_sidebar_geometry(&layout, 30);
}

#[test]
fn regular_sidebar_scale_4_keeps_full_width_rows() {
	let layout = sidebar(640 * 4, 30, 4, options(4, 0, 5, false, true, true));
	assert!(!layout.compact_rail);
	assert_eq!(layout.sidebar_width, 184 * 4);
	assert_eq!(layout.tab_width, 172 * 4);
	assert_eq!(layout.row_height, 32 * 4);
	assert_eq!(layout.row_height_for(5), 32 * 4);
	assert_eq!(layout.button_width, 32 * 4);
	assert_eq!(layout.button_height, 28 * 4);
	assert_sidebar_geometry(&layout, 30);
}
