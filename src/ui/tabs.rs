//! Tab-strip geometry.

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

pub(crate) fn tab_layout(
	width: usize,
	count: usize,
	scale: usize,
	left_reserve: usize,
	scroll: usize,
) -> TabLayout {
	let scale = scale.max(1);
	let arrow_w = 28 * scale;
	let left_arrow_x = left_reserve + 8 * scale;
	let start = left_arrow_x + arrow_w + 6 * scale;
	let plus_w = 52 * scale;
	let settings_w = 42 * scale;
	let controls_gap = 8 * scale;
	let right = 12 * scale;
	let settings_x = width.saturating_sub(right + settings_w);
	let plus_x = settings_x.saturating_sub(controls_gap + plus_w);
	let right_arrow_x = plus_x.saturating_sub(controls_gap + arrow_w);
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tab_strip_can_reach_older_tabs() {
		let first_page = tab_layout(980, 20, 1, 0, 0);
		let last_page = tab_layout(980, 20, 1, 0, 19);

		assert!(first_page.visible_count < 20);
		assert_eq!(first_page.first_visible, 0);
		assert!(last_page.first_visible > 0);
		assert!(last_page.first_visible + last_page.visible_count <= 20);
	}

	#[test]
	fn narrow_tab_strip_keeps_a_reachable_tab() {
		let layout = tab_layout(450, 20, 1, 0, 19);

		assert!(layout.visible_count >= 1);
		assert!(layout.first_visible < 20);
		assert!(layout.first_visible + layout.visible_count <= 20);
	}
}
