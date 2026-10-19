//! Tab-strip geometry and markers.
use crate::term::status::State;

pub(crate) struct TabLayout {
	pub(crate) start: usize,
	pub(crate) width: usize,
	pub(crate) gap: usize,
	pub(crate) first_visible: usize,
	pub(crate) visible_count: usize,
	pub(crate) left_arrow_x: usize,
	pub(crate) right_arrow_x: usize,
	pub(crate) arrow_w: usize,
	pub(crate) plus_x: usize,
	pub(crate) plus_w: usize,
	pub(crate) settings_x: usize,
	pub(crate) settings_w: usize,
	pub(crate) scale: usize,
}

/// Geometry for tabs arranged in a left sidebar.
pub(crate) struct VerticalTabLayout {
	pub(crate) sidebar_width: usize,
	pub(crate) tab_start_y: usize,
	pub(crate) tab_width: usize,
	pub(crate) row_height: usize,
	pub(crate) first_visible: usize,
	pub(crate) visible_count: usize,
	pub(crate) plus_x: usize,
	pub(crate) plus_y: Option<usize>,
	pub(crate) settings_x: usize,
	pub(crate) settings_y: Option<usize>,
	pub(crate) button_width: usize,
	pub(crate) button_height: usize,
	pub(crate) scale: usize,
}

pub(crate) struct VerticalTabOptions {
	pub(crate) top_reserve: usize,
	pub(crate) status_height: usize,
	pub(crate) scroll: usize,
	pub(crate) show_plus: bool,
	pub(crate) show_settings: bool,
}

pub(crate) fn tab_layout(
	width: usize,
	count: usize,
	scale: usize,
	left_reserve: usize,
	scroll: usize,
	show_plus: bool,
	show_settings: bool,
) -> TabLayout {
	let scale = scale.max(1);
	let arrow_w = 28 * scale;
	let left_arrow_x = left_reserve + 8 * scale;
	let start = left_arrow_x + arrow_w + 6 * scale;
	let plus_w = if show_plus { 52 * scale } else { 0 };
	let settings_w = if show_settings { 42 * scale } else { 0 };
	let controls_gap = if show_plus && show_settings {
		8 * scale
	} else {
		0
	};
	let right = 12 * scale;
	let settings_x = width.saturating_sub(right + settings_w);
	let plus_x = settings_x.saturating_sub(controls_gap + plus_w);
	let controls_left = if show_plus {
		plus_x
	} else if show_settings {
		settings_x
	} else {
		width.saturating_sub(right)
	};
	let right_arrow_x = controls_left.saturating_sub(8 * scale + arrow_w);
	let viewport_end = right_arrow_x.saturating_sub(8 * scale);
	let gap = 6 * scale;
	let room = viewport_end.saturating_sub(start);
	let slots = ((room + gap) / (170 * scale + gap)).max(1);
	let tab_width = room
		.saturating_sub(slots.saturating_sub(1) * gap)
		.checked_div(slots)
		.unwrap_or(0)
		.min(220 * scale);
	let visible_count = ((room + gap) / (tab_width.max(1) + gap)).max(1);
	let first_visible = scroll.min(count.saturating_sub(visible_count));
	TabLayout {
		start,
		width: tab_width,
		gap,
		first_visible,
		visible_count,
		left_arrow_x,
		right_arrow_x,
		arrow_w,
		plus_x,
		plus_w,
		settings_x,
		settings_w,
		scale,
	}
}

pub(crate) fn vertical_tab_layout(
	height: usize,
	count: usize,
	scale: usize,
	options: VerticalTabOptions,
) -> VerticalTabLayout {
	let scale = scale.max(1);
	let VerticalTabOptions {
		top_reserve,
		status_height,
		scroll,
		show_plus,
		show_settings,
	} = options;
	let sidebar_width = 184 * scale;
	let row_height = 32 * scale;
	let tab_start_y = top_reserve + 8 * scale;
	let button_height = 28 * scale;
	let button_width = 32 * scale;
	let has_buttons = show_plus || show_settings;
	let button_y = has_buttons.then(|| {
		height
			.saturating_sub(status_height)
			.saturating_sub(10 * scale)
			.saturating_sub(button_height)
	});
	let tab_end_y = button_y.map_or_else(
		|| height.saturating_sub(status_height + 10 * scale),
		|y| y.saturating_sub(6 * scale),
	);
	let visible_count = ((tab_end_y.saturating_sub(tab_start_y)) / row_height).max(1);
	let first_visible = scroll.min(count.saturating_sub(visible_count));
	let button_gap = 6 * scale;
	let buttons_width = usize::from(show_plus) * button_width
		+ usize::from(show_settings) * button_width
		+ usize::from(show_plus && show_settings) * button_gap;
	let controls_start_x = sidebar_width.saturating_sub(buttons_width) / 2;
	let plus_x = controls_start_x;
	let settings_x = if show_plus {
		controls_start_x + button_width + button_gap
	} else {
		controls_start_x
	};
	VerticalTabLayout {
		sidebar_width,
		tab_start_y,
		tab_width: sidebar_width.saturating_sub(12 * scale),
		row_height,
		first_visible,
		visible_count,
		plus_x,
		plus_y: show_plus.then_some(button_y.unwrap_or(0)),
		settings_x,
		settings_y: show_settings.then_some(button_y.unwrap_or(0)),
		button_width,
		button_height,
		scale,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tab_strip_can_reach_older_tabs() {
		let first_page = tab_layout(980, 20, 1, 0, 0, true, true);
		let last_page = tab_layout(980, 20, 1, 0, 19, true, true);

		assert!(first_page.visible_count < 20);
		assert_eq!(first_page.first_visible, 0);
		assert!(last_page.first_visible > 0);
		assert!(last_page.first_visible + last_page.visible_count <= 20);
	}

	#[test]
	fn narrow_tab_strip_keeps_a_reachable_tab() {
		let layout = tab_layout(450, 20, 1, 0, 19, true, true);

		assert!(layout.visible_count >= 1);
		assert!(layout.first_visible < 20);
		assert!(layout.first_visible + layout.visible_count <= 20);
	}

	#[test]
	fn hidden_horizontal_controls_give_tabs_their_space() {
		let layout = tab_layout(980, 20, 1, 0, 0, false, false);

		assert_eq!(layout.plus_w, 0);
		assert_eq!(layout.settings_w, 0);
		assert!(layout.visible_count > tab_layout(980, 20, 1, 0, 0, true, true).visible_count);
	}

	#[test]
	fn vertical_tab_layout_pages_and_places_optional_controls() {
		let layout = vertical_tab_layout(
			640,
			20,
			1,
			VerticalTabOptions {
				top_reserve: 52,
				status_height: 26,
				scroll: 19,
				show_plus: true,
				show_settings: false,
			},
		);

		assert!(layout.first_visible > 0);
		assert!(layout.first_visible + layout.visible_count <= 20);
		assert_eq!(layout.sidebar_width, 184);
		assert_eq!(layout.row_height, 32);
		assert_eq!(layout.plus_y, Some(576));
		assert_eq!(layout.settings_y, None);
	}

	#[test]
	fn vertical_controls_are_compact_and_share_a_centered_row() {
		let layout = vertical_tab_layout(
			640,
			4,
			1,
			VerticalTabOptions {
				top_reserve: 52,
				status_height: 26,
				scroll: 0,
				show_plus: true,
				show_settings: true,
			},
		);

		assert_eq!(layout.button_width, 32);
		assert_eq!(layout.button_height, 28);
		assert_eq!(layout.settings_x - layout.plus_x, 38);
	}
}

/// The sign before a tab's title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Marker {
	Bell,
	Blocked,
	Error,
	Done,
	Working,
	Activity,
}

/// What a tab's marker shows: what most needs the user first.
pub(crate) fn tab_marker(bell: bool, status: Option<State>, activity: bool) -> Option<Marker> {
	if bell {
		return Some(Marker::Bell);
	}
	match status {
		Some(State::Blocked) => Some(Marker::Blocked),
		Some(State::Error) => Some(Marker::Error),
		Some(State::Done) => Some(Marker::Done),
		Some(State::Working) => Some(Marker::Working),
		Some(State::Idle) | None => activity.then_some(Marker::Activity),
	}
}

#[cfg(test)]
mod marker_tests {
	use super::*;

	#[test]
	fn markers_put_what_needs_the_user_first() {
		assert_eq!(
			tab_marker(true, Some(State::Blocked), true),
			Some(Marker::Bell)
		);
		assert_eq!(
			tab_marker(false, Some(State::Blocked), true),
			Some(Marker::Blocked)
		);
		assert_eq!(
			tab_marker(false, Some(State::Done), false),
			Some(Marker::Done)
		);
		assert_eq!(
			tab_marker(false, Some(State::Working), true),
			Some(Marker::Working)
		);
		assert_eq!(
			tab_marker(false, Some(State::Idle), true),
			Some(Marker::Activity)
		);
		assert_eq!(tab_marker(false, None, false), None);
	}
}
