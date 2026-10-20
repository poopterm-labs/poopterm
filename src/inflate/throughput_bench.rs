//! Inflate throughput against `miniz_oxide`, on the kinds of streams the
//! kitty graphics protocol carries.
use std::{
	hint::black_box,
	time::{Duration, Instant},
};

use super::{
	tests::{compress, sample},
	zlib,
};
use crate::bench::corpus::Rng;

/// RGB pixels like those of the kitty benchmark workloads.
fn gradient(side: usize) -> Vec<u8> {
	(0..side * side)
		.flat_map(|i| {
			let (x, y) = (i % side, i / side);
			[
				(x * 255 / side) as u8,
				(y * 255 / side) as u8,
				(x ^ y) as u8,
			]
		})
		.collect()
}

/// The best of five runs of decoding about 64 MiB.
fn best(size: usize, decode: &dyn Fn()) -> Duration {
	let reps = ((1 << 26) / size.max(1)).max(1);
	(0..5)
		.map(|_| {
			let start = Instant::now();
			for _ in 0..reps {
				decode();
			}
			start.elapsed() / reps as u32
		})
		.min()
		.unwrap()
}

/// Run with `cargo test --release -- --ignored --nocapture inflate_throughput`.
#[test]
#[ignore = "benchmark; run explicitly in release mode"]
fn inflate_throughput() {
	let mut rng = Rng::new(7);
	let mut inputs: Vec<(String, Vec<u8>)> = [32, 64, 128, 256]
		.into_iter()
		.map(|side| (format!("gradient {side}x{side}"), gradient(side)))
		.collect();
	for (kind, name) in ["random", "words", "runs", "periods"]
		.into_iter()
		.enumerate()
	{
		inputs.push((format!("{name} 1 MiB"), sample(&mut rng, kind, 1 << 20)));
	}
	println!(
		"{:<20} {:>9} {:>12} {:>12}",
		"input", "stream", "ours MB/s", "miniz MB/s"
	);
	for (name, data) in inputs {
		let stream = compress(&data, 6);
		let size = data.len();
		assert_eq!(zlib(&stream, size, size).as_ref(), Some(&data));
		let ours = best(size, &|| {
			_ = black_box(zlib(black_box(&stream), size, size))
		});
		let miniz = best(size, &|| {
			_ = black_box(miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
				black_box(&stream),
				size,
			))
		});
		let rate = |time: Duration| size as f64 / time.as_secs_f64() / 1e6;
		println!(
			"{name:<20} {:>9} {:>12.0} {:>12.0}",
			stream.len(),
			rate(ours),
			rate(miniz)
		);
	}
}
