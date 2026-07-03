//! M2 — the event loop + Oberon-style tiling window manager (UI.md §6, §7),
//! tested headlessly on the VM. Same gst-built-image pattern as tests/ui_gfx.rs.
//! Proves: viewers tile to fill a track; open/resize re-tile; events are drained
//! every tick independent of redraw (UI.md §3.2); and drawing has a deterministic
//! work-counter budget (UI.md §12 item 5, §13).

use smallishtalk::vm::{Vm, VmConfig};
use std::process::Command;

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn compiler_sources() -> Vec<String> {
    [
        "Compat.st", "Treaty.st", "Platform.st", "AST.st", "Lexer.st", "Parser.st",
        "ChunkReader.st", "CodeGen.st", "Encoder.st", "ImageWriter.st", "Compiler.st",
    ]
    .iter()
    .map(|f| format!("{}/st/compiler/{}", root(), f))
    .collect()
}

fn build_ui_image(name: &str, driver_src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smallishtalk-uiwm-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join(format!("{name}.im"));
    let driver_path = dir.join(format!("{name}.driver.st"));
    std::fs::write(&driver_path, driver_src).unwrap();
    let tool = format!("{}/st/tools/build_ui_image.st", root());
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_sources())
        .arg(&tool)
        .arg("-a")
        .arg(&driver_path)
        .arg(&image)
        .current_dir(root())
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IMAGE-WRITTEN"),
        "wm image build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    image
}

/// Build+run a driver with no injected events; return its transcript.
fn drive(name: &str, driver_src: &str) -> String {
    drive_with_events(name, driver_src, &[])
}

/// Build+run, pushing host events into the queue before running (so the image's
/// pumpEvents drains them). Returns the transcript.
fn drive_with_events(name: &str, driver_src: &str, events: &[[i64; 5]]) -> String {
    let image = build_ui_image(name, driver_src);
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).expect("load");
    for e in events {
        vm.host.push_event(*e);
    }
    let active = vm.active_process;
    vm.run(active).expect("run");
    let out = String::from_utf8_lossy(&std::mem::take(&mut vm.stdout_capture)).into_owned();
    std::fs::remove_dir_all(image.parent().unwrap()).ok();
    out
}

#[test]
fn two_viewers_tile_to_fill_the_track() {
    let driver = r#"
| d t |
d := Display width: 16 height: 24.
t := d addTrackLeft: 0 width: 16.
t open: (Viewer named: 'A').
t open: (Viewer named: 'B').
d cursorX: 100 y: 100.
d draw.
Transcript show: d asAsciiString.
"#;
    // Two viewers, each 12 tall, stacked; each: top border, name glyph in the
    // menu strip, a menu-strip rule at y=10, a bottom border. Cursor parked
    // off-screen so this asserts pure tiling. (A at rows 0-11, B at 12-23.)
    let expected = "\
################
#..###.........#
#.#...#........#
#.#...#........#
#.#####........#
#.#...#........#
#.#...#........#
#.#...#........#
#..............#
#..............#
################
################
################
#.####.........#
#.#...#........#
#.#...#........#
#.####.........#
#.#...#........#
#.#...#........#
#.####.........#
#..............#
#..............#
################
################
";
    assert_eq!(drive("tile", driver), expected);
}

#[test]
fn resize_moves_the_split_between_viewers() {
    let driver = r#"
| d t a b |
d := Display width: 16 height: 24.
t := d addTrackLeft: 0 width: 16.
a := Viewer named: 'A'.
b := Viewer named: 'B'.
t open: a.
t open: b.
"move the split up: upper neighbor (a) grows by 4, b shrinks by 4"
t resize: b by: 4.
d draw.
Transcript showCr: 'a=' , a height printString , ' b=' , b height printString.
Transcript showCr: 'aBounds=' , a bounds printString.
Transcript showCr: 'bBounds=' , b bounds printString.
"#;
    let expected = "\
a=16 b=8
aBounds=0@0 corner: 16@16
bBounds=0@16 corner: 16@24
";
    assert_eq!(drive("resize", driver), expected);
}

#[test]
fn events_drain_every_tick_independent_of_redraw() {
    // A mouse-move then a close event are injected. pumpEvents must drain BOTH
    // with no blit in between (UI.md §3.2): the cursor moves and running goes
    // false. Nothing is drawn here at all.
    let driver = r#"
| d s |
d := Display width: 40 height: 40.
s := UISupervisor on: d.
s pumpEvents.
Transcript showCr: 'cursor: ' , d cursorX printString , ',' , d cursorY printString.
Transcript showCr: 'running: ' , s running printString.
"#;
    let events = [[1i64, 12, 7, 0, 0], [7, 0, 0, 0, 0]]; // move to 12,7 ; close
    let out = drive_with_events("events", driver, &events);
    assert_eq!(out, "cursor: 12,7\nrunning: false\n");
}

#[test]
fn draw_has_a_deterministic_work_budget() {
    // One runStep drawing two named viewers. Everything blits now: 1 whole-
    // form clear + per viewer (4 frame edges + 1 separator line + 1 name
    // glyph) x 2 + 1 XOR cursor = 14 BitBlt calls, and 1 frame presented.
    // These are the deterministic WORK metrics CI asserts (timing is only
    // reported).
    let driver = r#"
| d t s |
d := Display width: 32 height: 24.
t := d addTrackLeft: 0 width: 32.
t open: (Viewer named: 'A').
t open: (Viewer named: 'B').
s := UISupervisor on: d.
Profiler resetCounters.
s runStep.
Transcript showCr: 'blits=' , (s probe counterNamed: 'ui.bitblt_calls') printString.
Transcript showCr: 'frames=' , (s probe counterNamed: 'ui.frames_presented') printString.
"#;
    let out = drive("perf", driver);
    assert_eq!(out, "blits=14\nframes=1\n");
}

#[test]
fn xor_cursor_is_visible_and_reversible() {
    // The cursor is a small Form blitted with XOR (rule 6, UI.md §7.4): drawing
    // it changes the screen; blitting it a second time restores the screen
    // exactly, with no saved background.
    let driver = r#"
| d blank withCursor erased |
d := Display width: 12 height: 6.
d cursorX: 100 y: 100.
d draw.
blank := d asAsciiString.
d cursorX: 2 y: 1.
d draw.
withCursor := d asAsciiString.
d blitCursorXor.
erased := d asAsciiString.
Transcript showCr: (withCursor = blank) printString.
Transcript showCr: (erased = blank) printString.
"#;
    assert_eq!(drive("cursor", driver), "false\ntrue\n");
}
