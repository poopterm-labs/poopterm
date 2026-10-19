//! Headless benchmarks over a corpus of real-world workloads.
//!
//! `poopterm --benchmark` feeds every workload through the parser and
//! terminal, with and without rendering frames, and prints MB/s. It needs no
//! window, so it is also the training run for profile-guided optimization.
//! `poopterm --bench-corpus NAME` prints a workload's bytes, so other
//! terminals can be measured on identical input (`scripts/bench-throughput.py`).
pub(crate) mod corpus;
mod images;

use crate::{
	parser::Parser,
	render::{
		glyphs::FontSet,
		grid::{GridView, draw_grid},
		painter::Painter,
	},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};
use corpus::{Size, WORKLOADS, Workload};
use std::{error::Error, io::Write, time::Instant};

/// Bytes per `advance` call, like one PTY read.
const READ: usize = 64 << 10;
/// Bytes between rendered frames: a 200 MB/s flood drawn at 30 fps is about
/// 6.7 MB per frame; interactive programs draw far more often. This is in
/// between, and keeps rendering in the PGO profile.
const FRAME_EVERY: usize = 1 << 20;
const CELL: (usize, usize) = (10, 20);

struct Options {
	/// Bytes per workload.
	bytes: usize,
	size: Size,
	only: Vec<String>,
	render: bool,
}

fn options(args: impl Iterator<Item = String>) -> Result<Options, Box<dyn Error>> {
	let mut options = Options {
		bytes: 16 << 20,
		size: Size {
			cols: 120,
			rows: 40,
		},
		only: Vec::new(),
		render: true,
	};
	let mut args = args.peekable();
	while let Some(arg) = args.next() {
		let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
		match arg.as_str() {
			"--mib" => options.bytes = value()?.parse::<usize>()?.clamp(1, 4096) << 20,
			"--kib" => options.bytes = value()?.parse::<usize>()?.clamp(1, 1 << 22) << 10,
			"--size" => {
				let value = value()?;
				let (cols, rows) = value.split_once('x').ok_or("--size is COLSxROWS")?;
				options.size = Size {
					cols: cols.parse::<usize>()?.clamp(2, 1000),
					rows: rows.parse::<usize>()?.clamp(2, 1000),
				};
			}
			"--only" => options.only.extend(value()?.split(',').map(str::to_owned)),
			"--no-render" => options.render = false,
			"--list" => {
				for workload in WORKLOADS {
					println!("{:<11} {}", workload.name, workload.about);
				}
				std::process::exit(0);
			}
			_ => return Err(format!("unknown benchmark option: {arg}").into()),
		}
	}
	for name in &options.only {
		corpus::find(name).ok_or_else(|| format!("unknown workload: {name}; try --list"))?;
	}
	Ok(options)
}

fn fonts() -> FontSet {
	let font = fontdue::Font::from_bytes(
		include_bytes!("../../assets/font.ttf") as &[u8],
		fontdue::FontSettings::default(),
	)
	.expect("bundled font is valid");
	FontSet::new(font, Vec::new())
}

/// Feeds `data` to a fresh terminal; draws a frame every [`FRAME_EVERY`]
/// bytes when `fonts` is given. Returns MB/s.
pub(crate) fn measure(data: &[u8], size: Size, mut fonts: Option<&mut FontSet>) -> f64 {
	let theme = Theme::from_kind(ThemeKind::Poop);
	let mut term = Term::new(size.cols, size.rows, theme, "/".into());
	(term.cell_width, term.cell_height) = CELL;
	let mut parser = Parser::new();
	let mut frame = Frame::default();
	let mut pixels = vec![0u32; size.cols * CELL.0 * size.rows * CELL.1];
	let mut since_frame = 0;
	let start = Instant::now();
	for chunk in data.chunks(READ) {
		parser.advance(&mut term, chunk);
		term.responses.clear();
		since_frame += chunk.len();
		if let Some(fonts) = fonts.as_deref_mut()
			&& since_frame >= FRAME_EVERY
		{
			since_frame = 0;
			term.snapshot(&mut frame);
			draw(&mut pixels, fonts, &frame, theme.bg);
		}
	}
	data.len() as f64 / start.elapsed().as_secs_f64() / 1e6
}

fn draw(pixels: &mut [u32], fonts: &mut FontSet, frame: &Frame, bg: u32) {
	let (width, height) = (frame.cols * CELL.0, frame.rows * CELL.1);
	let mut painter = Painter::new(pixels, width, height);
	painter.fill(bg);
	let view = GridView {
		frame,
		origin: (0, 0),
		cell: CELL,
		px: 16.0,
		ascent: 15,
		scale: 1,
		default_bg: bg,
		link_color: 0x89b4fa,
		selection_colors: (0x1e1e2e, 0x89b4fa),
		match_colors: (bg, 0xf9e2af),
		selection: None,
		search_match: None,
		focused: true,
	};
	draw_grid(&mut painter, fonts, &view, None);
}

fn selected(options: &Options) -> impl Iterator<Item = &'static Workload> + '_ {
	WORKLOADS.iter().filter(|workload| {
		options.only.is_empty() || options.only.iter().any(|name| name == workload.name)
	})
}

/// `poopterm --benchmark [--mib N] [--size COLSxROWS] [--only a,b] [--no-render] [--list]`
pub(crate) fn run(args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
	let options = options(args)?;
	let mut fonts = fonts();
	println!(
		"poopterm {} headless, {}x{}, {} KiB per workload, MB/s",
		env!("CARGO_PKG_VERSION"),
		options.size.cols,
		options.size.rows,
		options.bytes >> 10
	);
	println!(
		"{:<11} {:>9} {:>9}",
		"workload",
		"parse",
		if options.render { "+render" } else { "" }
	);
	let (mut total_bytes, mut total_seconds) = (0.0, 0.0);
	for workload in selected(&options) {
		let data = workload.build(options.bytes, options.size);
		measure(&data[..data.len() / 8], options.size, None);
		let parse = measure(&data, options.size, None);
		total_bytes += data.len() as f64;
		total_seconds += data.len() as f64 / parse / 1e6;
		let rendered = if options.render {
			format!("{:9.1}", measure(&data, options.size, Some(&mut fonts)))
		} else {
			String::new()
		};
		println!("{:<11} {parse:9.1} {rendered}", workload.name);
	}
	// The geometric view would hide slow workloads; this is total bytes over total time.
	println!(
		"{:<11} {:9.1}",
		"overall",
		total_bytes / total_seconds / 1e6
	);
	Ok(())
}

/// `poopterm --bench-corpus NAME [--mib N | --kib N] [--size COLSxROWS]`: the bytes of one workload.
pub(crate) fn print_corpus(mut args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
	let name = args
		.next()
		.ok_or("missing workload name; try --benchmark --list")?;
	let workload = corpus::find(&name).ok_or_else(|| format!("unknown workload: {name}"))?;
	let options = options(args)?;
	let data = workload.build(options.bytes, options.size);
	std::io::stdout().lock().write_all(&data)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Parser and terminal only, every workload. Run with
	/// `cargo test --release -- --ignored --nocapture parser_throughput`;
	/// `poopterm --benchmark` reports the same plus rendering.
	#[test]
	#[ignore = "benchmark; run explicitly in release mode"]
	fn parser_throughput() {
		let size = Size {
			cols: 200,
			rows: 50,
		};
		for workload in WORKLOADS {
			let data = workload.build(32 << 20, size);
			println!(
				"parser {:<11} {:8.1} MB/s",
				workload.name,
				measure(&data, size, None)
			);
		}
	}

	/// Every workload parses and renders without panicking, at a small size.
	#[test]
	fn every_workload_runs_headless() {
		let mut fonts = fonts();
		let size = Size { cols: 40, rows: 12 };
		for workload in WORKLOADS {
			let data = workload.build(FRAME_EVERY + 1, size);
			assert!(
				measure(&data, size, Some(&mut fonts)) > 0.0,
				"{}",
				workload.name
			);
		}
	}
}
