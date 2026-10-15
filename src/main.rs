use std::{
    collections::HashMap,
    env,
    error::Error,
    io::{Read, Write},
    num::NonZeroU32,
    sync::Arc,
    thread,
};

use fontdue::{Font, FontSettings, Metrics};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use vte::{Params, Parser, Perform};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;

// ---- Poop palette 💩 -------------------------------------------------------

const DEFAULT_FG: u32 = 0xE8D5B0; // cream
const DEFAULT_BG: u32 = 0x24170B; // dark chocolate
const BORDER_COL: u32 = 0x7A4E1F; // caramel rim
const PALETTE: [u32; 8] = [
    0x1E1409, // black
0xD9534F, // red
0x8BC34A, // green
0xE0B040, // yellow
0x5C9BD6, // blue
0xB06AC8, // magenta
0x4FB6B0, // cyan
0xE8D5B0, // white
];

// ---- Terminal state --------------------------------------------------------

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: u32,
    bg: u32,
}

const BLANK: Cell = Cell { ch: ' ', fg: DEFAULT_FG, bg: DEFAULT_BG };

struct Term {
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
    cx: usize,
    cy: usize,
    fg: u32,
    bg: u32,
}

impl Term {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![BLANK; cols * rows],
            cx: 0,
            cy: 0,
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
        }
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.cols + x
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        let mut new = vec![BLANK; cols * rows];
        let (old_cols, old_rows) = (self.cols, self.rows);
        for y in 0..rows.min(old_rows) {
            for x in 0..cols.min(old_cols) {
                new[y * cols + x] = self.cells[y * old_cols + x];
            }
        }
        self.cells = new;
        self.cols = cols;
        self.rows = rows;
        self.cx = self.cx.min(cols - 1);
        self.cy = self.cy.min(rows - 1);
    }

    fn scroll_up(&mut self) {
        self.cells.drain(0..self.cols);
        self.cells.extend(std::iter::repeat(BLANK).take(self.cols));
    }

    fn linefeed(&mut self) {
        if self.cy + 1 < self.rows {
            self.cy += 1;
        } else {
            self.scroll_up();
        }
    }

    fn clear_range(&mut self, from: usize, to: usize) {
        for i in from..to.min(self.cells.len()) {
            self.cells[i] = Cell { ch: ' ', fg: DEFAULT_FG, bg: DEFAULT_BG };
        }
    }

    fn sgr(&mut self, params: &Params) {
        if params.is_empty() {
            self.fg = DEFAULT_FG;
            self.bg = DEFAULT_BG;
            return;
        }
        for param in params.iter() {
            match param[0] {
                0 => {
                    self.fg = DEFAULT_FG;
                    self.bg = DEFAULT_BG;
                }
                30..=37 => self.fg = PALETTE[(param[0] - 30) as usize],
                39 => self.fg = DEFAULT_FG,
                40..=47 => self.bg = PALETTE[(param[0] - 40) as usize],
                49 => self.bg = DEFAULT_BG,
                90..=97 => self.fg = PALETTE[(param[0] - 90) as usize],
                _ => {}
            }
        }
    }
}

impl Perform for Term {
    fn print(&mut self, c: char) {
        if self.cx >= self.cols {
            self.cx = 0;
            self.linefeed();
        }
        let i = self.idx(self.cx, self.cy);
        self.cells[i] = Cell { ch: c, fg: self.fg, bg: self.bg };
        self.cx += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => self.linefeed(),
            b'\r' => self.cx = 0,
            0x08 => self.cx = self.cx.saturating_sub(1),
            b'\t' => self.cx = (((self.cx / 8) + 1) * 8).min(self.cols - 1),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _: &[u8], _: bool, action: char) {
        // First value of the i-th param, with 0 treated as "default".
        let p = |i: usize, d: u16| {
            params
            .iter()
            .nth(i)
            .and_then(|s| s.first().copied())
            .filter(|&v| v != 0)
            .unwrap_or(d) as usize
        };
        let raw0 = params.iter().next().and_then(|s| s.first().copied()).unwrap_or(0);

        match action {
            'A' => self.cy = self.cy.saturating_sub(p(0, 1)),
            'B' => self.cy = (self.cy + p(0, 1)).min(self.rows - 1),
            'C' => self.cx = (self.cx + p(0, 1)).min(self.cols - 1),
            'D' => self.cx = self.cx.saturating_sub(p(0, 1)),
            'G' => self.cx = (p(0, 1) - 1).min(self.cols - 1),
            'H' | 'f' => {
                self.cy = (p(0, 1) - 1).min(self.rows - 1);
                self.cx = (p(1, 1) - 1).min(self.cols - 1);
            }
            'J' => match raw0 {
                0 => {
                    let from = self.idx(self.cx, self.cy);
                    self.clear_range(from, self.cells.len());
                }
                2 | 3 => self.clear_range(0, self.cells.len()),
                _ => {}
            },
            'K' => {
                let row_start = self.idx(0, self.cy);
                let row_end = row_start + self.cols;
                match raw0 {
                    0 => self.clear_range(self.idx(self.cx, self.cy), row_end),
                    2 => self.clear_range(row_start, row_end),
                    _ => {}
                }
            }
            'm' => self.sgr(params),
            _ => {}
        }
    }
}

// ---- Events from the PTY reader thread --------------------------------------

#[derive(Debug)]
enum UserEvent {
    Output(Vec<u8>),
    Closed,
}

// ---- App ---------------------------------------------------------------------

struct App {
    font: Font,
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
    px: f32,
    cw: usize,
    ch: usize,
    ascent: i32,
    margin: usize,
    top: usize,
    border: usize,

    term: Term,
    parser: Parser,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,

    window: Option<Arc<Window>>,
    _context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    width: u32,
    height: u32,
    ctrl: bool,
}

impl App {
    fn resize(&mut self, w: u32, h: u32) {
        self.width = w.max(1);
        self.height = h.max(1);

        let cols = ((self.width as usize).saturating_sub(2 * self.margin) / self.cw.max(1)).max(2);
        let rows = ((self.height as usize).saturating_sub(self.top + self.margin) / self.ch.max(1)).max(2);

        if cols != self.term.cols || rows != self.term.rows {
            self.term.resize(cols, rows);
            let _ = self.master.resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }

        if let Some(s) = self.surface.as_mut() {
            if let (Some(nw), Some(nh)) = (NonZeroU32::new(self.width), NonZeroU32::new(self.height)) {
                let _ = s.resize(nw, nh);
            }
        }
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }

    fn key_bytes(&self, key: &Key) -> Option<Vec<u8>> {
        match key {
            Key::Named(n) => match n {
                NamedKey::Enter => Some(b"\r".to_vec()),
                NamedKey::Backspace => Some(vec![0x7f]),
                NamedKey::Tab => Some(b"\t".to_vec()),
                NamedKey::Escape => Some(vec![0x1b]),
                NamedKey::Space => Some(b" ".to_vec()),
                NamedKey::ArrowUp => Some(b"\x1b[A".to_vec()),
                NamedKey::ArrowDown => Some(b"\x1b[B".to_vec()),
                NamedKey::ArrowRight => Some(b"\x1b[C".to_vec()),
                NamedKey::ArrowLeft => Some(b"\x1b[D".to_vec()),
                _ => None,
            },
            Key::Character(s) => {
                let s = s.as_str();
                if self.ctrl && s.len() == 1 {
                    let c = s.as_bytes()[0];
                    if c.is_ascii_alphabetic() {
                        return Some(vec![c.to_ascii_lowercase() & 0x1f]);
                    }
                    return None;
                }
                Some(s.as_bytes().to_vec())
            }
            _ => None,
        }
    }

    fn draw(&mut self) {
        let Some(surface) = self.surface.as_mut() else { return };
        let Ok(mut buf) = surface.buffer_mut() else { return };
        let pixels: &mut [u32] = &mut buf;
        let (w, h) = (self.width as usize, self.height as usize);

        // Caramel rim, then chocolate interior.
        pixels.fill(BORDER_COL);
        fill_rect(
            pixels,
            w,
            h,
            self.border,
            self.border,
            w.saturating_sub(2 * self.border),
                  h.saturating_sub(2 * self.border),
                  DEFAULT_BG,
        );

        let (cw, ch, m, top, px, ascent) = (self.cw, self.ch, self.margin, self.top, self.px, self.ascent);
        let font = &self.font;
        let glyphs = &mut self.glyphs;
        let term = &self.term;

        for row in 0..term.rows {
            for col in 0..term.cols {
                let cell = term.cells[row * term.cols + col];
                let is_cursor = row == term.cy && col == term.cx;
                let (fg, bg) = if is_cursor { (cell.bg, cell.fg) } else { (cell.fg, cell.bg) };

                let x0 = m + col * cw;
                let y0 = top + row * ch;
                fill_rect(pixels, w, h, x0, y0, cw, ch, bg);

                if cell.ch == ' ' {
                    continue;
                }
                let (gm, bitmap) = glyphs
                .entry(cell.ch)
                .or_insert_with(|| font.rasterize(cell.ch, px));

                let gx0 = x0 as i32 + gm.xmin;
                let gy0 = y0 as i32 + ascent - gm.ymin - gm.height as i32;
                for gy in 0..gm.height {
                    for gx in 0..gm.width {
                        let a = bitmap[gy * gm.width + gx] as u32;
                        if a == 0 {
                            continue;
                        }
                        let x = gx0 + gx as i32;
                        let y = gy0 + gy as i32;
                        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                            continue;
                        }
                        pixels[y as usize * w + x as usize] = blend(bg, fg, a);
                    }
                }
            }
        }

        let _ = buf.present();
    }
}

fn fill_rect(px: &mut [u32], w: usize, h: usize, x: usize, y: usize, rw: usize, rh: usize, color: u32) {
    for yy in y..(y + rh).min(h) {
        let row = yy * w;
        for xx in x..(x + rw).min(w) {
            px[row + xx] = color;
        }
    }
}

fn blend(bg: u32, fg: u32, a: u32) -> u32 {
    let mix = |shift: u32| {
        let f = (fg >> shift) & 0xff;
        let b = (bg >> shift) & 0xff;
        ((f * a + b * (255 - a)) / 255) << shift
    };
    mix(16) | mix(8) | mix(0)
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
        .with_title("💩 poopterm")
        .with_inner_size(LogicalSize::new(900.0, 560.0));

        // On macOS, keep the native frame so the traffic lights exist,
        // but make the title bar transparent and hide the title text.
        #[cfg(target_os = "macos")]
        let attrs = attrs
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true);

        let window = Arc::new(event_loop.create_window(attrs).expect("failed to create window"));

        let scale = window.scale_factor() as f32;
        self.px = 18.0 * scale;
        self.margin = (8.0 * scale) as usize;
        self.border = (scale * 3.0) as usize;
        self.top = if cfg!(target_os = "macos") {
            (28.0 * scale) as usize // space for the traffic lights
        } else {
            self.margin
        };

        let lm = self.font.horizontal_line_metrics(self.px).expect("font line metrics");
        self.ascent = lm.ascent.round() as i32;
        self.ch = lm.new_line_size.ceil() as usize;
        self.cw = self.font.metrics('M', self.px).advance_width.ceil() as usize;

        let size = PhysicalSize::new(
            (self.cw * self.term.cols + 2 * self.margin) as u32,
                                     (self.ch * self.term.rows + self.top + self.margin) as u32,
        );
        let _ = window.request_inner_size(size);

        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        self.surface = Some(surface);
        self._context = Some(context);
        self.window = Some(window);

        let s = self.window.as_ref().unwrap().inner_size();
        self.resize(s.width, s.height);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.ctrl = m.state().control_key(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(bytes) = self.key_bytes(&event.logical_key) {
                    let _ = self.writer.write_all(&bytes);
                    let _ = self.writer.flush();
                }
            }
            // No title bar, so click-drag anywhere moves the window.
            WindowEvent::MouseInput { state, button, .. }
            if state == ElementState::Pressed && button == MouseButton::Left =>
            {
                if let Some(win) = &self.window {
                    let _ = win.drag_window();
                }
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Output(bytes) => {
                // vte's Parser consumes one byte at a time.
                for &b in bytes.iter() {
                    self.parser.advance(&mut self.term, b);
                }
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            UserEvent::Closed => event_loop.exit(),
        }
    }
}

// ---- main --------------------------------------------------------------------

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let font = Font::from_bytes(include_bytes!("../assets/font.ttf") as &[u8], FontSettings::default())?;

    let (cols, rows) = (100usize, 30usize);
    let pty = native_pty_system();
    let pair = pty.openpty(PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut cmd = CommandBuilder::new(shell);
    cmd.env("TERM", "xterm-256color");
    let _child = pair.slave.spawn_command(cmd)?;
    // The slave must be closed in the parent for EOF to arrive.
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;

    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = proxy.send_event(UserEvent::Closed);
                    break;
                }
                Ok(n) => {
                    if proxy.send_event(UserEvent::Output(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut app = App {
        font,
        glyphs: HashMap::new(),
        px: 18.0,
        cw: 10,
        ch: 20,
        ascent: 15,
        margin: 8,
        top: 8,
        border: 1,
        term: Term::new(cols, rows),
        parser: Parser::new(),
        master: pair.master,
        writer,
        window: None,
        _context: None,
        surface: None,
        width: 0,
        height: 0,
        ctrl: false,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}
