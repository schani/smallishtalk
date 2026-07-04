# UI.md — A Classic Smalltalk UI for smallishtalk

*A plan for a bitmap display, an Oberon-style tiling window manager, a small
Smalltalk widget toolkit, and — as the first real application — a live Class
Browser.*

Status: **implemented (M0–M5 complete; M6 Workspace done, rest stretch).** This
document describes the design; the staged path below has been built and is
tested headlessly in `cargo test`. A live Class Browser renders from the running
image (see `docs/class-browser-demo.png`). It is the UI counterpart to `SPEC.md`
and `JIT.md`.

> **Implementation note.** All layers L0–L5 are in place: the host seam
> (`src/host_ui.rs`, `src/png.rs`, primitives 323/330–334), the graphics kernel
> (`st/ui/gfx/`), the tiling WM (`st/ui/wm/`), the widget toolkit
> (`st/ui/widgets/`), reflection + runtime compilation (`st/ui/kernel/`), and the
> apps (`st/ui/apps/`). Live "accept" works via an in-image runtime reifier
> (`RuntimeCompiler`) over `PRIM_METHOD_INSTALL`. Tests: the host seam in
> `tests/ui_headless.rs` and `tests/live_install_test.rs`; everything above it
> as **in-image Smalltalk tests** — `st/tests/ui/` (graphics, WM, widgets,
> reflection, browser suites on a minimal in-image SUnit), launched by
> `tests/st_suite.rs`. Scripted input reaches the real event pipeline through
> `primPostEvent` (334).
> One deviation from the plan: live compilation required the compiler filed into
> the image and `CompiledMethod`/`CompiledBlock` to declare their real ivars.

---

## 0. Goals & non-goals

### Goals
- Put **real pixels on a real screen** from the running image, through the
  primitive seam the Treaty already reserves (SPEC §16).
- Do it **the classic Smalltalk way**: a thin host, a `BitBlt` primitive, and
  *everything above BitBlt written in Smalltalk* — `Form`, `Pen`, fonts,
  `Canvas`, views, controllers, windows.
- A **tiling window manager modelled on Oberon**: the screen is divided into
  vertical *tracks*, each track into non-overlapping, title-less *viewers* that
  always tile to fill their track. No overlapping windows, no free-floating
  chrome.
- A **live Class Browser** as the first application: navigate class categories →
  classes → protocols → selectors → source, **edit a method's source and
  "accept" it to recompile and install it live**, and *do-it / print-it* on
  selected text.
- **Headless-first operability** (foundational, not a fallback): the whole UI can
  be *driven without a window* by a script of actions, and its screen captured to
  an image file at any point — so a developer or an agent can operate it, take
  screenshots, and look at them. The real `minifb` window is just an optional
  live view over the same machinery.
- **Profiling built in from day one**, in a principled way: every frame is
  instrumented; deterministic *work* metrics (blits, pixels, glyphs, events) are
  separated from wall-clock *timing* metrics, so performance can be both watched
  by humans and asserted by CI.
- **Automated testing built in from day one**, in a principled way: the headless
  driver + a deterministic clock + screenshot capture *is* the test substrate;
  every milestone ships with tests and runs in CI without a window.

### Non-goals (for this plan)
- No color/anti-aliasing/compositing to start — the Display is **1-bit
  monochrome**, as Smalltalk-80 was. Grayscale/color is a later, optional depth.
- No dependency on the JIT. The UI runs interpreted; JIT (see `JIT.md`) only
  makes it faster later. Nothing here assumes it.
- No web/canvas rendering. This is a native window (the recently-built HTML
  walk-through is documentation, unrelated to this runtime UI).
- Not a full Morphic. We build the minimum classic toolkit the Browser needs,
  designed to grow.

### Decisions locked for this plan
| Decision | Choice |
|---|---|
| Host windowing backend | **`minifb`** (single framebuffer + input), behind a `ui` Cargo feature |
| Look / interaction model | **Oberon tiling geometry + Smalltalk-80 widgets** inside viewers |
| VM / image split | **Thin seam**: host blits a `Form` + delivers events; **BitBlt is a Rust primitive**; all higher drawing is Smalltalk |
| First browser milestone | **Live edit / compile / accept** |
| Display depth | **1-bit monochrome** first (expand to ARGB at blit) |
| Fonts | **Baked-in bitmap strike font**, no font crate |
| Primary run mode | **Headless-first**: a scriptable driver + screenshot capture; the window is an optional view |
| Determinism | A selectable **virtual clock**; headless runs are fully reproducible |
| Profiling | **Built in from M0**: always-on work counters + per-frame timing; deterministic counts vs wall times kept distinct |
| Testing | **Built in from M0**: driver-scripted scenarios + golden screenshots + model/perf assertions, all headless |

---

## 1. Where we are starting from (recon summary)

The runtime is green-field for graphics but the seam is pre-cut:

- **No graphics code anywhere** in `src/` or `st/`. Zero GUI/graphics crates;
  `Cargo.toml` `[dependencies]` is empty.
- **The Treaty already reserves the seam** (SPEC §16):
  - `PRIM_NEXT_EVENT = 330` (`src/treaty.rs:354`) — implemented today as a
    **stub** returning `nil` (`src/prims.rs:275`); "the host has no event
    sources; the queue is always empty."
  - `PRIM_PIXEL_BLIT = 331` (`src/treaty.rs:355`) — **name only**: in the Treaty
    validation set and mirrored in `st/compiler/Treaty.st`, but **no dispatch
    arm** yet.
- **Primitive model**: one big `match n` in `dispatch_primitive`
  (`src/prims.rs`), primitives grouped by range (host I/O lives at 300–331),
  `<primitive: N>` pragmas on kernel methods, `PrimOutcome::Fail(code)` falls
  back to the Smalltalk body. Adding a host primitive is a known 5-step ritual
  (see §11).
- **Concurrency**: single native interpreter thread + a **Smalltalk-level
  scheduler** (`ProcessorScheduler`, priority run-queues), real `Process`,
  `Semaphore`, `Delay`. Timers via `PRIM_SIGNAL_AT_MS = 322` +
  `self.timer_requests` + the interpreter **safepoint**
  (`poll_safepoint`/`service_safepoint`, `src/interp.rs`). **This is enough for a
  UI loop** — no new concurrency model needed.
- **Compiler is in-image and reusable** (`st/compiler/`), and
  `PRIM_METHOD_INSTALL = 402` installs a compiled method. So *live* recompilation
  is already possible.
- **Two real gaps for a browser**, both addressed by this plan:
  1. **No reflective enumeration API** — no `allClasses`, `selectors`,
     `subclasses`, `methodDictionary` accessor; `SystemDictionary` is declared
     with no methods and there is no populated `Smalltalk` global.
  2. **Method source is not retained** — `CompiledMethod` has no source ivars;
     the reserved slot `METHOD_SOURCE_INFO = 6` (`src/treaty.rs:221`) is
     **explicitly nilled** at `st/compiler/ImageWriter.st:352`.

---

## 2. Architecture: five layers

```
 ┌──────────────────────────────────────────────────────────────┐
 │  L5  Applications        ClassBrowser · Workspace · Transcript │  Smalltalk (st/ui/apps/)
 ├──────────────────────────────────────────────────────────────┤
 │  L4  Widget toolkit      View/Controller · ListPane ·          │  Smalltalk (st/ui/widgets/)
 │                          TextPane · MenuPane · ScrollBar        │
 ├──────────────────────────────────────────────────────────────┤
 │  L3  Window manager      Display · Track · Viewer · damage/     │  Smalltalk (st/ui/wm/)
 │      (Oberon tiling)     redraw · focus · cursor                │
 ├──────────────────────────────────────────────────────────────┤
 │  L2  Graphics + events   Form · Point · Rectangle · BitBlt      │  Smalltalk (st/ui/gfx/)
 │                          (wrapper) · Pen · StrikeFont · Canvas  │
 │                          Event · UI event-loop process          │
 ├──────────────────────────────────────────────────────────────┤
 │  L1  Primitive seam      primPixelBlit(331) · primNextEvent(330)│  Rust (src/prims.rs,
 │                          · primBitBlt(332)                      │        src/host_ui.rs)
 ├──────────────────────────────────────────────────────────────┤
 │  L0  Host backend        minifb window · ARGB buffer · key/     │  Rust (feature "ui")
 │                          mouse polling  ‖  headless PPM sink     │
 └──────────────────────────────────────────────────────────────┘
```

Only **L0–L1 are Rust**. Everything from `Form` upward is Smalltalk, which is the
whole point of "classic Smalltalk UI." The single narrow contract between them is
a handful of primitives plus an event encoding.

### 2.1 Three cross-cutting pillars (built in from day one)

Orthogonal to the layers, three concerns are designed in from M0 rather than
bolted on. They share one foundation — a **headless, deterministic driver** —
described in §4A, with detail in the sections noted:

- **Operability / headless + screenshots (§4A).** The Display can be presented
  into an in-memory buffer and saved to a PNG on demand; input can be supplied by
  a *script* of actions. So the UI is fully operable with no window — a person or
  an agent drives it and inspects screenshots. Same code paths as the windowed
  build; the window is one more presenter.
- **Profiling (§13).** A frame instrument records per-phase work and time every
  tick; a small set of always-on counters lives in the VM. Deterministic
  *work* metrics (blits, pixels, glyphs, events, allocations) are kept separate
  from non-deterministic *timing* metrics, so CI asserts the former and humans
  read the latter.
- **Testing (§12).** The driver + a deterministic virtual clock + screenshot
  capture is the test substrate: a test is a scenario script plus golden
  screenshots plus model/perf assertions. Every milestone ships tests; the whole
  suite runs headless in `cargo test`.

None of the three depends on the JIT or on a display server.

---

## 3. L0 — Host backend (`minifb`, feature-gated)

### 3.1 The window
A new module `src/host_ui.rs` owns an `Option<HostWindow>` held on the `Vm`
(`src/vm.rs`). `HostWindow` wraps a `minifb::Window` plus:
- an ARGB scratch buffer `Vec<u32>` sized `width*height` (the thing minifb
  actually presents),
- an **event queue** `VecDeque<HostEvent>`,
- last-known mouse position / button state (minifb is poll-based).

The window is created **lazily on the first `primPixelBlit`**, so an image that
never opens a display never touches minifb.

### 3.2 Threading — who owns the OS loop
minifb (like most windowing) wants the window created and pumped on the **main
thread**, which is exactly where our single interpreter loop already runs. That
is convenient, not a conflict:

- We do **not** hand the OS event loop to minifb. The interpreter stays in
  charge.
- **Input is harvested on demand, not only at blit.** The host polls minifb's
  state (`get_keys`, `get_keys_pressed`, `get_mouse_pos`, `get_mouse_down`,
  `get_scroll_wheel`) and refills the `HostEvent` queue **inside
  `primNextEvent`** (§4.2). This is the fix for a subtle trap: if events were
  only harvested during `primPixelBlit`, a *static* screen (no damage → no blit)
  would silently stop receiving input, including the window-close event. Because
  the UI loop calls `primNextEvent` every tick regardless of damage, events
  always flow. (`update_with_buffer` still incidentally pumps the OS queue when
  we do blit; that is a bonus, not the contract.)
- The **image drives the cadence** with the machinery that already exists: a
  high-priority UI `Process` sleeps on a `Delay` (~16 ms) between frames
  (`PRIM_SIGNAL_AT_MS` → safepoint → semaphore), wakes, **drains events every
  tick**, and *then* redraws damage (if any) and blits. No host→image push
  interrupt is required; polling at ~60 Hz is the classic approach and keeps
  minifb responsive.

> Because there is no push, input latency is bounded by the poll interval. 16 ms
> is fine for a browser. If we ever want lower latency we can add an interrupt
> semaphore, but it is explicitly out of scope here.

### 3.3 Two presenters over one Display (headless is not a fallback)
The Display `Form` and every layer above it are **identical** whether or not a
window exists. L0 only chooses *who presents the pixels* and *where events come
from*:

| | Windowed (`--ui`, feature `ui`) | Headless (default) |
|---|---|---|
| Present | `minifb::update_with_buffer` | in-memory ARGB buffer; `primSaveForm:toFile:` → **PNG** file |
| Events | polled from minifb | pulled from a **scripted queue** (the driver, §4A) |
| Clock | real monotonic time | selectable **virtual clock** (§4A) |
| Output capture | window **and** buffer | buffer only |

This mirrors the existing `stdout_capture` pattern (real stdout **and** a capture
buffer): the same drawing code always fills the same buffer; a window, if
present, is just an extra consumer of it. Because the default build pulls **no
crates and opens no window**, `cargo test` and any agent-driven session run the
*real* UI headlessly and reproducibly. See §4A.

### 3.4 Cargo
```toml
[features]
ui = ["dep:minifb"]

[dependencies]
minifb = { version = "0.27", optional = true }   # first external dependency
```
The binary gains a `--ui` path (or an env flag) that enables opening a real
window; without it, or without the feature, it stays headless.

---

## 4. L1 — The primitive seam

The entire Rust⇄image graphics contract — three drawing/event primitives plus two
that exist purely to make the UI operable and profilable (§4.4):

### 4.1 `primPixelBlit` (331) — present the Display
```
primPixelBlit(displayForm)  "framed or frameless per §16 convention"
```
- Argument: the Display `Form` (§5.1). Reads its `width`, `height`, `depth`, and
  `bits` (a `ByteArray`).
- For a 1-bit Form: expand each bit to `0xFF000000` (black) / `0xFFFFFFFF`
  (white) into the ARGB buffer, then `update_with_buffer`. (Depth 8/32 later:
  direct copy / palette.)
- Lazily creates the window at the Form's size on first call; on later calls, if
  the Form size changed, resizes.
- Returns the receiver (or `nil`). (Input harvesting is **not** tied to blit —
  it happens in `primNextEvent`, §3.2/§4.2 — so a static, undamaged screen still
  receives events.)

Optional refinement: accept a **damage rectangle** argument
(`primPixelBlit: form rect: aRectangle`) so we only repaint changed pixels. Start
whole-Form; add the rect variant when redraw cost warrants.

### 4.2 `primNextEvent` (330) — harvest + poll one event
Replace the stub. **First refills the queue** from the host (windowed: poll
minifb; headless: pull from the scripted queue, §4A), **then** pops the oldest
`HostEvent` and returns it **encoded as a small `Array`** (cheap, no new class needed across the seam; the image wraps it in an
`Event`, §6.1):

```
#(type  a  b  c  d)   "SmallIntegers"
 type 0  none / queue empty          →  nil
 type 1  mouse move       a=x b=y
 type 2  mouse button     a=x b=y c=buttons(bitmask L=1 M=2 R=4) d=down?(1/0)
 type 3  key down         a=keycode b=modifiers c=unicodeChar
 type 4  key up           a=keycode b=modifiers
 type 5  scroll           a=x b=y c=dxFixed d=dyFixed
 type 6  window resize    a=newWidth b=newHeight
 type 7  window close     (user hit the close box)
```
Returning `nil` on empty preserves the current stubbed contract, so existing
callers that expect "empty → nil" keep working.

The write side of the same queue is `primPostEvent` (334): push one scripted
`type a b c d` event exactly as a real device would enqueue it. That is what
lets the **in-image test suite** (`st/tests/ui/`) drive `pumpEvents` —
clicks, moves, close — with no host-side cooperation
(`Event postType:a:b:c:d:`).

### 4.3 `primBitBlt` (332) — the workhorse (NEW primitive number)
The one piece of drawing that lives in Rust, for speed — exactly as Smalltalk-80
made BitBlt a primitive. It copies a rectangle of bits from a source Form to a
destination Form under a combination rule, with clipping.

Argument: a `BitBlt` setup object (§5.3) whose fields the primitive reads:
`destForm sourceForm halftoneForm combinationRule destX destY width height
sourceX sourceY clipX clipY clipWidth clipHeight`. It performs the standard
BitBlt inner loop over packed monochrome rows (row stride = `ceil(width/8)`
bytes), honoring these combination rules to start:

| rule | op |
|---|---|
| 0 | `dest ← 0` (clear) |
| 3 | `dest ← src` (store/over) |
| 6 | `dest ← src XOR dest` (reversible — cursors, selection) |
| 7 | `dest ← src OR dest` (paint) |
| others | added as needed |

On failure (bad args, out-of-range), `Fail` → the Smalltalk fallback body can do
a slow pure-Smalltalk blit (useful in headless builds and as a correctness
oracle). This gives us the *pure* path for free as the fallback while the
primitive is the fast path.

> Why a primitive and not pure Smalltalk? A full-screen 1-bit redraw is
> ~hundreds of KB of bit twiddling per frame; interpreted that is too slow for
> interactive feel today (pre-JIT). The BitBlt primitive is the classic answer;
> everything *above* it stays in the image.

### 4.4 `primSaveForm:toFile:` (333) and a high-resolution clock (323) — NEW
Two small primitives that exist so the UI is operable, screenshot-able, and
profilable from the very first milestone:

- **`primSaveForm: aForm toFile: aFileName` (333)** — write **the Form passed as
  an argument** (normally the Display Form, but any Form) to a **PNG** file. It
  takes an explicit Form rather than an implicit "current Display": the host only
  reliably knows the object handed to it, and an explicit argument avoids
  stale/ambiguous screenshots. PNG (not PPM) so screenshots are directly viewable
  by a person or an agent. The encoder is a ~50-line **zero-dependency** writer
  using stored (uncompressed) zlib blocks — no image crate. Monochrome expands to
  black/white; higher depths later. Works identically in windowed and headless
  builds. (A raw-PPM path is kept as a trivial oracle.)
- **`primClockMonotonicNs` (323)** — a nanosecond monotonic clock for frame
  timing (`PRIM_CLOCK_MONOTONIC_MS = 320` is too coarse for per-phase frame
  measurement). In **virtual-clock** mode (§4A) it returns the virtual time, so
  timing-based logic stays deterministic under the driver.

Both follow the standard ritual in §11.

---

## 4A. Operability: headless-first driving, screenshots & determinism

This is the foundation the three pillars (§2.1) stand on. It lands in **M0**, so
every later layer is built and inspected through it.

### 4A.1 The virtual clock (determinism)
A `TimeSource` abstraction on the Vm, selected at launch:
- **real** — `primClockMonotonic*`, `primSignalAt:` use wall time (windowed/live
  use).
- **virtual** — time is a counter that only advances when the driver says
  `tick <ms>`. `primSignalAt:` fires against virtual time; `Delay` "sleeps"
  resolve instantly by advancing the counter to the next due timer. Nothing in
  the image changes — it still uses `Delay`/`Semaphore` — but a scenario is
  perfectly reproducible and runs as fast as the CPU allows.

This is what makes headless runs deterministic and profiling repeatable.

### 4A.2 The driver (operating the UI without a window)
**Important constraint:** today there is *no* host→image command channel — the VM
loads an image and runs `active_process>>startUp`, full stop. So the driver is
authored **in the image**, where it has full access to send messages; the host's
job is deliberately narrow.

- **In-image `UIDriver` / `UIScript` (the authoritative driver).** Ordinary
  Smalltalk. It has both **low-level** verbs — `moveTo:`, `clickAt:`, `key:`,
  `type:`, `tick:` — that synthesize `HostEvent`s and step the clock/UI loop, and
  **semantic** verbs — `open:`, `select:`, `accept`, `assert:` — that are just
  message sends to the running app/model. A scenario is a Smalltalk method (or a
  tiny data literal it interprets). SUnit `UITestCase`s (§12) *are* these
  scenarios with assertions.
  ```smalltalk
  driver open: ClassBrowser.
  driver select: Array; select: #printOn:.
  driver type: 'printOn: aStream
      aStream nextPutAll: ''an Array'''.
  driver accept.
  driver shot: '01-array-printOn'.           "→ primSaveForm:toFile:"
  self assert: (Array includesSelector: #printOn:).
  ```
- **Host CLI (narrow).**
  ```
  smallishtalk --headless --scenario browser-demo --shots out/ image.im
  ```
  The host selects headless mode + the virtual clock, names an **in-image
  scenario** for `startUp` to run, and sets the screenshot output dir. It may
  also feed a **low-level event script** (a host-parseable file of
  `move/click/key/type/tick`) into the scripted event queue for cases where you
  want to drive purely by raw input. **CLI *semantic* verbs are deferred** — they
  would need a host→image control channel that doesn't exist yet; not worth
  building for v1. So: semantic driving lives in Smalltalk; the host only injects
  raw events and picks the scenario.

Either way a scenario is both a **repro** and a **test**.

### 4A.3 Screenshots
`shot <name>` calls `primSaveForm: Display form toFile: …` (§4.4) → a PNG under
`--shots`. Because the
default Display is **1-bit**, screenshots are crisp and, crucially, **exactly
reproducible** — a golden test can compare the *entire buffer by hash*, with no
anti-aliasing flakiness. For humans/agents the PNGs are viewable directly.

> **The agent/dev workflow this enables:** build the UI image → run a driver
> scenario producing PNGs → view the PNGs → adjust → re-run. Trying the UI out
> and testing it are the *same* loop, and neither needs a display server.

### 4A.4 Why headless is primary
Everything above L0 is presenter-agnostic (§3.3), so building headless-first costs
nothing and buys: reproducibility, CI without a display, agent-operability, and a
clean seam for profiling. The `minifb` window is added for live use, not required
for development.

---

## 5. L2 — Graphics kernel (Smalltalk, `st/ui/gfx/`)

All of this is ordinary Smalltalk built on the three primitives.

### 5.1 `Point`, `Rectangle`
Value classes: `Point x y` with `+ - * < max: min:`, `Rectangle origin corner`
with `containsPoint: intersect: merge: area width height`. Needed everywhere
below; small and mechanical.

### 5.2 `Form`
```
Form
  ivars: width height depth bits   "bits is a ByteArray, monochrome-packed"
  Form width:height:               "allocates bits = ByteArray of height*stride"
  bitsAt:put: / stride / boundingBox / extent
  copy:from:to:rule:               "convenience → builds a BitBlt and runs it"
  displayOn: aForm at: aPoint      "blit self onto another Form"
```
The **Display** is a distinguished `Form` (screen-sized, e.g. 1024×768×1) held by
`DisplayScreen`/`Display` (§7.1). `primPixelBlit` is only ever called with it.

### 5.3 `BitBlt`
A thin Smalltalk object holding the setup fields (§4.3) with a `copyBits` method
that invokes `<primitive: 332>` and falls back to a pure-Smalltalk loop. All
higher drawing composes BitBlts.

### 5.4 `Pen` (turtle/line drawing)
`Pen` on a `Form`: `place: go: goto: turn: down up print:` — lines via BitBlt of a
1-pixel brush (Bresenham in Smalltalk). Enough for viewer borders, separators,
the caret, list highlight rules.

### 5.5 `StrikeFont` (baked-in bitmap font)
Classic Smalltalk-80 strike font: one `Form` holding all glyphs laid out
side-by-side plus an `xTable` of left edges; `characterFormAt:` returns a glyph
sub-rectangle; `displayString:on:at:` BitBlts glyphs left-to-right.
- Ship **one** default font, embedded as a `ByteArray` literal in
  `st/ui/gfx/DefaultFont.st`, generated from a permissively-licensed bitmap
  font (Adobe Helvetica 12 BDF, `st/ui/gfx/helvR12.bdf`) by a tiny host script
  (`st/tools/gen_font_bdf.py` → prints the `.st`). No font crate, no runtime
  file I/O.
- `TextStyle`/metrics kept minimal: one size, ascii + a few symbols.

### 5.6 `Canvas` (drawing façade)
A convenience wrapper a view is handed to paint itself:
`fillRectangle:color: frameRectangle: line:to: drawString:at:font: clipTo:`.
Internally all BitBlt/Pen. Views never touch primitives directly; they talk to a
`Canvas` clipped to their bounds.

---

## 6. L2 — Events & the UI loop (Smalltalk)

### 6.1 `Event`
Wraps the array from `primNextEvent` (§4.2) into a message-y object:
`isMouseMove isMouseDown isKeystroke position keyCharacter modifiers buttons`.

### 6.2 The event-loop process
A single high-priority `Process` — the classic "UI process":

```
UISupervisor>>run
  [true] whileTrue: [
    self pumpEvents.               "drain primNextEvent → dispatch to focus/hit viewer"
    Display damaged ifNotEmpty: [  "coalesced dirty rectangles"
      self redrawDamage.
      Display flush].              "→ primPixelBlit (whole Form or damage rect)"
    (Delay forMilliseconds: 16) wait]   "→ primSignalAt: → safepoint → semaphore"
```
- **Dispatch**: mouse events go to the viewer under the pointer; keystrokes go to
  the *focused* viewer/pane. The WM (§7) resolves hit-testing and focus.
- **Damage/redraw**: views mark dirty rectangles; the loop merges them and
  repaints only those, then blits. Keeps interpreted redraw affordable.
- Built entirely on existing `Delay`/`Semaphore`/scheduler — no VM changes.

`window close` (type 7) cleanly terminates the UI process and lets the image
snapshot or exit.

---

## 7. L3 — The window manager (Oberon tiling, `st/ui/wm/`)

The distinctive part. We take **Oberon's geometry and tiling discipline** and run
Smalltalk widgets inside it.

### 7.1 Model
```
Display  (the screen Form + the whole desktop)
  └─ Track*        vertical columns, side by side, together filling width
        └─ Viewer*  stacked top→bottom, together filling the track height
              └─ Frame/View  the viewer's content (a widget tree)
```
- **No overlap, no gaps, no title bars.** Viewers always tile to fill their
  track; tracks always tile to fill the screen. Opening/closing/resizing a
  viewer re-tiles its track.
- Each Viewer has a thin **border** and a small **menu region** (Oberon's
  "menu-frame" — a one-line strip carrying the viewer's name and commands),
  drawn by the WM; the content frame gets the rest.

### 7.2 Geometry / tiling rules
- **Open** a viewer in a track: split the track's *focused* viewer, giving the
  newcomer the lower portion (Oberon convention), or the whole track if empty.
- **Close**: the freed space is absorbed by the neighbor above (or the track
  collapses).
- **Resize**: drag a viewer's top border ("the star"/handle) to move the split
  between it and its upper neighbor; content reflows.
- **Grow/full** *(optional, defer if the browser doesn't need it)*: a command to
  expand a viewer to fill its track (toggle).
- Default layout: **two tracks** (a narrow "system/log" track and a wide "work"
  track), like Oberon's user track + system track.

```
 ┌───────────── Display ──────────────────────────────┐
 │  Track 0 (work)            │  Track 1 (system)      │
 │ ┌───────────────────────┐  │ ┌────────────────────┐ │
 │ │ Viewer: Class Browser │  │ │ Viewer: Transcript │ │
 │ │ (menu strip)          │  │ │ (menu strip)       │ │
 │ │ ┌───────────────────┐ │  │ ├────────────────────┤ │
 │ │ │  content frame    │ │  │ │ Viewer: Workspace  │ │
 │ │ └───────────────────┘ │  │ │                    │ │
 │ └───────────────────────┘  │ └────────────────────┘ │
 └────────────────────────────┴────────────────────────┘
```

### 7.3 Interaction (Oberon-flavored, Smalltalk-run)
- **Focus** follows the pointer's viewer for keystrokes (Oberon-like); explicit
  click sets an insertion focus inside a text pane.
- **Commands**: the viewer's menu strip carries word-commands. Middle-click
  ("execute") on a command word runs it — this is Oberon's central gesture and
  maps cleanly onto Smalltalk `do-it`: selecting text anywhere and middle-
  clicking *executes* it; a menu word like `Accept` runs the pane's command.
- **Three-button mouse mapped to Smalltalk-80 meanings** where it helps
  (left=select, middle=execute/operate menu, right=window/viewer menu), so users
  of either tradition are at home. (Configurable; documented.)

### 7.4 Cursor
A small `Form` blitted with XOR (rule 6) so it is reversible without saving
background; moved on every mouse-move event.

---

## 8. L4 — Widget toolkit (`st/ui/widgets/`)

A deliberately small **MVC-lite** kit — only what the Browser needs, built to
extend.

- **`View`** — a rectangle in a viewer's content frame: `bounds model
  controller`, `displayOn: aCanvas`, `invalidate`, `containsPoint:`, child
  views. Composable (a browser is a `PanedView` of sub-views).
- **`Controller`** — handles events routed to a view: `handleEvent:`,
  `wantsFocus`. (We keep controllers as light strategy objects, not the full
  ST-80 control-loop, since our event loop is centralized in §6.)
- **`PanedView`** — splits its bounds into N child panes with draggable
  splitters (used for the 5-pane browser). Horizontal and vertical.
- **`ListPane`** — a scrollable list of strings with single selection;
  `items: selectionIndex: onSelect:`; highlights via XOR BitBlt; keyboard
  up/down.
- **`TextPane`** — a scrollable, **editable** text view: caret, selection,
  insertion, backspace, cut/paste (a simple global paste buffer), word/line
  navigation; exposes `contents`, `selection`, `accept` hook. This is the
  workhorse for the source pane and the workspace.
- **`MenuPane`** — pop-up list menu (for the right-button viewer menu and the
  browser's operate menus) returning the chosen command symbol.
- **`ScrollBar`** — thin, attaches to `ListPane`/`TextPane`. `ListPane` builds
  one in automatically when its items overflow: a 6-px strip on the right edge
  with a proportional thumb. Pressing the thumb grabs it and mouse moves drag
  it until release; pressing the trough jumps there first, then drags. The
  `ClassBrowser` captures the mouse on any pane press, so a drag keeps
  scrolling even when the pointer leaves the strip; the selection is never
  touched by scrolling.

All render through `Canvas` (§5.6) and the `StrikeFont`; all interaction arrives
as `Event`s from the loop (§6).

---

## 9. L4.5 — Reflection & source retention (the Browser's data model)

Two enablers, needed before a *live* browser can exist.

### 9.1 Retain method source
- **Stop nilling** the reserved slot: change `st/compiler/ImageWriter.st:352`
  (`m at: Treaty methodSourceInfo + 1 put: nilObj`) to instead store the
  method's **source string** (already in hand at compile time in
  `StCodeGen`/`Compiler`). Store either the raw source `String`, or a small
  `Array { sourceString. protocolSymbol }` so we also keep the method's protocol.
- Add `CompiledMethod>>sourceString` / `protocol` reading that slot.
- Cost: image grows by the size of kernel source (acceptable; can be made
  optional per build). No VM change — the slot already exists in the Treaty
  layout (`METHOD_SOURCE_INFO = 6`).

### 9.2 A reflection API (kernel methods, `st/kernel/`)
- `Behavior>>methodDictionary` (accessor), `selectors` (its keys),
  `includesSelector:`, `sourceCodeAt:` (→ method's `sourceString`),
  `compiledMethodAt:`.
- `Behavior>>superclass` (exists), `subclasses`, `allSubclasses` — **computed by
  scanning** all classes' `superclass` (via `Smalltalk allClasses`). No subclass
  registry to maintain (deferred; a registry is a later optimization, not needed
  for a browser).
- **Metadata lives in side dictionaries, NOT new class-object slots.** Adding
  `category`/`comment`/protocol ivars to `Behavior` would be **bootstrap/layout
  work** (the class objects are built during image bootstrap), which is invasive
  and risky. Instead a `SystemOrganization` object holds plain `Dictionary`s
  keyed by class and by `class→selector`:
  *class → category*, *class → comment*, *(class, selector) → protocol*.
  `Behavior>>category` etc. just look themselves up there. This keeps the object
  model untouched and is trivially populated at image build and updated by
  `compile:classified:`.
- **Populate a real `Smalltalk` global**: at image build, fill a `Dictionary`
  (the `SystemDictionary` instance, currently method-less) with `name → class`.
  Add `SystemDictionary>>allClasses`, `classNamed:`, `classNamesDo:`,
  `organization` (→ the `SystemOrganization`).
- **Categories & protocols** are thus a thin view over those dictionaries.
  Defaults: a class's category is its **source-declared** one — the image
  writer records each `category:` from the class-definition chunks in a
  `SystemClassCategories` global (an Array parallel to `SystemClassList`,
  indexed by classIndex) and `SystemOrganization` falls back to it (then to
  `'Kernel'`); a method's protocol is likewise its **source-declared**
  `methodsFor:` group — the image writer stores it (with the source) in the
  method's source-info `Array` (§9.1) and `SystemOrganization` falls back to
  `CompiledMethod>>protocol` (then to `#unclassified`, e.g. for `startUp`).
  `compile:classified:` overrides via the side dictionary. Category, class,
  protocol, and selector lists come back alphabetically sorted.

### 9.3 Live compile / accept
```
Behavior>>compile: sourceString classified: protocolSymbol
  | method |
  method := Compiler new compile: sourceString in: self.   "existing in-image compiler"
  <install via primitive 402: PRIM_METHOD_INSTALL self selector method>
  method sourceInfo: { sourceString. protocolSymbol }.       "retain source (§9.1)"
  self organization classify: selector under: protocolSymbol.
  ^selector
```
**Cache invalidation is a hard contract, gated by a test (do not hand-wave).**
The headline feature is worthless if callers keep hitting the *old* method. So we
make it part of `PRIM_METHOD_INSTALL`'s defined behavior that installing a method
invalidates **both** the global lookup cache **and** any inline caches referencing
the affected selector/class. If it does not already, extend it (or add
`primFlushCaches`). Crucially, we write a **permanent regression test early (in
M1, before any browser work)**: warm a send site by calling it in a loop, install
a replacement method for that selector, then prove subsequent sends observe the
**new** behavior. This test stays forever — later JIT work (see `JIT.md`) can
re-break it. Browser milestones (M4/M5) are **gated** on it being green. *See also
§14.1.*

---

## 10. L5 — The Class Browser (the deliverable)

A classic **five-pane System Browser**, laid into a single Viewer as a
`PanedView`:

```
 ┌──────────── Class Browser (menu: Accept  Cancel  Format …) ────────────┐
 │ categories │ classes │ protocols │ selectors │        [class/instance]  │  ← 4 list panes
 │ ────────── │ ─────── │ ───────── │ ───────── │                          │
 │ Kernel     │ Object  │ accessing │ printOn:  │                          │
 │ Collections│ Array   │ testing   │ size      │                          │
 │ Compiler   │ String  │ private   │ at:put:   │                          │
 │ UI-Gfx     │ Form    │ ...       │ ...       │                          │
 ├────────────┴─────────┴───────────┴───────────┴──────────────────────────┤
 │  printOn: aStream                                                        │
 │      aStream nextPutAll: 'a '; nextPutAll: self class name               │  ← TextPane
 │                                                            (source)      │    (editable)
 └──────────────────────────────────────────────────────────────────────────┘
```

### 10.1 Model — `ClassBrowser`
Holds current category / class / protocol / selector selections and the
class-side vs instance-side switch. Data comes entirely from §9's reflection API.

### 10.2 Wiring
- Category selected → refresh class list (`Smalltalk organization classesIn:`).
- Class selected → refresh protocol list (its `organization protocols`), plus
  show the **class definition** template in the text pane.
- Protocol selected → refresh selector list.
- Selector selected → show `class sourceCodeAt: selector` in the (editable) text
  pane.
- **Accept** (menu word / middle-click / cmd-S) → `class compile: textPane
  contents classified: currentProtocol` (§9.3). **Compile-error presentation is a
  designed interaction, not an afterthought:** the compiler returns a diagnostic
  as `(characterPosition, messageString)`. The browser handles it the classic ST
  way — it **inserts the message text into the source at that position and selects
  it** (so the caret lands on the offending token), leaving the buffer *not*
  accepted; the user edits and re-accepts. (A simpler status-line variant is the
  fallback if inline insertion proves fiddly.) This needs the `TextPane` to
  support "set selection to range [i,j]" — a capability M3 must provide, so it is
  listed as an M3 requirement, not discovered in M5. On success, the selector
  list refreshes and the method is **live immediately**.
- **do-it / print-it**: select text anywhere, pop the operate menu → compile a
  zero-arg doit method in a scratch context, run it; *print-it* appends the
  result's `printString`. (Same mechanism the Workspace uses.)

### 10.3 Companion viewers (small, reuse the toolkit)
- **Transcript** viewer: a read-only `TextPane` fed by `Transcript` (already
  exists — just render its stream). Immediate proof the UI shows live output.
- **Workspace** viewer *(implemented — `st/ui/apps/Workspace.st`, `make
  ui-workspace`)*: an editable `TextPane` with do-it/print-it. Type to edit;
  a left-press–drag sweeps a selection (`TextPane` anchors the drag; the
  supervisor's routing delivers the moves); **right-clicking ON the selected
  text** pops a `MenuPane` operate menu with **do it** / **print it** — do-it
  evaluates the selection (the whole buffer when nothing is selected) via
  `Smalltalk evaluate:`, print-it also inserts `' ' , result printString`
  right after the selection and leaves it selected, so the next keystroke
  replaces it. The host feeds the gestures: `src/host_ui.rs` reports the
  right button as button 2 (event slot `c`) with the same edge-only
  discipline as the left, and typed characters via minifb's input callback
  as key events carrying the character in slot `c` (backspace/enter are
  translated from raw keys to codes 8/10). Tested end-to-end through the
  posted-event pipeline in `st/tests/ui/WorkspaceTests.st`.

---

## 11. Adding the primitives — the exact ritual

For each new/finished primitive (`330` finish, `331` implement, add `332`
BitBlt, `333` SaveDisplay, `323` monotonic-ns clock), follow the established
pattern:
1. **`src/treaty.rs`** — `330`/`331` exist; add `pub const PRIM_BITBLT: u16 =
   332;`, `PRIM_SAVE_FORM = 333`, `PRIM_CLOCK_MONOTONIC_NS = 323`, and add
   each to the Treaty **validation set** (`src/treaty.rs:~540`).
2. **`st/compiler/Treaty.st`** — regenerate the mirror via
   `cargo run --bin gen_treaty_st` (adds `primBitBlt` alongside `primNextEvent`
   / `primPixelBlit`). The bidirectional Treaty tests must stay green.
3. **`src/prims.rs`** — add/replace the `match` arms: real `PRIM_NEXT_EVENT`,
   `PRIM_PIXEL_BLIT`, new `PRIM_BITBLT`. Host-window arms are `#[cfg(feature =
   "ui")]`; the headless arms (in-memory buffer + injected event queue) are
   always compiled.
4. **`src/host_ui.rs`** — new: `HostWindow`, event translation, ARGB expansion,
   headless sink. Held as `Option<…>` on the `Vm` (`src/vm.rs`).
5. **Kernel** — expose via `<primitive: N>` methods (`Display>>flush`,
   `Sensor>>nextEvent`, `BitBlt>>copyBits`).
6. **Tests** — bidirectional Treaty test; a Rust headless test that blits a known
   Form and checks the buffer; a Treaty round-trip. Keep all interactive/bench
   runs wrapped in `timeout` (project rule).

---

## 12. Testing strategy (principled, built in from M0)

Testing is not a phase; it is the same driver + virtual clock + screenshot
substrate from §4A used to assert instead of to explore. Principles: **headless
by default**, **deterministic** (virtual clock, scripted events, 1-bit exact
comparison), **layered** (unit → model → pixel → perf → persistence), and
**gated** (each milestone lands with the tests that prove it).

**The test taxonomy**

1. **Rust primitive tests** (`tests/ui_headless.rs`) — blit a known Form, assert
   the ARGB buffer; PNG encoder round-trips; event encoding.
2. **BitBlt oracle.** The pure-Smalltalk BitBlt fallback (§4.3) is the
   correctness oracle for the Rust primitive: random blits compared both ways,
   must match bit-for-bit.
3. **Model unit tests (SUnit).** `ClassBrowser`, `TextPane` editing,
   `ClassOrganizer`, reflection API, tiling geometry — tested as plain objects
   under the existing Phase-2 SUnit harness, no pixels.
4. **Scenario / golden-screenshot tests.** A `UITestCase` boots a headless
   Display, runs a driver scenario (§4A.2) under the virtual clock, then asserts
   on **(a)** model state (`assert selectorList includes: …`) and **(b)** the
   Display via a **hash of the 1-bit buffer** against a checked-in golden.
   Monochrome + virtual clock ⇒ zero flakiness; a mismatch can also dump the
   actual PNG next to the golden for eyeballing. Goldens live under
   `tests/ui/golden/`; scenarios under `tests/ui/scenarios/`.
5. **Performance-budget tests** (see §13) — assert on **deterministic work
   metrics** (BitBlt count, pixels touched, glyphs, allocations) for a scenario,
   e.g. "selecting a method repaints ≤ N pixels." Timing (wall) is reported, not
   asserted.
6. **Persistence (reinit, not resurrect).** The Smalltalk side — Display `Form`
   bits, the UI `Process`, viewers, models — are ordinary objects and *do*
   survive STIM save/load. The **host-side state does not**: the `minifb` window,
   ARGB scratch buffer, event queue, and `TimeSource` live in the Rust `Vm` and
   are gone on reload. So resume is a **reinit**: on startup the UI process
   discards any assumption of live host state, lets the window/buffer be
   recreated lazily on the first `primPixelBlit`, starts from an empty event
   queue, rebinds the clock, and **forces a full repaint**. The test asserts that
   a snapshot taken mid-session reloads and repaints to the same Display buffer —
   *not* that Rust host objects persist. (This is a later-milestone claim, not an
   early one.)

**Execution**
- `cargo test` runs 1–6 headless with **no `ui` feature and no crates**; the
  UI scenarios themselves are in-image Smalltalk tests (`st/tests/ui/`, run
  by the in-image `TestRunner`, launched by `tests/st_suite.rs`).
- Windowed smoke (`cargo run --features ui -- --ui image.im`) is manual and
  always wrapped in `timeout` (project rule); it is never required for CI.
- Every scenario is committed, so it doubles as living documentation and a repro.

---

## 13. Performance profiling & instrumentation (principled, built in from M0)

The UI is the first *interactive*, redraw-bound workload in the system, so it
carries profiling from the first commit. The guiding principle is a hard split
between two kinds of metric:

- **Work metrics — deterministic, CI-assertable.** Counts of BitBlt calls,
  pixels written, glyphs drawn, events dispatched, dirty rectangles, frames, and
  allocations/GCs during a scenario. Under the virtual clock these are *identical
  every run*, so tests can assert budgets on them (§12, item 5). These extend the
  existing counters infrastructure (`src/counters.rs`, `src/profile.rs`, the
  `vm-counters` feature): add `bitblt_calls`, `pixels_blitted`, `glyphs_drawn`,
  `events_processed`, `frames_presented`.
- **Timing metrics — wall-clock, human-facing.** Per-phase durations measured
  with `primClockMonotonicNs` (§4.4): event-drain, dispatch, layout, paint, blit.
  Non-deterministic, so **reported, never asserted**.

**Frame instrument (`FrameProbe`, Smalltalk).** The UI loop (§6.2) wraps each
phase in the probe. It keeps a fixed-size ring buffer of recent frames (time +
work) — no allocation churn, always on, negligible cost. It can:
- print a **report** (a table: p50/p95 frame time, per-phase share, blits/frame,
  pixels/frame) to the Transcript/stdout, gated by `SMALLISHTALK_STATS` (the
  existing gate) or a `--ui-stats` flag;
- emit a **machine-readable dump** (the deterministic work metrics) for perf
  tests;
- optionally drive an on-screen **HUD viewer** (a tiny viewer showing FPS and
  phase bars) for live windowed debugging.

**Honest phasing.** What can exist in **M0** is the VM-side **work counters** plus
a **machine-readable dump** of them for a scenario — that is real, deterministic,
and CI-assertable from the first commit. The **`FrameProbe`** (per-frame/per-phase
*timing*) necessarily arrives in **M2**, because there is no frame loop before
then; claiming per-frame profiling in M0 would be fiction. So: work counters +
dump in M0; frame/phase timing in M2; the HUD (below) is optional and later.

**Why from the start:** retrofitting instrumentation after the redraw path
solidifies is how UIs end up un-profilable. Having the work counters in M0 and
`FrameProbe` land *with* the loop in M2 means every later milestone can answer
"did that get slower?" with a number, and the perf-budget tests keep it honest as
the toolkit grows. (The JIT will later move the timing numbers; the *work* numbers
are the invariant we design against now.)

---

## 14. Risks & open questions

1. **Cache invalidation on live install** *(highest risk — treat as a gate)*.
   Live "accept" is worthless if callers keep hitting the old method. Make
   flushing the global lookup cache **and** inline caches part of
   `PRIM_METHOD_INSTALL`'s contract, and land the **warm-callsite regression
   test in M1** (§9.3) — long before the browser needs it. M4/M5 are gated on it.
   Keep the test permanently; JIT work can re-break it.
2. **Redraw performance pre-JIT.** Interpreted damage-redraw + BitBlt primitive
   should be fine for a browser at 60 Hz, but full-screen repaints may stutter.
   Mitigations: damage rectangles, XOR cursor, `primPixelBlit:rect:`. Measure
   early (M2). JIT (`JIT.md`) later removes the ceiling; nothing here depends on
   it.
3. **Main-thread window ownership** on macOS/Wayland via minifb. The
   interpreter-on-main-thread design (§3.2) sidesteps it, but confirm minifb's
   behavior on the target platform in M0.
4. **Source retention image growth.** Storing all kernel source enlarges the
   image; make source retention a build option and consider compressing or
   storing source out-of-line if it bites.
5. **Category/protocol metadata.** *(resolved)* Class categories are the
   source-declared ones, carried through the image writer as the
   `SystemClassCategories` table (§9.2); method protocols are the
   source-declared `methodsFor:` groups, carried in each method's source-info
   `Array` (§9.1), with `compile:classified:` overriding via the side
   dictionary.
6. **Font licensing.** *(resolved)* The embedded strike is Adobe Helvetica
   Medium 12 from the X11 75dpi BDF collection (`st/ui/gfx/helvR12.bdf`),
   under the permissive Adobe/DEC notice embedded in the BDF; provenance is
   documented in `DefaultFont.st`.
7. **Keyboard/i18n.** ASCII + a few symbols only to start; Unicode input beyond
   BMP basics is out of scope.

---

## 15. Milestones

Each milestone is independently demoable and testable. **The three pillars land
in M0 and are a standing requirement thereafter:** every milestone below ships
driver scenarios, golden screenshots, and perf counters for what it adds — a
milestone is not "done" until it is operable headlessly, screenshot-tested, and
instrumented.

| # | Milestone | Deliverable / demo | Key files |
|---|---|---|---|
| **M0** | **Host seam + operability foundation** *(all three pillars start here)* | `primPixelBlit`/`primNextEvent`/`primBitBlt`(rules 0/3/6/7)/`primSaveForm:toFile:`(PNG)/`primClockMonotonicNs`; **virtual clock**; **UIDriver** + `--drive/--shots`; base **work counters**; first golden-screenshot test of a hand-built Form; `minifb` window optional behind `ui`; Treaty green. | `Cargo.toml`, `src/host_ui.rs`, `src/png.rs`, `src/prims.rs`, `src/treaty.rs`, `st/compiler/Treaty.st`, `tests/ui_headless.rs` |
| **M1** | **Graphics kernel** | `Form`/`Point`/`Rectangle`/`BitBlt`/`Pen`/`StrikeFont`/`Canvas`; draw shapes + text; golden-image tests. | `st/ui/gfx/*.st`, `st/tools/gen_font_bdf.py` |
| **M2** | **Event loop + tiling WM** | UI `Process` that **drains events every tick independent of redraw**; `FrameProbe` wired into the loop (per-phase timing + work counters); `Display`/`Track`/`Viewer` tiling; open/close/resize viewers; damage+redraw; XOR cursor. Demo (headless, screenshotted): two tiled viewers, resize scenario + golden shots. | `st/ui/wm/*.st`, `st/ui/gfx/Event.st`, `st/ui/testing/FrameProbe.st` |
| **M3** | **Widget toolkit** | `View`/`Controller`/`PanedView`/`ListPane`/`TextPane`(editable, incl. **set-selection-to-range** for compile-error display, §10.2)/`MenuPane`/`ScrollBar`. Demo: a list + editable text viewer; Transcript viewer showing live output. | `st/ui/widgets/*.st` |
| **M4** | **Reflection + source retention** | Stop nilling the source slot; store source; reflection API; populated `Smalltalk`; `compile:classified:` with cache flush. Test: enumerate classes, read source, recompile a method live. | `st/compiler/ImageWriter.st`, `st/kernel/*.st` |
| **M5** | **The Class Browser** | Five-pane live browser in a viewer: navigate → view → **edit → accept (live)**; do-it/print-it; error display. **The headline deliverable.** | `st/ui/apps/ClassBrowser.st`, `st/tools/build_ui_image.st` |
| **M6** *(stretch)* | Workspace + Inspector + multi-track polish + snapshot-with-UI | Usable classic environment; image saved with the browser open reopens to it. | `st/ui/apps/*.st` |

Order of PRs: **straight down the table, M0 → M1 → M2 → M3 → M4 → M5.** (An
earlier draft proposed front-loading M4 as `M0 → M4 → M1`; that is worse — M4
doesn't exercise the graphics/event seam and stacks a second invasive kernel
change ahead of the first end-to-end UI slice.) The **one** thing pulled forward
is the **cache-invalidation regression test** from §9.3/§14.1: write it in **M1**,
because the live browser (M4/M5) is gated on it and it is cheap to land early.

**Explicitly deferred out of v1** (keep the first version small): CLI *semantic*
driver verbs (host→image channel) — Smalltalk scenarios only; the on-screen
profiling **HUD**; a **subclass registry** (scan instead); **class-side metadata
slots** (use side dictionaries, §9.2); WM **grow/full** and any viewer command
the browser doesn't require; color/grayscale depths; multiple fonts.

---

## 16. File map (new & changed)

**Rust**
- `Cargo.toml` — `minifb` optional dep + `ui` feature.
- `src/host_ui.rs` *(new)* — window + headless sink, ARGB present, event
  translation, **scripted event queue**, **virtual clock (`TimeSource`)**.
- `src/png.rs` *(new)* — ~50-line zero-dep PNG writer for `primSaveForm:toFile:`.
- `src/prims.rs` — implement `330`/`331`, add `332` (BitBlt), `333`
  (`primSaveForm:toFile:`), `323` (`primClockMonotonicNs`); feature-gate host arms.
- `src/treaty.rs` — add `PRIM_BITBLT`, `PRIM_SAVE_FORM`,
  `PRIM_CLOCK_MONOTONIC_NS`; extend validation set.
- `src/counters.rs` / `src/profile.rs` — add UI **work counters**
  (`bitblt_calls`, `pixels_blitted`, `glyphs_drawn`, `events_processed`,
  `frames_presented`).
- `src/vm.rs` — hold `Option<HostWindow>`, the (scripted or live) event queue,
  and the selected `TimeSource`.
- `src/main.rs` — `--ui`, `--headless`, `--scenario <name>` (in-image scenario to
  run), `--drive <events-file>` (optional low-level raw-event script),
  `--shots <dir>`, `--ui-stats` launch paths.
- `tests/ui_headless.rs` *(new)* — blit/PNG/oracle/event-injection tests.
- `tests/ui/scenarios/*.txt`, `tests/ui/golden/*.png` *(new)* — scenario +
  golden-screenshot fixtures.

**Smalltalk**
- `st/compiler/Treaty.st` — regenerated mirror (adds `primBitBlt`).
- `st/compiler/ImageWriter.st` — retain method source (was nilling slot).
- `st/kernel/*.st` — reflection API, populated `Smalltalk`, `compile:classified:`,
  `CompiledMethod>>sourceString`.
- `st/ui/gfx/*.st` *(new)* — `Point Rectangle Form BitBlt Pen StrikeFont Canvas
  Event`, `DefaultFont.st`.
- `st/ui/wm/*.st` *(new)* — `Display Track Viewer` + tiling/damage/focus.
- `st/ui/widgets/*.st` *(new)* — `View Controller PanedView ListPane TextPane
  MenuPane ScrollBar`.
- `st/ui/apps/*.st` *(new)* — `ClassBrowser`, later `Workspace`, `Inspector`,
  `TranscriptViewer`.
- `st/ui/testing/*.st` *(new)* — `UIDriver`, `UIScript` (scenario verbs),
  `UITestCase` (SUnit base: boot headless Display, run scenario, assert model +
  golden), `FrameProbe`/`UIProfiler` (instrumentation, §13).
- `st/tools/gen_font_bdf.py` *(new)* — emit the embedded strike font `.st` from a BDF.
- `st/tools/build_ui_image.st` *(new)* — build an image whose `startUp` opens the
  Display and a Class Browser (and a headless variant for the driver/tests).

---

## 17. Summary

The runtime already reserves exactly the seam this needs. The plan is: implement
a handful of primitives — `primPixelBlit`, `primNextEvent`, `primBitBlt`, plus
`primSaveForm:toFile:` (PNG) and a high-res clock for operability — over a
feature-gated `minifb` backend, then build the **entire rest of the UI in
Smalltalk** — `Form`/BitBlt graphics, a bitmap font, an Oberon-style **tiling
window manager**, a small MVC widget kit, and finally a **live Class Browser**
that edits and recompiles methods in the running image. Two supporting changes —
**retaining method source** (un-nil the reserved slot) and a **reflection API** —
turn the existing in-image compiler into a live programming environment.

Three concerns are foundational, not afterthoughts: the UI is **headless-first
and operable by a driver script with PNG screenshots** (so a person or an agent
can run it and look at it without a display), and **profiling** and **automated
testing** are built in from M0 on a shared deterministic (virtual-clock)
substrate — with a deliberate split between deterministic *work* metrics (CI
asserts these) and wall-clock *timing* metrics (humans read these). It adds one
optional external dependency, no new concurrency model, and nothing that depends
on the JIT.
