//! Transmitted kitty images by id and number, with a memory quota enforced
//! by evicting the least recently used images.
use std::sync::Arc;

use crate::graphics::Bitmap;

/// Decoded bytes kept per terminal before old images are evicted.
pub(crate) const QUOTA: usize = 320 << 20;

#[derive(Clone)]
pub(crate) struct Image {
	pub id: u32,
	/// The client's image number (`I`), or 0.
	pub number: u32,
	pub bitmap: Arc<Bitmap>,
	/// When the image was last transmitted or placed, for eviction.
	used: u64,
}

#[derive(Clone)]
pub(crate) struct Store {
	images: Vec<Image>,
	quota: usize,
	bytes: usize,
	clock: u64,
	/// Next id handed to images sent with only a number; counts down from
	/// the top so it rarely meets ids that clients choose.
	next_id: u32,
}

impl Default for Store {
	fn default() -> Self {
		Self::with_quota(QUOTA)
	}
}

impl Store {
	pub(crate) fn with_quota(quota: usize) -> Self {
		Self {
			images: Vec::new(),
			quota,
			bytes: 0,
			clock: 0,
			next_id: u32::MAX,
		}
	}

	#[cfg(test)]
	pub(crate) fn len(&self) -> usize {
		self.images.len()
	}

	#[cfg(test)]
	pub(crate) fn bytes(&self) -> usize {
		self.bytes
	}

	pub(crate) fn get(&self, id: u32) -> Option<&Image> {
		self.images.iter().find(|image| image.id == id)
	}

	pub(crate) fn ids(&self) -> impl Iterator<Item = u32> {
		self.images.iter().map(|image| image.id)
	}

	/// The newest image with this number.
	pub(crate) fn by_number(&self, number: u32) -> Option<&Image> {
		self
			.images
			.iter()
			.rev()
			.find(|image| image.number == number)
	}

	/// Marks an image as used now.
	pub(crate) fn touch(&mut self, id: u32) {
		self.clock += 1;
		let clock = self.clock;
		if let Some(image) = self.images.iter_mut().find(|image| image.id == id) {
			image.used = clock;
		}
	}

	pub(crate) fn remove(&mut self, id: u32) {
		if let Some(index) = self.images.iter().position(|image| image.id == id) {
			let image = self.images.remove(index);
			self.bytes -= size(&image.bitmap);
		}
	}

	/// Stores an image under `id`, or under a new id when `id` is 0, then
	/// evicts the least recently used other images until the store fits its
	/// quota, preferring images `shown` says are not on screen. Returns the
	/// id and the evicted ids, whose placements the caller removes.
	pub(crate) fn insert(
		&mut self,
		id: u32,
		number: u32,
		bitmap: Arc<Bitmap>,
		shown: impl Fn(u32) -> bool,
	) -> (u32, Vec<u32>) {
		let id = if id == 0 { self.free_id() } else { id };
		self.remove(id);
		self.bytes += size(&bitmap);
		self.clock += 1;
		self.images.push(Image {
			id,
			number,
			bitmap,
			used: self.clock,
		});
		let mut evicted = Vec::new();
		while self.bytes > self.quota {
			let victim = self
				.images
				.iter()
				.filter(|image| image.id != id)
				.min_by_key(|image| (shown(image.id), image.used))
				.map(|image| image.id);
			let Some(victim) = victim else {
				break;
			};
			self.remove(victim);
			evicted.push(victim);
		}
		(id, evicted)
	}

	fn free_id(&mut self) -> u32 {
		loop {
			let id = self.next_id;
			self.next_id = self
				.next_id
				.checked_sub(1)
				.filter(|&id| id != 0)
				.unwrap_or(u32::MAX);
			if self.get(id).is_none() {
				return id;
			}
		}
	}
}

fn size(bitmap: &Bitmap) -> usize {
	bitmap.pixels.len() * 4
}

#[cfg(test)]
mod tests {
	use super::*;

	fn bitmap(pixels: usize) -> Arc<Bitmap> {
		Arc::new(Bitmap::new(pixels, 1, vec![0xff000000; pixels]))
	}

	#[test]
	fn images_are_found_by_id_and_by_newest_number() {
		let mut store = Store::default();
		assert_eq!(store.insert(7, 0, bitmap(1), |_| false).0, 7);
		let (first, _) = store.insert(0, 3, bitmap(2), |_| false);
		let (second, _) = store.insert(0, 3, bitmap(3), |_| false);
		assert!(first != 0 && first != 7 && second != first && second != 7);
		assert_eq!(store.get(7).unwrap().bitmap.width, 1);
		assert_eq!(store.by_number(3).unwrap().id, second);
		assert!(store.by_number(4).is_none());
		// Same id replaces the image.
		store.insert(7, 0, bitmap(5), |_| false);
		assert_eq!(store.get(7).unwrap().bitmap.width, 5);
		assert_eq!(store.len(), 3);
		store.remove(7);
		assert!(store.get(7).is_none());
		assert_eq!(store.bytes(), (2 + 3) * 4);
	}

	#[test]
	fn eviction_keeps_the_quota_and_prefers_unplaced_old_images() {
		let mut store = Store::with_quota(10 * 4);
		store.insert(1, 0, bitmap(4), |_| false);
		store.insert(2, 0, bitmap(4), |_| false);
		store.insert(3, 0, bitmap(1), |_| false);
		store.touch(1);
		// Image 2 is the oldest not shown, image 3 is shown.
		let (_, evicted) = store.insert(4, 0, bitmap(4), |id| id == 3);
		assert_eq!(evicted, [2]);
		assert!(store.bytes() <= 10 * 4);
		// When everything is shown, the least recently used goes anyway.
		let (_, evicted) = store.insert(5, 0, bitmap(6), |_| true);
		assert_eq!(evicted, [3, 1]);
		assert_eq!(store.len(), 2);
	}

	#[test]
	fn an_image_larger_than_the_quota_evicts_everything_but_is_kept() {
		let mut store = Store::with_quota(4);
		store.insert(1, 0, bitmap(1), |_| false);
		let (_, evicted) = store.insert(2, 0, bitmap(2), |_| false);
		assert_eq!(evicted, [1]);
		assert!(store.get(2).is_some());
	}
}
