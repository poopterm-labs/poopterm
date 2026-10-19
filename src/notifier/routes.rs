//! Which session and terminal notification each desktop notification key
//! stands for, so reports reach the program that asked.
/// Most notifications routed at once; the oldest are forgotten.
const MAX_ROUTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Route {
	pub key: u64,
	pub session: u64,
	/// The terminal's token for the notification.
	pub token: u64,
	/// Activating it focuses the session.
	pub focus: bool,
}

#[derive(Default)]
pub(crate) struct Routes {
	routes: Vec<Route>,
	next: u64,
}

impl Routes {
	/// Routes a new notification; returns its key and the key of the one
	/// it replaces, which is forgotten.
	pub(crate) fn add(
		&mut self,
		session: u64,
		token: u64,
		focus: bool,
		replaces: Option<u64>,
	) -> (u64, Option<u64>) {
		let replaced = replaces.and_then(|replaces| {
			let index = self
				.routes
				.iter()
				.position(|route| route.session == session && route.token == replaces)?;
			Some(self.routes.remove(index).key)
		});
		if self.routes.len() >= MAX_ROUTES {
			self.routes.remove(0);
		}
		self.next += 1;
		self.routes.push(Route {
			key: self.next,
			session,
			token,
			focus,
		});
		(self.next, replaced)
	}

	pub(crate) fn get(&self, key: u64) -> Option<Route> {
		self.routes.iter().find(|route| route.key == key).copied()
	}

	/// The key of the terminal notification `token` of `session`.
	pub(crate) fn key(&self, session: u64, token: u64) -> Option<u64> {
		self
			.routes
			.iter()
			.find(|route| route.session == session && route.token == token)
			.map(|route| route.key)
	}

	pub(crate) fn remove(&mut self, key: u64) {
		self.routes.retain(|route| route.key != key);
	}

	/// Forgets the notifications of a closed session.
	pub(crate) fn remove_session(&mut self, session: u64) {
		self.routes.retain(|route| route.session != session);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn keys_route_back_to_sessions_and_replace_per_session() {
		let mut routes = Routes::default();
		let (a, _) = routes.add(1, 10, true, None);
		let (b, _) = routes.add(2, 10, false, None);
		assert_ne!(a, b);
		assert_eq!(
			routes.get(b).map(|route| (route.session, route.focus)),
			Some((2, false))
		);
		// Token 10 of session 1 is replaced, not session 2's.
		let (c, replaced) = routes.add(1, 11, true, Some(10));
		assert_eq!(replaced, Some(a));
		assert_eq!(
			(routes.get(a), routes.key(1, 11), routes.key(2, 10)),
			(None, Some(c), Some(b))
		);
		routes.remove_session(2);
		assert_eq!(routes.get(b), None);
		routes.remove(c);
		assert_eq!(routes.get(c), None);
		for token in 0..1000 {
			routes.add(3, token, true, None);
		}
		assert_eq!(routes.routes.len(), MAX_ROUTES);
	}
}
