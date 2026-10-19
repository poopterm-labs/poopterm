//! Program status: OSC 7501 records and OSC 9;4 progress, from bytes in to
//! stored state and the feature detection reply out.
use super::status::{Kind, Progress, ProgressState, State};
use crate::{
	graphics::base64::encode,
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(8, 2, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn b64(text: &str) -> String {
	encode(text.as_bytes(), false)
}

/// Ids and states of the records, oldest update first.
fn states(term: &Term) -> Vec<(String, State)> {
	term
		.status
		.records()
		.map(|record| (record.id.clone(), record.state))
		.collect()
}

#[test]
fn reports_replace_their_record_completely() {
	let mut t = terminal();
	send(
		&mut t,
		&format!(
			"\x1b]7501;state=blocked:kind=permission:app=terraform:progress=40:title={}:msg={}\x1b\\",
			b64("Plan"),
			b64("Apply 3 to add?")
		),
	);
	let record = t.status.get("").unwrap();
	assert_eq!(record.state, State::Blocked);
	assert_eq!(record.kind, Some(Kind::Permission));
	assert_eq!(record.progress, Some(40));
	assert_eq!(record.app.as_deref(), Some("terraform"));
	assert_eq!(record.title.as_deref(), Some("Plan"));
	assert_eq!(record.msg.as_deref(), Some("Apply 3 to add?"));
	// Keys left out are gone; kind and progress only go with some states;
	// whitespace is trimmed, the last repeated key wins, unknown keys and
	// malformed pairs are skipped.
	send(
		&mut t,
		"\x1b]7501; state = idle :state=done:kind=auth:progress=5:future=1:bad:x=a b\x07",
	);
	let record = t.status.get("").unwrap();
	assert_eq!(record.state, State::Done);
	assert_eq!(
		(record.kind, record.progress, record.app.as_deref()),
		(None, None, None)
	);
	// Out-of-range progress and unknown kinds are treated as absent.
	send(
		&mut t,
		"\x1b]7501;state=working:progress=101\x07\x1b]7501;id=b:state=blocked:kind=x\x07",
	);
	assert_eq!(t.status.get("").unwrap().progress, None);
	assert_eq!(t.status.get("b").unwrap().kind, None);
}

#[test]
fn invalid_reports_change_nothing() {
	let mut t = terminal();
	send(&mut t, "\x1b]7501;state=working:app=cargo\x07");
	let before = states(&t);
	let reports = [
		"app=x".to_owned(),
		"state=sleeping".to_owned(),
		format!("state=idle:msg={}", b64("a\nb")),
		"state=idle:msg=a".to_owned(),
		format!("state=idle:msg={}", b64(&"x".repeat(2049))),
		format!("state=idle:title={}", b64(&"x".repeat(193))),
		format!("state=idle:app={}", "a".repeat(33)),
		format!("state=idle:{}=1", "k".repeat(17)),
		format!("state=idle:id={}", "a".repeat(33)),
		format!("state=idle:id={}", ["a"; 9].join("/")),
		"state=idle:id=a//b".to_owned(),
		"state=idle:id=a,b".to_owned(),
		format!("state=idle:pad={}", "a".repeat(4100)),
	];
	for report in reports {
		send(&mut t, &format!("\x1b]7501;{report}\x07"));
		assert_eq!(states(&t), before, "{report}");
		assert_eq!(
			t.status.get("").unwrap().app.as_deref(),
			Some("cargo"),
			"{report}"
		);
	}
	// An app outside its character set is only treated as absent.
	send(&mut t, "\x1b]7501;state=idle:app=a+b,c\x07");
	assert_eq!(t.status.get("").unwrap().app, None);
}

#[test]
fn ids_nest_inherit_apps_and_clear_subtrees() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]7501;state=working:app=deploy\x07\
		 \x1b]7501;state=working:id=build\x07\
		 \x1b]7501;state=blocked:id=build/test\x07\
		 \x1b]7501;state=done:id=builder:app=other\x07",
	);
	assert_eq!(t.status.app("build/test"), Some("deploy"));
	assert_eq!(t.status.app("builder"), Some("other"));
	send(&mut t, "\x1b]7501;state=clear:id=build\x07");
	assert_eq!(
		states(&t),
		[
			("".to_owned(), State::Working),
			("builder".to_owned(), State::Done)
		]
	);
	send(&mut t, "\x1b]7501;state=clear\x07");
	assert!(states(&t).is_empty());
}

#[test]
fn prompts_drop_running_records_and_resets_drop_all() {
	let mut t = terminal();
	for (id, state) in ["working", "blocked", "done", "error", "idle"]
		.iter()
		.enumerate()
	{
		send(&mut t, &format!("\x1b]7501;state={state}:id=r{id}\x07"));
	}
	send(&mut t, "\x1b]133;A\x07");
	let kept: Vec<_> = states(&t).into_iter().map(|(_, state)| state).collect();
	assert_eq!(kept, [State::Done, State::Error, State::Idle]);
	// Switching screens and soft resets keep records; a full reset does not.
	send(&mut t, "\x1b[?1049h\x1b[!p");
	assert_eq!(states(&t).len(), 3);
	send(&mut t, "\x1bc");
	assert!(states(&t).is_empty());
}

#[test]
fn records_are_bounded_by_least_recent_update() {
	let mut t = terminal();
	for index in 0..super::status::MAX_RECORDS {
		send(&mut t, &format!("\x1b]7501;state=idle:id=r{index}\x07"));
	}
	send(&mut t, "\x1b]7501;state=working:id=r0\x07");
	send(&mut t, "\x1b]7501;state=idle:id=new\x07");
	let ids: Vec<_> = states(&t).into_iter().map(|(id, _)| id).collect();
	assert_eq!(ids.len(), super::status::MAX_RECORDS);
	assert!(ids.contains(&"r0".to_owned()) && !ids.contains(&"r1".to_owned()));
	assert_eq!(ids.last().map(String::as_str), Some("new"));
}

#[test]
fn feature_detection_is_answered_with_the_queries_terminator() {
	let mut t = terminal();
	send(&mut t, "\x1b]7501;?\x07\x1b]7501;?\x1b\\");
	let reply = "\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";
	assert_eq!(
		t.responses,
		[
			format!("{reply}\x07").into_bytes(),
			format!("{reply}\x1b\\").into_bytes()
		]
	);
}

#[test]
fn osc9_4_reports_progress() {
	let mut t = terminal();
	let progress = |state, percent| Some(Progress { state, percent });
	let cases = [
		("1;50", progress(ProgressState::Normal, Some(50))),
		("1;150", progress(ProgressState::Normal, Some(100))),
		("2", progress(ProgressState::Error, None)),
		("2;75", progress(ProgressState::Error, Some(75))),
		("3", progress(ProgressState::Indeterminate, None)),
		("4;25", progress(ProgressState::Paused, Some(25))),
		("0", None),
		("1;20", progress(ProgressState::Normal, Some(20))),
		("", None),
	];
	for (params, expected) in cases {
		let sequence = if params.is_empty() {
			"\x1b]9;4\x07".to_owned()
		} else {
			format!("\x1b]9;4;{params}\x07")
		};
		send(&mut t, &sequence);
		assert_eq!(t.status.progress, expected, "{params}");
	}
	// Unknown states change nothing.
	send(&mut t, "\x1b]9;4;1;10\x07\x1b]9;4;9;50\x07");
	assert_eq!(t.status.progress, progress(ProgressState::Normal, Some(10)));
	assert!(t.notifications.requests.is_empty());
}

#[test]
fn finished_records_go_once_seen_and_progress_is_summarized() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b]7501;state=done:id=a\x07\x1b]7501;state=error:id=b\x07\
		 \x1b]7501;state=blocked:id=c:progress=30\x07\x1b]7501;state=idle\x07",
	);
	assert_eq!(t.status.summary(), Some(State::Blocked));
	assert_eq!(
		t.status.shown_progress(),
		Some(Progress {
			state: ProgressState::Paused,
			percent: Some(30)
		})
	);
	// OSC 9;4 takes precedence.
	send(&mut t, "\x1b]9;4;1;80\x07");
	assert_eq!(t.status.shown_progress().unwrap().percent, Some(80));
	t.status.acknowledge();
	let left: Vec<_> = states(&t).into_iter().map(|(id, _)| id).collect();
	assert_eq!(left, ["c", ""]);
}
