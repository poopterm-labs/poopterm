//! Transparent huge page hints for large, long-lived buffers (Linux only).
//!
//! Not wired into any allocation yet: each buffer that adopts `advise` must
//! show a reproducible RSS or throughput gain first (see `TODO.md`).
#![allow(dead_code, reason = "not adopted by any buffer yet")]

use std::sync::OnceLock;

/// Size of a huge page on the platforms we advise (x86-64 and aarch64 with
/// 4 KiB base pages).
pub(crate) const HUGE_PAGE: usize = 2 << 20;

const THP_ENABLED: &str = "/sys/kernel/mm/transparent_hugepage/enabled";

/// Whether `madvise(MADV_HUGEPAGE)` can take effect, read once.
pub(crate) fn enabled() -> bool {
	static ENABLED: OnceLock<bool> = OnceLock::new();
	*ENABLED.get_or_init(|| {
		cfg!(target_os = "linux")
			&& std::fs::read_to_string(THP_ENABLED).is_ok_and(|text| mode_allows_madvise(&text))
	})
}

/// Parses the sysfs file, where the active mode is bracketed:
/// `always [madvise] never`.
fn mode_allows_madvise(text: &str) -> bool {
	let active = text.split_whitespace().find(|word| word.starts_with('['));
	matches!(active, Some("[always]" | "[madvise]"))
}

/// The huge-page-aligned part of `[start, start + len)`, as `(start, len)`,
/// or `None` when it holds no whole huge page.
fn aligned_range(start: usize, len: usize) -> Option<(usize, usize)> {
	let first = start.checked_next_multiple_of(HUGE_PAGE)?;
	let end = (start.checked_add(len)?) / HUGE_PAGE * HUGE_PAGE;
	(end > first).then(|| (first, end - first))
}

/// Asks the kernel to back the aligned part of `buf` with huge pages. Does
/// nothing for buffers smaller than a huge page, when THP is off, and on
/// other platforms. Purely a hint: failures are ignored.
pub(crate) fn advise<T>(buf: &[T]) {
	if size_of_val(buf) < HUGE_PAGE || !enabled() {
		return;
	}
	let Some((_start, _len)) = aligned_range(buf.as_ptr() as usize, size_of_val(buf)) else {
		return;
	};
	#[cfg(target_os = "linux")]
	// SAFETY: the range lies inside `buf`, and MADV_HUGEPAGE only changes how
	// the kernel backs those pages, never their contents.
	unsafe {
		libc::madvise(_start as *mut libc::c_void, _len, libc::MADV_HUGEPAGE);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reads_the_bracketed_mode() {
		assert!(mode_allows_madvise("always [madvise] never\n"));
		assert!(mode_allows_madvise("[always] madvise never\n"));
		assert!(!mode_allows_madvise("always madvise [never]\n"));
		assert!(!mode_allows_madvise(""));
	}

	#[test]
	fn aligned_range_keeps_only_whole_huge_pages() {
		assert_eq!(aligned_range(0, HUGE_PAGE), Some((0, HUGE_PAGE)));
		assert_eq!(aligned_range(1, HUGE_PAGE), None);
		assert_eq!(
			aligned_range(1, 3 * HUGE_PAGE),
			Some((HUGE_PAGE, 2 * HUGE_PAGE))
		);
		assert_eq!(aligned_range(usize::MAX - 1, 8), None);
	}

	#[test]
	fn advise_accepts_small_and_large_buffers() {
		advise(&[0u8; 16]);
		advise(&vec![0u8; 3 * HUGE_PAGE]);
	}
}
