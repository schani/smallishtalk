//! L0 — the host backend for the UI (UI.md §3, §4A).
//!
//! Headless-first: the in-memory ARGB present buffer, a scripted event queue,
//! and a selectable clock are *always* compiled and are the primary run mode.
//! A real `minifb` window (behind the `ui` feature) is just one more presenter
//! over the same buffer — added for live use, never required for development,
//! tests, or agent operation.

use std::collections::VecDeque;

/// One host event, already encoded as the five SmallIntegers the image sees
/// (UI.md §4.2): `[type, a, b, c, d]`. Keeping the encoding in the host means
/// the seam needs no new cross-boundary class.
pub type HostEvent = [i64; 5];

// Event type tags (UI.md §4.2). `EV_NONE`/empty queue is reported as nil.
pub const EV_NONE: i64 = 0;
pub const EV_MOUSE_MOVE: i64 = 1;
pub const EV_MOUSE_BUTTON: i64 = 2;
pub const EV_KEY_DOWN: i64 = 3;
pub const EV_KEY_UP: i64 = 4;
pub const EV_SCROLL: i64 = 5;
pub const EV_RESIZE: i64 = 6;
pub const EV_CLOSE: i64 = 7;

/// The clock behind `primClockMonotonicNs`/`Ms` and (later) timers
/// (UI.md §4A.1). Virtual mode makes headless scenarios perfectly
/// reproducible and profiling repeatable.
pub enum TimeSource {
    /// Wall time, measured from the VM's `start_instant`.
    Real,
    /// Deterministic virtual time in nanoseconds; only the driver advances it.
    Virtual { now_ns: u64 },
}

/// All host-side UI state, held as one field on the `Vm`. None of it survives
/// a snapshot (UI.md §12 item 6): on reload it is recreated and the image
/// forces a full repaint.
pub struct HostUi {
    /// ARGB present buffer — what a window shows, and what golden tests hash.
    /// Sized lazily on the first present.
    pub buffer: Vec<u32>,
    pub buf_width: u32,
    pub buf_height: u32,
    /// Pending host events, oldest first (UI.md §3.2/§4.2).
    pub events: VecDeque<HostEvent>,
    pub clock: TimeSource,
    #[cfg(feature = "ui")]
    pub window: Option<minifb::Window>,
    /// Windowed builds open a real window on first present; headless never does.
    #[cfg(feature = "ui")]
    pub windowed: bool,
}

impl Default for HostUi {
    fn default() -> HostUi {
        HostUi {
            buffer: Vec::new(),
            buf_width: 0,
            buf_height: 0,
            events: VecDeque::new(),
            clock: TimeSource::Real,
            #[cfg(feature = "ui")]
            window: None,
            #[cfg(feature = "ui")]
            windowed: false,
        }
    }
}

impl HostUi {
    pub fn new() -> HostUi {
        HostUi::default()
    }

    // --- Events -------------------------------------------------------------

    /// Enqueue a host event (windowed poll or the headless scripted queue).
    pub fn push_event(&mut self, ev: HostEvent) {
        self.events.push_back(ev);
    }

    /// Pop the oldest event, or `None` if the queue is empty.
    pub fn pop_event(&mut self) -> Option<HostEvent> {
        self.events.pop_front()
    }

    /// Refill the event queue from the live window (windowed builds only).
    /// Headless builds have nothing to poll — events arrive via `push_event`
    /// from the scripted driver — so this is a no-op there. Harvesting here
    /// (not at blit) is deliberate: a static, undamaged screen still receives
    /// input, including the close box (UI.md §3.2).
    pub fn harvest(&mut self) {
        #[cfg(feature = "ui")]
        self.harvest_window();
    }

    // --- Present ------------------------------------------------------------

    /// Present a monochrome (1-bit) form's bits, expanded to ARGB, into the
    /// buffer (and, windowed, the window). `bits` is MSB-first packed rows of
    /// `ceil(width/8)` bytes. Bit set → black, clear → white (UI.md §4.1).
    pub fn present_mono(&mut self, width: u32, height: u32, bits: &[u8]) {
        let stride = width.div_ceil(8) as usize;
        self.buffer.clear();
        self.buffer.reserve((width * height) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let byte = bits.get(y * stride + (x >> 3)).copied().unwrap_or(0);
                let set = (byte >> (7 - (x & 7))) & 1 == 1;
                self.buffer.push(if set { 0xFF00_0000 } else { 0xFFFF_FFFF });
            }
        }
        self.buf_width = width;
        self.buf_height = height;
        #[cfg(feature = "ui")]
        self.present_window();
    }

    /// Expand a monochrome form to a packed RGB buffer (`w*h*3` bytes) for the
    /// PNG screenshot writer. Bit set → black, clear → white (UI.md §4A.3).
    pub fn mono_to_rgb(width: u32, height: u32, bits: &[u8]) -> Vec<u8> {
        let stride = width.div_ceil(8) as usize;
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let byte = bits.get(y * stride + (x >> 3)).copied().unwrap_or(0);
                let set = (byte >> (7 - (x & 7))) & 1 == 1;
                let v = if set { 0 } else { 255 };
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        rgb
    }

    // --- Clock (UI.md §4A.1) -----------------------------------------------

    /// Switch to the deterministic virtual clock, starting at t=0.
    pub fn use_virtual_clock(&mut self) {
        self.clock = TimeSource::Virtual { now_ns: 0 };
    }

    /// Advance virtual time (driver `tick`); no-op under the real clock.
    pub fn advance_virtual_ns(&mut self, dt: u64) {
        if let TimeSource::Virtual { now_ns } = &mut self.clock {
            *now_ns += dt;
        }
    }

    /// Monotonic nanoseconds: virtual counter, or wall time from `start`.
    pub fn mono_ns(&self, start: std::time::Instant) -> u64 {
        match &self.clock {
            TimeSource::Virtual { now_ns } => *now_ns,
            TimeSource::Real => start.elapsed().as_nanos() as u64,
        }
    }

    // --- Windowed backend (feature "ui") -----------------------------------

    #[cfg(feature = "ui")]
    fn present_window(&mut self) {
        use minifb::{Window, WindowOptions};
        if !self.windowed {
            return;
        }
        let (w, h) = (self.buf_width as usize, self.buf_height as usize);
        let recreate = match &self.window {
            Some(win) => win.get_size() != (w, h),
            None => true,
        };
        if recreate {
            self.window = Window::new("smallishtalk", w, h, WindowOptions::default()).ok();
        }
        if let Some(win) = &mut self.window {
            let _ = win.update_with_buffer(&self.buffer, w, h);
        }
    }

    #[cfg(feature = "ui")]
    fn harvest_window(&mut self) {
        use minifb::{Key, MouseButton, MouseMode};
        let Some(win) = &self.window else { return };
        if !win.is_open() || win.is_key_down(Key::Escape) {
            self.events.push_back([EV_CLOSE, 0, 0, 0, 0]);
            return;
        }
        if let Some((x, y)) = win.get_mouse_pos(MouseMode::Discard) {
            self.events
                .push_back([EV_MOUSE_MOVE, x as i64, y as i64, 0, 0]);
            let down = win.get_mouse_down(MouseButton::Left);
            self.events
                .push_back([EV_MOUSE_BUTTON, x as i64, y as i64, 1, down as i64]);
        }
        for key in win.get_keys_pressed(minifb::KeyRepeat::No) {
            self.events
                .push_back([EV_KEY_DOWN, key as i64, 0, 0, 0]);
        }
    }
}
