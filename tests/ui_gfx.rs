//! M1 — the Smalltalk graphics kernel (UI.md §5), tested end-to-end on the VM.
//!
//! A driver program is cross-compiled (with the kernel + the st/ui/gfx/ layer)
//! into an image by the GST-side compiler, then executed by this VM; captured
//! stdout is diffed against the expected transcript. This is the corpus pattern
//! (see tests/corpus_test.rs) specialized to the UI image, so the graphics
//! classes run as real compiled Smalltalk over the M0 primitives — including
//! the BitBlt primitive-vs-fallback oracle (UI.md §12 item 2).

use smallishtalk::vm::{Vm, VmConfig};
use std::path::Path;
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

/// Cross-compile the kernel + gfx layer + `driver_src` into an image via GST.
fn build_ui_image(driver_src: &str, image_path: &Path) {
    std::fs::create_dir_all(image_path.parent().unwrap()).unwrap();
    let driver_path = image_path.with_extension("driver.st");
    std::fs::write(&driver_path, driver_src).unwrap();
    let tool = format!("{}/st/tools/build_ui_image.st", root());
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_sources())
        .arg(&tool)
        .arg("-a")
        .arg(&driver_path)
        .arg(image_path)
        .current_dir(root())
        .output()
        .expect("run gst (is GNU Smalltalk installed?)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IMAGE-WRITTEN"),
        "gfx image build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_image(image: &Path) -> String {
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).expect("load image");
    let active = vm.active_process;
    vm.run(active).expect("run image");
    String::from_utf8_lossy(&std::mem::take(&mut vm.stdout_capture)).into_owned()
}

/// Build + run a driver, returning its transcript. One gst invocation per call.
fn drive(name: &str, driver_src: &str) -> String {
    // Per-test dir (name + pid) so parallel cargo test threads don't collide.
    let dir = std::env::temp_dir().join(format!("smallishtalk-uigfx-{}-{name}", std::process::id()));
    let image = dir.join(format!("{name}.im"));
    build_ui_image(driver_src, &image);
    let out = run_image(&image);
    std::fs::remove_dir_all(&dir).ok();
    out
}

#[test]
fn gfx_kernel_end_to_end() {
    let driver = r#"
| p q r dest src destA destB bb bbf |
p := Point x: 3 y: 4.
q := Point x: 10 y: 1.
Transcript showCr: (p + q) printString.
Transcript showCr: (p max: q) printString.
r := Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 8 y: 4).
Transcript showCr: r area printString.
Transcript showCr: (r containsPoint: (Point x: 7 y: 3)) printString.
Transcript showCr: (r containsPoint: (Point x: 8 y: 0)) printString.
dest := Form width: 8 height: 2.
src := Form width: 8 height: 2.
src fill: 1.
dest copyFrom: src boundingBox in: src to: (Point x: 0 y: 0) rule: 3.
Transcript show: dest asAsciiString.
dest copyFrom: src boundingBox in: src to: (Point x: 0 y: 0) rule: 6.
Transcript show: dest asAsciiString.
destA := Form width: 8 height: 1.
destB := Form width: 8 height: 1.
destA bits at: 1 put: 85.
destB bits at: 1 put: 85.
src := Form width: 8 height: 1.
src bits at: 1 put: 178.
bb := BitBlt new.
bb destForm: destA. bb sourceForm: src. bb combinationRule: 7.
bb destX: 0. bb destY: 0. bb width: 8. bb height: 1.
bb sourceX: 0. bb sourceY: 0. bb clipX: 0. bb clipY: 0. bb clipWidth: 8. bb clipHeight: 1.
bb copyBits.
bbf := BitBlt new.
bbf destForm: destB. bbf sourceForm: src. bbf combinationRule: 7.
bbf destX: 0. bbf destY: 0. bbf width: 8. bbf height: 1.
bbf sourceX: 0. bbf sourceY: 0. bbf clipX: 0. bbf clipY: 0. bbf clipWidth: 8. bbf clipHeight: 1.
bbf copyBitsFallback.
Transcript showCr: 'oracle: ' , (destA bits = destB bits) printString.
Transcript show: destA asAsciiString.
"#;
    let expected = "\
13@5
10@4
32
true
false
########
########
........
........
oracle: true
####.###
";
    assert_eq!(drive("gfx", driver), expected);
}

#[test]
fn text_pen_and_canvas_render() {
    // Golden-screenshot substrate (UI.md §12 item 4): deterministic 1-bit
    // rendering asserted as ASCII art. Text via the baked-in strike font,
    // a Bresenham diagonal, and a Canvas rectangle frame.
    let driver = r#"
| f pen c |
f := Form width: 17 height: 14.
StrikeFont default displayString: 'AB' on: f at: (Point x: 0 y: 0).
Transcript show: f asAsciiString.
Transcript showCr: '--'.
f := Form width: 5 height: 5.
pen := Pen on: f.
pen place: (Point x: 0 y: 0).
pen goto: (Point x: 4 y: 4).
Transcript show: f asAsciiString.
Transcript showCr: '--'.
f := Form width: 6 height: 5.
c := Canvas on: f.
c frameRectangle: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 6 y: 5)) value: 1.
Transcript show: f asAsciiString.
"#;
    // 'A' has a 9px advance, 'B' 8px; caps sit on the baseline at row 10
    // (ascent 11), with the 3 descent rows blank.
    let expected = "\
.................
.................
....#.....#####..
...#.#....#....#.
...#.#....#....#.
..#...#...#....#.
..#...#...#####..
..#####...#....#.
.#.....#..#....#.
.#.....#..#....#.
.#.....#..#####..
.................
.................
.................
--
#....
.#...
..#..
...#.
....#
--
######
#....#
#....#
#....#
######
";
    assert_eq!(drive("text", driver), expected);
}
