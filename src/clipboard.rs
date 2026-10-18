//! System clipboard and URL opening through platform helper programs.
#[cfg(all(unix, not(target_os = "macos")))]
use std::env;
use std::{
	io::Write,
	process::{Command, Stdio},
	thread,
};

pub(crate) fn clipboard_write_with(
	program: &str,
	args: &[&str],
	text: &str,
) -> std::io::Result<()> {
	let mut child = Command::new(program)
		.args(args)
		.stdin(Stdio::piped())
		.spawn()?;
	child.stdin.take().unwrap().write_all(text.as_bytes())?;
	let status = child.wait()?;
	if status.success() {
		Ok(())
	} else {
		Err(std::io::Error::other(format!(
			"{program} exited with {status}"
		)))
	}
}

pub(crate) fn clipboard_read_with(program: &str, args: &[&str]) -> std::io::Result<String> {
	let output = Command::new(program).args(args).output()?;
	if output.status.success() {
		Ok(String::from_utf8_lossy(&output.stdout).into_owned())
	} else {
		Err(std::io::Error::other(format!(
			"{program} exited with {}",
			output.status
		)))
	}
}

#[cfg(target_os = "macos")]
pub(crate) fn open_external(target: &str) -> std::io::Result<()> {
	let mut child = Command::new("open").arg(target).spawn()?;
	thread::spawn(move || {
		let _ = child.wait();
	});
	Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn open_external(target: &str) -> std::io::Result<()> {
	let mut child = Command::new("xdg-open").arg(target).spawn()?;
	thread::spawn(move || {
		let _ = child.wait();
	});
	Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn clipboard_write(text: &str) -> std::io::Result<()> {
	clipboard_write_with("pbcopy", &[], text)
}

#[cfg(target_os = "macos")]
pub(crate) fn clipboard_read() -> std::io::Result<String> {
	clipboard_read_with("pbpaste", &[])
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn clipboard_write(text: &str) -> std::io::Result<()> {
	if env::var_os("WAYLAND_DISPLAY").is_some()
		&& let Ok(()) = clipboard_write_with("wl-copy", &[], text)
	{
		return Ok(());
	}
	clipboard_write_with("xclip", &["-selection", "clipboard"], text)
		.or_else(|_| clipboard_write_with("xsel", &["--clipboard", "--input"], text))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn clipboard_read() -> std::io::Result<String> {
	if env::var_os("WAYLAND_DISPLAY").is_some()
		&& let Ok(text) = clipboard_read_with("wl-paste", &["--no-newline"])
	{
		return Ok(text);
	}
	clipboard_read_with("xclip", &["-selection", "clipboard", "-o"])
		.or_else(|_| clipboard_read_with("xsel", &["--clipboard", "--output"]))
}

#[cfg(not(unix))]
pub(crate) fn clipboard_write(_: &str) -> std::io::Result<()> {
	Err(std::io::Error::other(
		"clipboard support is unavailable on this platform",
	))
}

#[cfg(not(unix))]
pub(crate) fn clipboard_read() -> std::io::Result<String> {
	Err(std::io::Error::other(
		"clipboard support is unavailable on this platform",
	))
}
