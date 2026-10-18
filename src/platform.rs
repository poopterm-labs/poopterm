//! Platform-specific window tweaks.
#[cfg(target_os = "macos")]
use winit::window::Window;

#[cfg(target_os = "macos")]
pub(crate) fn set_window_opacity(window: &Window, opacity: usize) {
	use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
	let Ok(handle) = window.window_handle() else {
		return;
	};
	let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
		return;
	};
	let view = handle.ns_view.as_ptr().cast::<objc2::runtime::AnyObject>();
	unsafe {
		let native_window: *mut objc2::runtime::AnyObject = objc2::msg_send![view, window];
		if !native_window.is_null() {
			let _: () = objc2::msg_send![native_window, setAlphaValue: opacity as f64 / 100.0];
		}
	}
}
