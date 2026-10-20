//! bash reports the command line it runs, from history, through PS0.
use super::tests::{COMMAND, reported_command, shell_command};

/// Whether bash is 4.4 or later, which expands PS0; macOS ships 3.2.
fn bash_has_ps0() -> bool {
	let check = "(( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404 ))";
	shell_command("bash")
		.args(["--norc", "-c", check])
		.status()
		.is_ok_and(|status| status.success())
}

/// Draws a prompt, runs `between` (which may add a history entry) and
/// prints PS0 the way bash does before running a command.
fn ps0_after(between: &str) -> Option<String> {
	if !bash_has_ps0() {
		return None;
	}
	let script = format!(
		"source shell-integration/bash.sh; set -o history; history -s old; \
		 __poopterm_prompt_command >/dev/null; {between}; printf '%s' \"${{PS0@P}}\""
	);
	let output = shell_command("bash")
		.args(["--norc", "-c", &script])
		.env("COMMAND", COMMAND)
		// A user's timestamps would otherwise lead `history` output.
		.env("HISTTIMEFORMAT", "%F %T ")
		.output()
		.ok()?;
	assert_eq!(String::from_utf8_lossy(&output.stderr), "");
	Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn bash_reports_the_command_history_recorded() {
	let Some(ps0) = ps0_after("history -s \"$COMMAND\"") else {
		return;
	};
	assert_eq!(reported_command(&ps0).as_deref(), Some(COMMAND), "{ps0:?}");
	assert_eq!(ps0.matches('\x07').count(), 1, "one sequence: {ps0:?}");
}

#[test]
fn commands_history_skipped_are_not_reported_as_the_one_before() {
	// `ignorespace`, `ignoredups` and `set +o history` add no entry; the
	// newest one is then an older command.
	let Some(ps0) = ps0_after("true") else {
		return;
	};
	assert_eq!(ps0, "\x1b]133;C\x07");
}

#[test]
fn a_ps0_the_user_set_still_follows() {
	let Some(ps0) = ps0_after("true") else {
		return;
	};
	let script = "PS0='[mine]'; source shell-integration/bash.sh; printf '%s' \"${PS0@P}\"";
	let Some(output) = shell_command("bash")
		.args(["--norc", "-c", script])
		.output()
		.ok()
	else {
		return;
	};
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert_eq!(stdout, format!("{ps0}[mine]"));
}
