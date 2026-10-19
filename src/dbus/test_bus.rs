//! A private `dbus-daemon` for tests, stopped when dropped.
use std::{
	io::{BufRead, BufReader},
	path::PathBuf,
	process::{Child, Command, Stdio},
	sync::atomic::{AtomicU32, Ordering},
};

pub(crate) struct TestBus {
	child: Child,
	dir: PathBuf,
	pub address: String,
}

impl TestBus {
	/// Starts a bus, or `None` when `dbus-daemon` is not installed.
	pub(crate) fn start() -> Option<Self> {
		static NEXT: AtomicU32 = AtomicU32::new(0);
		let name = format!(
			"poopterm-bus-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		);
		let dir = std::env::temp_dir().join(name);
		std::fs::create_dir_all(&dir).ok()?;
		let mut child = Command::new("dbus-daemon")
			.args(["--session", "--nofork", "--nopidfile", "--print-address"])
			.arg(format!("--address=unix:path={}", dir.join("bus").display()))
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.spawn()
			.ok()?;
		let mut address = String::new();
		BufReader::new(child.stdout.take()?)
			.read_line(&mut address)
			.ok()?;
		Some(Self {
			child,
			dir,
			address: address.trim().to_owned(),
		})
	}
}

impl Drop for TestBus {
	fn drop(&mut self) {
		let _ = self.child.kill();
		let _ = self.child.wait();
		let _ = std::fs::remove_dir_all(&self.dir);
	}
}
