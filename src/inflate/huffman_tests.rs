//! Huffman table tests, against the canonical code of RFC 1951 section 3.2.2.
use super::huffman::{BASE, END, INVALID, KIND, LITERAL, Table, extra, length};
use crate::bench::corpus::Rng;

fn plain(symbol: usize) -> u32 {
	LITERAL | (symbol as u32) << 16
}

/// Each symbol's code, as the RFC assigns them: shorter codes first, and
/// codes of one length in symbol order.
pub(super) fn canonical(lengths: &[u8]) -> Vec<Option<u32>> {
	let mut next = 0;
	let mut codes = vec![None; lengths.len()];
	for length in 1..=15 {
		for (symbol, _) in lengths.iter().enumerate().filter(|(_, l)| **l == length) {
			codes[symbol] = Some(next);
			next += 1;
		}
		next <<= 1;
	}
	codes
}

/// The bits a code is sent as: its top bit first, so reversed.
fn sent(code: u32, length: u8) -> u64 {
	u64::from(code.reverse_bits() >> (32 - length))
}

/// Lengths of a random complete code over `symbols` symbols, made by
/// splitting random leaves of a tree of one code.
pub(super) fn complete(rng: &mut Rng, symbols: usize, codes: usize) -> Vec<u8> {
	let mut leaves = vec![1u8, 1];
	while leaves.len() < codes {
		let leaf = rng.below(leaves.len());
		if leaves[leaf] < 15 {
			leaves[leaf] += 1;
			leaves.push(leaves[leaf]);
		}
	}
	let mut lengths = vec![0; symbols];
	for length in leaves {
		let mut symbol = rng.below(symbols);
		while lengths[symbol] != 0 {
			symbol = (symbol + 1) % symbols;
		}
		lengths[symbol] = length;
	}
	lengths
}

/// Checks every code in `table` decodes to its symbol, with any bits after it.
fn decodes<const SIZE: usize>(rng: &mut Rng, table: &Table<SIZE>, lengths: &[u8]) {
	for (symbol, code) in canonical(lengths).into_iter().enumerate() {
		let Some(code) = code else { continue };
		let length = lengths[symbol];
		let after = (rng.next() & 0xff_ffff) << length;
		let entry = table.entry(sent(code, length) | after);
		assert_eq!(
			(entry >> 16, super::huffman::length(entry)),
			(symbol as u32, u32::from(length)),
			"symbol {symbol}, code {code:b} of {length} bits"
		);
		assert_eq!(
			table.resolve(
				table.first(sent(code, length) | after),
				sent(code, length) | after
			),
			entry
		);
	}
}

#[test]
fn random_complete_codes_decode_every_symbol() {
	let mut rng = Rng::new(31);
	for _ in 0..300 {
		let codes = 2 + rng.below(286);
		let lengths = complete(&mut rng, 288, codes);
		let mut table = Table::<1024>::new();
		assert!(table.build(&lengths, plain).is_some(), "{lengths:?}");
		decodes(&mut rng, &table, &lengths);
	}
}

#[test]
fn smaller_tables_decode_the_same_codes() {
	let mut rng = Rng::new(32);
	for _ in 0..100 {
		let codes = 2 + rng.below(30);
		let distances = complete(&mut rng, 32, codes);
		let mut table = Table::<256>::new();
		assert!(table.build(&distances, plain).is_some());
		decodes(&mut rng, &table, &distances);
		let codes = 2 + rng.below(17);
		let code_lengths = complete(&mut rng, 19, codes);
		let mut table = Table::<128>::new();
		assert!(table.build(&code_lengths, plain).is_some());
		decodes(&mut rng, &table, &code_lengths);
	}
}

#[test]
fn over_subscribed_codes_are_refused() {
	let mut rng = Rng::new(33);
	let mut table = Table::<1024>::new();
	assert!(table.build(&[1, 1, 1], plain).is_none());
	// One code too many at any length.
	for _ in 0..100 {
		let codes = 2 + rng.below(200);
		let mut lengths = complete(&mut rng, 288, codes);
		let unused = lengths.iter().position(|&l| l == 0).unwrap();
		lengths[unused] = 1 + rng.below(15) as u8;
		assert!(table.build(&lengths, plain).is_none(), "{lengths:?}");
	}
	// Two codes of fifteen bits past a complete code.
	let mut lengths = vec![1, 1, 15, 15];
	assert!(table.build(&lengths, plain).is_none());
	lengths.truncate(2);
	assert!(table.build(&lengths, plain).is_some());
}

#[test]
fn incomplete_codes_are_refused_unless_single() {
	let mut table = Table::<1024>::new();
	assert!(table.build(&[2, 2, 2], plain).is_none());
	assert!(table.build(&[1, 2], plain).is_none());
	assert!(table.build(&[15, 15], plain).is_none());
	let mut rng = Rng::new(34);
	for _ in 0..100 {
		// Drop one code from a complete code of three or more.
		let codes = 3 + rng.below(200);
		let mut lengths = complete(&mut rng, 288, codes);
		let used: Vec<usize> = (0..288).filter(|&s| lengths[s] != 0).collect();
		lengths[*rng.pick(&used)] = 0;
		assert!(table.build(&lengths, plain).is_none(), "{lengths:?}");
	}
}

#[test]
fn a_single_code_decodes_and_its_sibling_is_invalid() {
	let mut table = Table::<1024>::new();
	assert!(table.build(&[0, 1], plain).is_some());
	assert_eq!(table.entry(0), plain(1) | 1);
	assert_eq!(table.entry(1) & KIND, INVALID);
	// One code longer than the primary table covers.
	let mut lengths = [0u8; 20];
	lengths[7] = 12;
	assert!(table.build(&lengths, plain).is_some());
	assert_eq!(table.entry(0), plain(7) | 12);
	assert_eq!(table.entry(1 << 11) & KIND, INVALID);
	assert_eq!(table.entry(1) & KIND, INVALID);
	// No codes at all: nothing decodes.
	assert!(table.build(&[0; 30], plain).is_some());
	assert_eq!(table.entry(0) & KIND, INVALID);
	assert_eq!(table.entry(0x7fff) & KIND, INVALID);
}

#[test]
fn codes_of_every_length_resolve() {
	let mut table = Table::<1024>::new();
	// Lengths 1 to 15, then 15 again: each code is ones and a final zero.
	let lengths: Vec<u8> = (1..=15).chain([15]).collect();
	assert!(table.build(&lengths, plain).is_some());
	for (symbol, &bits) in lengths.iter().enumerate().take(15) {
		let entry = table.entry((1 << symbol) - 1);
		assert_eq!(
			(entry >> 16, length(entry)),
			(symbol as u32, u32::from(bits))
		);
	}
	assert_eq!(table.entry(0x7fff) >> 16, 15);
}

#[test]
fn only_long_codes_go_through_a_subtable() {
	let mut table = Table::<1024>::new();
	let lengths: Vec<u8> = (1..=15).chain([15]).collect();
	assert!(table.build(&lengths, plain).is_some());
	// Codes up to ten bits are in the primary table as they are.
	for symbol in 0..10 {
		let bits = (1 << symbol) - 1;
		assert_eq!(table.first(bits), table.entry(bits), "symbol {symbol}");
	}
	// Longer ones link to a subtable, which is not a literal.
	for symbol in 10..15 {
		let bits = (1 << symbol) - 1;
		assert_ne!(table.first(bits) & KIND, LITERAL, "symbol {symbol}");
		assert_eq!(table.entry(bits) >> 16, symbol as u32);
	}
}

#[test]
fn rebuilding_forgets_the_previous_code() {
	let mut rng = Rng::new(35);
	let mut table = Table::<1024>::new();
	let long: Vec<u8> = (1..=15).chain([15]).collect();
	assert!(table.build(&long, plain).is_some());
	// A short code over fewer symbols leaves nothing of the long one.
	assert!(table.build(&[0, 1], plain).is_some());
	for bits in [0x3ff, 0x7fff, 0x1ff] {
		assert_eq!(table.entry(bits) & KIND, INVALID, "{bits:x}");
	}
	for _ in 0..50 {
		let codes = 2 + rng.below(286);
		let lengths = complete(&mut rng, 288, codes);
		assert!(table.build(&lengths, plain).is_some());
		decodes(&mut rng, &table, &lengths);
	}
}

#[test]
fn entries_carry_their_kind_value_and_extra_bits() {
	let mut table = Table::<256>::new();
	let entry = |symbol: usize| match symbol {
		0 => END,
		1 => BASE | 300 << 16 | 13 << 8,
		_ => INVALID,
	};
	assert!(table.build(&[1, 2, 2], entry).is_some());
	let base = table.entry(0b01);
	assert_eq!(
		(base & KIND, base >> 16, extra(base), length(base)),
		(BASE, 300, 13, 2)
	);
	let end = table.entry(0b0);
	assert_eq!((end & KIND, length(end)), (END, 1));
	assert_eq!(table.entry(0b11) & KIND, INVALID);
}

#[test]
fn rebuilding_keeps_the_subtables_bounded() {
	// Every dynamic block rebuilds the tables: a stream of many must not
	// grow them without limit.
	let mut table = Table::<1024>::new();
	let long: Vec<u8> = (1..=15).chain([15]).collect();
	assert!(table.build(&long, plain).is_some());
	let size = table.subtable_len();
	assert!(size > 0);
	for _ in 0..1000 {
		assert!(table.build(&long, plain).is_some());
	}
	assert_eq!(table.subtable_len(), size);
	assert!(table.build(&[1, 1], plain).is_some());
	assert_eq!(table.subtable_len(), 0);
}
