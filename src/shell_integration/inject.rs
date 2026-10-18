//! Loading the hooks automatically, without editing the user's startup files.
//!
//! - zsh: `ZDOTDIR` points at a loader `.zshenv` that restores the user's
//!   `ZDOTDIR`, runs their `.zshenv`, and sources the hooks; zsh then reads
//!   the user's `.zprofile` and `.zshrc` as usual.
//! - fish: a `vendor_conf.d` script found through `XDG_DATA_DIRS`, which
//!   removes its directory from `XDG_DATA_DIRS` again.
//! - bash has no clean hook point, so it stays manual
//!   (`poopterm --print-shell-integration bash`).
use std::{ffi::OsString, fs, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shell {
	Zsh,
	Fish,
	Bash,
	Other,
}

impl Shell {
	/// Recognizes a shell from its path, e.g. `/usr/bin/zsh`.
	pub(crate) fn from_path(path: &str) -> Self {
		match Path::new(path).file_name().and_then(|name| name.to_str()) {
			Some("zsh") => Self::Zsh,
			Some("fish") => Self::Fish,
			Some("bash") => Self::Bash,
			_ => Self::Other,
		}
	}
}

/// Writes the loader files under `dir`, rewriting only files that changed.
pub(crate) fn install_scripts(dir: &Path) -> io::Result<()> {
	let fish = include_str!("../../shell-integration/fish-loader.fish").to_owned()
		+ include_str!("../../shell-integration/fish.fish");
	for (path, contents) in [
		(
			dir.join("zsh/.zshenv"),
			include_str!("../../shell-integration/zshenv-loader.zsh").to_owned(),
		),
		(
			dir.join("zsh/poopterm.zsh"),
			include_str!("../../shell-integration/zsh.zsh").to_owned(),
		),
		(dir.join("fish/fish/vendor_conf.d/poopterm.fish"), fish),
	] {
		if fs::read_to_string(&path).is_ok_and(|current| current == contents) {
			continue;
		}
		fs::create_dir_all(path.parent().expect("loader paths have parents"))?;
		fs::write(&path, contents)?;
	}
	Ok(())
}

/// Environment variables that make `shell` load the hooks from `dir`
/// (as written by [`install_scripts`]). `lookup` reads the current environment.
pub(crate) fn injection_env(
	shell: Shell,
	dir: &Path,
	lookup: impl Fn(&str) -> Option<OsString>,
) -> Vec<(String, OsString)> {
	match shell {
		Shell::Zsh => {
			let mut env = vec![("ZDOTDIR".to_owned(), dir.join("zsh").into_os_string())];
			if let Some(original) = lookup("ZDOTDIR") {
				env.push(("POOPTERM_ORIG_ZDOTDIR".to_owned(), original));
			}
			env
		}
		Shell::Fish => {
			let data = dir.join("fish").into_os_string();
			let mut dirs = data.clone();
			dirs.push(":");
			dirs.push(lookup("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()));
			vec![
				("XDG_DATA_DIRS".to_owned(), dirs),
				("POOPTERM_FISH_DATA_DIR".to_owned(), data),
			]
		}
		Shell::Bash | Shell::Other => Vec::new(),
	}
}

/// Prepares automatic loading for `shell_path` per the `shell_integration`
/// setting (`auto`, the default, or `off`), returning the variables to set.
pub(crate) fn prepare(shell_path: &str, setting: Option<&str>) -> Vec<(String, OsString)> {
	if setting.is_some_and(|value| value.trim() == "off") {
		return Vec::new();
	}
	let shell = Shell::from_path(shell_path);
	let Some(dir) = crate::config::data_dir().map(|dir| dir.join("shell-integration")) else {
		return Vec::new();
	};
	if matches!(shell, Shell::Bash | Shell::Other) || install_scripts(&dir).is_err() {
		return Vec::new();
	}
	injection_env(shell, &dir, |name| std::env::var_os(name))
}
