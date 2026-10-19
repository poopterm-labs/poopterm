//! OSC 3008 hierarchical contexts (UAPI.15): who is in control of the
//! terminal, nested from boot or login down to the running command.

/// Deepest nesting kept; later contexts are ignored, earlier ones kept.
pub(crate) const MAX_DEPTH: usize = 32;
/// Longest context id, in characters.
const MAX_ID: usize = 64;
/// Longest field value, in characters.
const MAX_FIELD: usize = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
	Boot,
	Container,
	Vm,
	Elevate,
	Chpriv,
	Subcontext,
	Remote,
	Shell,
	Command,
	App,
	Service,
	Session,
}

impl Kind {
	fn parse(name: &str) -> Option<Self> {
		Some(match name {
			"boot" => Self::Boot,
			"container" => Self::Container,
			"vm" => Self::Vm,
			"elevate" => Self::Elevate,
			"chpriv" => Self::Chpriv,
			"subcontext" => Self::Subcontext,
			"remote" => Self::Remote,
			"shell" => Self::Shell,
			"command" => Self::Command,
			"app" => Self::App,
			"service" => Self::Service,
			"session" => Self::Session,
			_ => return None,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Context {
	pub id: String,
	pub kind: Option<Kind>,
	/// Valid metadata fields other than `type`, unescaped.
	fields: Vec<(&'static str, String)>,
}

impl Context {
	pub(crate) fn field(&self, name: &str) -> Option<&str> {
		self
			.fields
			.iter()
			.find(|(field, _)| *field == name)
			.map(|(_, value)| value.as_str())
	}
}

/// The open contexts, outermost first. Resets leave them alone, so a
/// program cannot hide the contexts above it.
#[derive(Clone, Default)]
pub(crate) struct Contexts {
	stack: Vec<Context>,
}

impl Contexts {
	#[cfg(test)]
	pub(crate) fn stack(&self) -> &[Context] {
		&self.stack
	}

	/// Applies `start=ID` or `end=ID` and its fields.
	pub(crate) fn apply(&mut self, params: &[&[u8]]) {
		let Some((command, id)) = params.first().and_then(|first| {
			let first = std::str::from_utf8(first).ok()?;
			first.split_once('=')
		}) else {
			return;
		};
		let Some(id) = unescape(id).filter(|id| (1..=MAX_ID).contains(&id.chars().count())) else {
			return;
		};
		let found = self.stack.iter().position(|context| context.id == id);
		match command {
			"start" => {
				let context = parse(id, &params[1..]);
				match found {
					// Starting again updates the context and ends its children.
					Some(index) => {
						self.stack.truncate(index);
						self.stack.push(context);
					}
					None if self.stack.len() < MAX_DEPTH => self.stack.push(context),
					None => {}
				}
			}
			"end" => {
				if let Some(index) = found {
					self.stack.truncate(index);
				}
			}
			_ => {}
		}
	}

	/// Where the terminal's user acts, when that is not their own session:
	/// another user, a remote host, a container or a VM.
	pub(crate) fn label(&self) -> Option<String> {
		self.stack.iter().rev().find_map(|context| {
			let field = |name| context.field(name).map(str::to_owned);
			match context.kind? {
				Kind::Elevate => Some(field("targetuser").unwrap_or_else(|| "root".into())),
				Kind::Chpriv => field("targetuser"),
				Kind::Remote => match (field("targetuser"), field("targethost")) {
					(Some(user), Some(host)) => Some(format!("{user}@{host}")),
					(user, host) => host.or(user),
				},
				Kind::Container => field("container"),
				Kind::Vm => field("vm"),
				_ => None,
			}
		})
	}
}

/// A context from its id and the valid fields among `fields`.
fn parse(id: String, fields: &[&[u8]]) -> Context {
	let mut context = Context {
		id,
		kind: None,
		fields: Vec::new(),
	};
	for field in fields {
		let Some((name, value)) = std::str::from_utf8(field)
			.ok()
			.and_then(|field| field.split_once('='))
		else {
			continue;
		};
		let Some(value) = unescape(value) else {
			continue;
		};
		if name == "type" {
			context.kind = Kind::parse(&value);
			continue;
		}
		let Some(name) = NAMES.iter().find(|known| **known == name) else {
			continue;
		};
		if valid(name, &value) {
			context.fields.retain(|(field, _)| field != name);
			context.fields.push((name, value));
		}
	}
	context
}

/// Fields of the start sequence besides `type`.
const NAMES: [&str; 14] = [
	"user",
	"hostname",
	"machineid",
	"bootid",
	"pid",
	"pidfdid",
	"comm",
	"cwd",
	"cmdline",
	"vm",
	"container",
	"targetuser",
	"targethost",
	"sessionid",
];

fn valid(name: &str, value: &str) -> bool {
	let length = value.chars().count();
	match name {
		"machineid" | "bootid" => {
			(32..=36).contains(&length) && value.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
		}
		"pid" | "pidfdid" => value.parse::<u64>().is_ok() && value.bytes().all(|b| b.is_ascii_digit()),
		"cmdline" => length <= MAX_FIELD,
		_ => (1..=MAX_FIELD).contains(&length),
	}
}

/// Undoes the `\x3b` and `\x5c` escapes, the only ones allowed. Control
/// characters are refused.
fn unescape(text: &str) -> Option<String> {
	let mut out = String::with_capacity(text.len());
	let mut rest = text;
	while let Some(at) = rest.find('\\') {
		out.push_str(&rest[..at]);
		// ABNF strings ignore case: `\X3B` is `\x3b`.
		let escape = rest.get(at..at + 4)?.to_ascii_lowercase();
		out.push(match escape.as_str() {
			"\\x3b" => ';',
			"\\x5c" => '\\',
			_ => return None,
		});
		rest = &rest[at + 4..];
	}
	out.push_str(rest);
	(!out.chars().any(char::is_control)).then_some(out)
}
