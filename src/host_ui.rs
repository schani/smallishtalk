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
    /// Print UI host diagnostics to stderr (window creation, present, events).
    /// Set from `--verbose`; always compiled so both builds can toggle it.
    pub verbose: bool,
    /// Count of present calls, for a throttled verbose heartbeat.
    #[cfg(feature = "ui")]
    pub present_count: u64,
    #[cfg(feature = "ui")]
    pub window: Option<minifb::Window>,
    /// Windowed builds open a real window on first present; headless never does.
    #[cfg(feature = "ui")]
    pub windowed: bool,
    /// The buffer dimensions the current window was created for. We recreate the
    /// window only when THESE change — not when the OS-reported window size
    /// differs, which it always does on a Retina/HiDPI display (physical vs
    /// logical size), so comparing against get_size() would recreate every frame.
    #[cfg(feature = "ui")]
    pub win_w: u32,
    #[cfg(feature = "ui")]
    pub win_h: u32,
    /// Last left-button state, so a button event fires on press/release edges
    /// rather than every frame the button is held.
    #[cfg(feature = "ui")]
    pub last_down: bool,
    /// Last right-button state — same edge discipline as `last_down`.
    #[cfg(feature = "ui")]
    pub last_down_right: bool,
    /// Characters typed into the window, delivered by minifb's input callback
    /// (which owns a clone of this Arc) and drained on every harvest. The
    /// callback is the only source of real characters — `get_keys_pressed`
    /// reports raw `Key`s with no layout/shift translation.
    #[cfg(feature = "ui")]
    pub pending_chars: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
    /// Last reported mouse position, so a move event fires only when the
    /// position actually changes. minifb re-reports the same position on
    /// every poll; pushing it unconditionally makes the event queue
    /// self-feeding, and a drain-until-empty pump never terminates.
    #[cfg(feature = "ui")]
    pub last_pos: Option<(i64, i64)>,
    /// Keys reported pressed by the last harvest — same self-feeding hazard
    /// as `last_pos`: between window updates minifb re-reports the same
    /// pressed set, so a key-down must fire only when it newly appears.
    #[cfg(feature = "ui")]
    pub last_keys: Vec<minifb::Key>,
    /// The close event has been posted; never post it twice (the closed /
    /// escape state stays set on every poll until the process exits).
    #[cfg(feature = "ui")]
    pub close_sent: bool,
}

impl Default for HostUi {
    fn default() -> HostUi {
        HostUi {
            buffer: Vec::new(),
            buf_width: 0,
            buf_height: 0,
            events: VecDeque::new(),
            clock: TimeSource::Real,
            verbose: false,
            #[cfg(feature = "ui")]
            present_count: 0,
            #[cfg(feature = "ui")]
            window: None,
            #[cfg(feature = "ui")]
            windowed: false,
            #[cfg(feature = "ui")]
            win_w: 0,
            #[cfg(feature = "ui")]
            win_h: 0,
            #[cfg(feature = "ui")]
            last_down: false,
            #[cfg(feature = "ui")]
            last_down_right: false,
            #[cfg(feature = "ui")]
            pending_chars: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            #[cfg(feature = "ui")]
            last_pos: None,
            #[cfg(feature = "ui")]
            last_keys: Vec::new(),
            #[cfg(feature = "ui")]
            close_sent: false,
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
        // Recreate only when the buffer size changed — never on a get_size()
        // mismatch (that fires every frame on HiDPI; see the field docs).
        let recreate =
            self.window.is_none() || self.win_w != self.buf_width || self.win_h != self.buf_height;
        if recreate {
            if self.verbose {
                eprintln!("ui: creating {w}x{h} minifb window...");
            }
            match Window::new("smallishtalk", w, h, WindowOptions::default()) {
                Ok(mut win) => {
                    if self.verbose {
                        eprintln!("ui: window created ({w}x{h})");
                    }
                    // Typed characters land in pending_chars; harvest drains
                    // them into key events (see ingest_window_state).
                    win.set_input_callback(Box::new(CharSink(self.pending_chars.clone())));
                    self.window = Some(win);
                    self.win_w = self.buf_width;
                    self.win_h = self.buf_height;
                }
                Err(e) => {
                    // Never silently swallow this — a failed window is exactly
                    // the "no window appears" symptom. Fail fast instead of
                    // retrying (and re-printing) every frame: stop windowed
                    // present and post a close event so the UI loop unwinds.
                    eprintln!("ui: ERROR: could not create window: {e}");
                    eprintln!(
                        "ui: no window available — exiting the UI loop. Ensure you are in a \
                         graphical session (a real macOS login, or a Linux display server); \
                         this will not work over plain SSH."
                    );
                    self.window = None;
                    self.windowed = false;
                    self.events.push_back([EV_CLOSE, 0, 0, 0, 0]);
                    return;
                }
            }
        }
        if let Some(win) = &mut self.window {
            if let Err(e) = win.update_with_buffer(&self.buffer, w, h) {
                eprintln!("ui: ERROR: update_with_buffer failed: {e}");
            }
        }
        self.present_count += 1;
        // A heartbeat so `--verbose` shows the loop is actually presenting.
        if self.verbose && (self.present_count == 1 || self.present_count % 120 == 0) {
            eprintln!("ui: presented {} frame(s)", self.present_count);
        }
    }

    #[cfg(feature = "ui")]
    fn harvest_window(&mut self) {
        use minifb::{Key, MouseButton, MouseMode};
        // Snapshot everything off the window, then mutate our own fields (avoids
        // borrowing the window and the event queue at once).
        let Some((open, esc, pos, down, down_right, keys)) = self.window.as_ref().map(|win| {
            (
                win.is_open(),
                win.is_key_down(Key::Escape),
                win.get_mouse_pos(MouseMode::Discard),
                win.get_mouse_down(MouseButton::Left),
                win.get_mouse_down(MouseButton::Right),
                win.get_keys_pressed(minifb::KeyRepeat::No),
            )
        }) else {
            return;
        };
        let chars = std::mem::take(&mut *self.pending_chars.lock().unwrap());
        self.ingest_window_state(
            open,
            esc,
            pos.map(|(x, y)| (x as i64, y as i64)),
            down,
            down_right,
            keys,
            chars,
        );
    }

    /// Turn one window-state snapshot into queued events. Split from the
    /// window poll so the edge-detection rules are testable without a real
    /// window. THE contract: identical consecutive snapshots (which is what
    /// minifb reports between updates) must eventually enqueue NOTHING —
    /// primNextEvent harvests on every call, so any state re-reported as an
    /// event makes the image's drain-until-empty pump spin forever. `chars`
    /// is exempt: it is the drained callback buffer, edge-y by construction.
    #[cfg(feature = "ui")]
    #[allow(clippy::too_many_arguments)]
    fn ingest_window_state(
        &mut self,
        open: bool,
        esc: bool,
        pos: Option<(i64, i64)>,
        down: bool,
        down_right: bool,
        keys: Vec<minifb::Key>,
        chars: Vec<u32>,
    ) {
        if !open || esc {
            if self.close_sent {
                return;
            }
            self.close_sent = true;
            if self.verbose {
                eprintln!(
                    "ui: close event ({})",
                    if !open { "window closed" } else { "escape" }
                );
            }
            self.events.push_back([EV_CLOSE, 0, 0, 0, 0]);
            return;
        }
        if let Some((x, y)) = pos {
            if self.last_pos != Some((x, y)) {
                self.events.push_back([EV_MOUSE_MOVE, x, y, 0, 0]);
                self.last_pos = Some((x, y));
            }
            // Only on a press/release edge, so a held button doesn't re-fire.
            // Slot c is the button number: 1 left, 2 right (Event>>buttons).
            if down != self.last_down {
                self.events.push_back([EV_MOUSE_BUTTON, x, y, 1, down as i64]);
                self.last_down = down;
            }
            if down_right != self.last_down_right {
                self.events.push_back([EV_MOUSE_BUTTON, x, y, 2, down_right as i64]);
                self.last_down_right = down_right;
            }
        }
        for key in &keys {
            if !self.last_keys.contains(key) {
                // Editing/navigation keys never reach the char callback, so
                // translate them here into the control codes TextPane
                // understands (8/10 backspace/enter; 28-31 are the classic
                // Smalltalk left/right/up/down arrow codes).
                let ch = match key {
                    minifb::Key::Backspace => 8,
                    minifb::Key::Enter => 10,
                    minifb::Key::Left => 28,
                    minifb::Key::Right => 29,
                    minifb::Key::Up => 30,
                    minifb::Key::Down => 31,
                    _ => 0,
                };
                self.events.push_back([EV_KEY_DOWN, *key as i64, 0, ch, 0]);
            }
        }
        self.last_keys = keys;
        for ch in chars {
            // Text only. Control chars (some platforms do send them here)
            // are covered by the key path above, and U+F700-F8FF is Apple's
            // private-use encoding of function/arrow keys, which macOS
            // reports as [event characters] and minifb forwards verbatim —
            // keys, not text.
            if ch >= 32 && ch != 127 && !(0xF700..=0xF8FF).contains(&ch) {
                self.events.push_back([EV_KEY_DOWN, 0, 0, ch as i64, 0]);
            }
        }
    }
}

/// The minifb input callback: appends each typed character to the shared
/// buffer `harvest_window` drains. (A separate type because the callback
/// must be boxed and `'static`, so it can't borrow HostUi.)
#[cfg(feature = "ui")]
struct CharSink(std::sync::Arc<std::sync::Mutex<Vec<u32>>>);

#[cfg(feature = "ui")]
impl minifb::InputCallback for CharSink {
    fn add_char(&mut self, uni_char: u32) {
        self.0.lock().unwrap().push(uni_char);
    }
}

#[cfg(all(test, feature = "ui"))]
mod tests {
    use super::*;
    use minifb::Key;

    fn drain(ui: &mut HostUi) -> Vec<HostEvent> {
        let mut out = Vec::new();
        while let Some(e) = ui.pop_event() {
            out.push(e);
        }
        out
    }

    /// The invariant primNextEvent's drain loop depends on: re-reporting an
    /// unchanged window snapshot (what minifb does between updates) enqueues
    /// nothing after the first harvest. Each regression here was a live hang.
    /// (Chars are exempt by construction — the callback buffer is drained.)
    #[test]
    fn repeated_identical_snapshots_quiesce() {
        let mut ui = HostUi::new();
        ui.ingest_window_state(true, false, Some((10, 20)), true, true, vec![Key::A, Key::B], vec![]);
        let first = drain(&mut ui);
        assert_eq!(
            first,
            vec![
                [EV_MOUSE_MOVE, 10, 20, 0, 0],
                [EV_MOUSE_BUTTON, 10, 20, 1, 1],
                [EV_MOUSE_BUTTON, 10, 20, 2, 1],
                [EV_KEY_DOWN, Key::A as i64, 0, 0, 0],
                [EV_KEY_DOWN, Key::B as i64, 0, 0, 0],
            ]
        );
        for _ in 0..3 {
            ui.ingest_window_state(
                true,
                false,
                Some((10, 20)),
                true,
                true,
                vec![Key::A, Key::B],
                vec![],
            );
            assert_eq!(drain(&mut ui), Vec::<HostEvent>::new());
        }
    }

    /// The right button reports as button 2 in slot c, with the same
    /// edge-only discipline as the left — this is what the Workspace's
    /// right-click operate menu rides on.
    #[test]
    fn right_button_fires_edges_as_button_2() {
        let mut ui = HostUi::new();
        ui.ingest_window_state(true, false, Some((5, 6)), false, true, vec![], vec![]);
        assert_eq!(
            drain(&mut ui),
            vec![
                [EV_MOUSE_MOVE, 5, 6, 0, 0],
                [EV_MOUSE_BUTTON, 5, 6, 2, 1],
            ]
        );
        ui.ingest_window_state(true, false, Some((5, 6)), false, false, vec![], vec![]);
        assert_eq!(drain(&mut ui), vec![[EV_MOUSE_BUTTON, 5, 6, 2, 0]]);
    }

    /// Typed characters (the window's char callback) become key-down events
    /// carrying the character in slot c — what TextPane inserts. Control
    /// chars are dropped here; backspace/enter arrive as keys instead.
    #[test]
    fn chars_become_key_events_with_the_char_in_c() {
        let mut ui = HostUi::new();
        ui.ingest_window_state(true, false, Some((0, 0)), false, false, vec![], vec![104, 7, 105]);
        assert_eq!(
            drain(&mut ui),
            vec![
                [EV_MOUSE_MOVE, 0, 0, 0, 0],
                [EV_KEY_DOWN, 0, 0, 104, 0],
                [EV_KEY_DOWN, 0, 0, 105, 0],
            ]
        );
    }

    /// Backspace and Enter never come through the char callback, so their
    /// key events carry the control codes (8, 10) TextPane understands.
    #[test]
    fn editing_keys_carry_their_control_codes() {
        let mut ui = HostUi::new();
        ui.ingest_window_state(
            true,
            false,
            Some((0, 0)),
            false,
            false,
            vec![Key::Backspace, Key::Enter],
            vec![],
        );
        assert_eq!(
            drain(&mut ui),
            vec![
                [EV_MOUSE_MOVE, 0, 0, 0, 0],
                [EV_KEY_DOWN, Key::Backspace as i64, 0, 8, 0],
                [EV_KEY_DOWN, Key::Enter as i64, 0, 10, 0],
            ]
        );
    }

    /// macOS reports arrow/function/navigation keys as fake "characters" in
    /// the Unicode private-use area (U+F700–U+F8FF, e.g. left arrow =
    /// U+F702 = NSLeftArrowFunctionKey) and minifb's char callback forwards
    /// them verbatim. They are keys, not text: letting one through crashed
    /// the Workspace (option-left → insert 0xF702 into a byte String).
    #[test]
    fn mac_function_key_chars_are_dropped() {
        let mut ui = HostUi::new();
        ui.ingest_window_state(
            true,
            false,
            Some((0, 0)),
            false,
            false,
            vec![],
            vec![0xF700, 0xF702, 0xF8FF, 97],
        );
        assert_eq!(
            drain(&mut ui),
            vec![[EV_MOUSE_MOVE, 0, 0, 0, 0], [EV_KEY_DOWN, 0, 0, 97, 0]]
        );
    }

    /// Arrow keys ride the key path (like backspace/enter) as the classic
    /// Smalltalk control codes 28–31, so TextPane can navigate on them.
    #[test]
    fn arrow_keys_carry_the_classic_control_codes() {
        let mut ui = HostUi::new();
        ui.ingest_window_state(
            true,
            false,
            Some((0, 0)),
            false,
            false,
            vec![Key::Left, Key::Right, Key::Up, Key::Down],
            vec![],
        );
        assert_eq!(
            drain(&mut ui),
            vec![
                [EV_MOUSE_MOVE, 0, 0, 0, 0],
                [EV_KEY_DOWN, Key::Left as i64, 0, 28, 0],
                [EV_KEY_DOWN, Key::Right as i64, 0, 29, 0],
                [EV_KEY_DOWN, Key::Up as i64, 0, 30, 0],
                [EV_KEY_DOWN, Key::Down as i64, 0, 31, 0],
            ]
        );
    }

    #[test]
    fn changes_fire_as_edges() {
        let mut ui = HostUi::new();
        ui.ingest_window_state(true, false, Some((10, 20)), true, false, vec![Key::A], vec![]);
        drain(&mut ui);
        // Move, release, new key; A held over from last time must not re-fire.
        ui.ingest_window_state(true, false, Some((11, 20)), false, false, vec![Key::A, Key::C], vec![]);
        assert_eq!(
            drain(&mut ui),
            vec![
                [EV_MOUSE_MOVE, 11, 20, 0, 0],
                [EV_MOUSE_BUTTON, 11, 20, 1, 0],
                [EV_KEY_DOWN, Key::C as i64, 0, 0, 0],
            ]
        );
        // A released then re-pressed fires again.
        ui.ingest_window_state(true, false, Some((11, 20)), false, false, vec![], vec![]);
        drain(&mut ui);
        ui.ingest_window_state(true, false, Some((11, 20)), false, false, vec![Key::A], vec![]);
        assert_eq!(drain(&mut ui), vec![[EV_KEY_DOWN, Key::A as i64, 0, 0, 0]]);
    }

    #[test]
    fn close_fires_exactly_once() {
        for esc in [false, true] {
            let mut ui = HostUi::new();
            let open = esc; // closed-window in one case, escape in the other
            ui.ingest_window_state(open, esc, None, false, false, Vec::new(), Vec::new());
            assert_eq!(drain(&mut ui), vec![[EV_CLOSE, 0, 0, 0, 0]]);
            for _ in 0..3 {
                ui.ingest_window_state(open, esc, None, false, false, Vec::new(), Vec::new());
                assert_eq!(drain(&mut ui), Vec::<HostEvent>::new());
            }
        }
    }
}
