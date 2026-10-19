//! OSC 22: set (`=` or nothing), push (`>`), pop (`<`) or query (`?`) the
//! pointer shape.
use crate::term::{
	Term,
	pointer::{NAMES, shape},
};

impl Term {
	pub(super) fn osc_pointer(&mut self, value: &[u8], bell: bool) {
		let alternate = self.primary_screen.is_some();
		let (operation, names) = match value.first() {
			Some(&first @ (b'=' | b'>' | b'<' | b'?')) => (first, &value[1..]),
			_ => (b'=', value),
		};
		let mut names = names.split(|&byte| byte == b',');
		match operation {
			b'>' => {
				for shape in names.filter_map(shape) {
					self.pointer_shapes.push(alternate, shape);
				}
			}
			b'<' => {
				self.pointer_shapes.stack(alternate).pop();
			}
			b'?' => {
				let answers: Vec<&str> = names
					.map(|name| match name {
						b"__current__" => self.pointer_shapes.current(alternate).unwrap_or("0"),
						b"__default__" | b"__grabbed__" => NAMES[4],
						name if shape(name).is_some() => "1",
						_ => "0",
					})
					.collect();
				self.osc_reply(format!("22;{}", answers.join(",")), bell);
			}
			_ => {
				let name = names.next().unwrap_or_default();
				let stack = self.pointer_shapes.stack(alternate);
				if name.is_empty() {
					stack.clear();
				} else if let Some(shape) = shape(name) {
					stack.pop();
					stack.push(shape);
				}
			}
		}
	}
}
