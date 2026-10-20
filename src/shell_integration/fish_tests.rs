//! The fish hook next to fish's own marks: fish 4 marks prompts itself
//! (its `mark-prompt` feature), fish 3 needs the hook to mark them.
use super::tests::run;

/// Whether this fish marks prompts itself, or `None` without fish.
pub(super) fn fish_marks_prompts() -> Option<bool> {
	let (stdout, _) = run(
		"fish",
		&["--no-config", "-c"],
		"status test-feature mark-prompt; and echo yes",
	)?;
	Some(stdout == "yes\n")
}

/// Arguments starting fish without its own marks: fish 4 with the feature
/// turned off, or fish 3. `None` without fish.
pub(super) fn without_native_marks() -> Option<Vec<&'static str>> {
	let mut args = vec!["--no-config"];
	if fish_marks_prompts()? {
		args.extend(["--features", "no-mark-prompt"]);
	}
	Some(args)
}

fn run_without_native_marks(script: &str) -> Option<(String, String)> {
	let mut args = without_native_marks()?;
	args.push("-c");
	run("fish", &args, script)
}

#[test]
fn fish_4_keeps_its_own_marks_and_gets_only_the_directory() {
	if fish_marks_prompts() != Some(true) {
		return;
	}
	let script = concat!(
		"function fish_prompt; echo -n 'P> '; end; source shell-integration/fish.fish; ",
		"cd /tmp; false; emit fish_prompt; fish_prompt; emit fish_preexec 'ls'"
	);
	let (stdout, stderr) = run("fish", &["--no-config", "-c"], script).unwrap();
	assert_eq!(stderr, "");
	// A second D would close the next command early, a second A would drop
	// fish's click_events, and B would be reported twice.
	assert!(!stdout.contains("\x1b]133;"), "{stdout:?}");
	assert!(stdout.contains("/tmp\x07"), "{stdout:?}");
	assert!(stdout.ends_with("P> "), "{stdout:?}");
}

#[test]
fn fish_without_its_own_marks_gets_them_from_the_hook() {
	let script = concat!(
		"function fish_prompt; echo -n 'P> '; end; source shell-integration/fish.fish; ",
		"false; emit fish_prompt; fish_prompt; emit fish_preexec 'ls'"
	);
	let Some((stdout, stderr)) = run_without_native_marks(script) else {
		return;
	};
	assert_eq!(stderr, "");
	let after_status = stdout.split_once("\x1b]133;D;1\x07").unwrap().1;
	assert!(
		after_status.starts_with("\x1b]133;A;cl=line\x07"),
		"{stdout:?}"
	);
	assert!(
		stdout.contains("P> \x1b]133;B\x07\x1b]133;C;cmdline_url=ls\x07"),
		"{stdout:?}"
	);
}

#[test]
fn prompts_keep_the_status_and_are_wrapped_again_when_redefined() {
	let script = concat!(
		"function fish_prompt; echo -n \"[$status]> \"; end; source shell-integration/fish.fish; ",
		"emit fish_prompt; false; fish_prompt; ",
		// A theme replacing the prompt is wrapped at the next prompt, once.
		"function fish_prompt; echo -n 'Q> '; end; emit fish_prompt; emit fish_prompt; fish_prompt"
	);
	let Some((stdout, stderr)) = run_without_native_marks(script) else {
		return;
	};
	assert_eq!(stderr, "");
	assert!(stdout.contains("[1]> \x1b]133;B\x07"), "{stdout:?}");
	assert!(stdout.ends_with("Q> \x1b]133;B\x07"), "{stdout:?}");
	assert_eq!(stdout.matches("\x1b]133;B\x07").count(), 2, "{stdout:?}");
}

#[test]
fn a_missing_prompt_function_is_left_alone() {
	let script = concat!(
		"source shell-integration/fish.fish; functions -e fish_prompt; ",
		"function fish_prompt; end; functions -e fish_prompt; emit fish_prompt; ",
		"functions -q fish_prompt; or echo none"
	);
	let Some((stdout, stderr)) = run_without_native_marks(script) else {
		return;
	};
	assert_eq!(stderr, "");
	assert!(stdout.ends_with("none\n"), "{stdout:?}");
}
