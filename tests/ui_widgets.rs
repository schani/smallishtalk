//! M3 — the widget toolkit (UI.md §8), tested headlessly on the VM. Model
//! behavior (editing, selection, list navigation, layout) is asserted through
//! deterministic transcripts; one reverse-video render is a golden. Same
//! gst-built-image pattern as tests/ui_gfx.rs.

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

fn drive(name: &str, driver_src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("smallishtalk-uiw-{}-{name}", std::process::id()));
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
        "widget image build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).expect("load");
    let active = vm.active_process;
    vm.run(active).expect("run");
    let result = String::from_utf8_lossy(&std::mem::take(&mut vm.stdout_capture)).into_owned();
    std::fs::remove_dir_all(&dir).ok();
    result
}

#[test]
fn textpane_editing_selection_and_clipboard() {
    let driver = r#"
| tp |
tp := TextPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 40)).
tp contents: 'hello'.
tp caretAt: 5.
tp insertString: ' world'.
Transcript showCr: tp contents.
tp selectFrom: 0 to: 5.
Transcript showCr: tp selectedText.
tp cut.
Transcript showCr: tp contents.
tp caretAt: 0.
tp paste.
Transcript showCr: tp contents.
tp selectFrom: 6 to: 11.
tp backspace.
Transcript showCr: tp contents.
Transcript showCr: tp selectionStart printString.
tp contents: 'abc'.
tp caretAt: 3.
tp insertCharacter: 100.
Transcript showCr: tp contents.
tp backspace.
Transcript showCr: tp contents.
"#;
    // Note the trailing space on "hello " — deleting the selected "world" from
    // "hello world" correctly leaves the space. Written inline so no linter
    // strips the significant trailing space.
    let expected = "hello world\nhello\n world\nhello world\nhello \n6\nabcd\nabc\n";
    assert_eq!(drive("edit", driver), expected);
}

#[test]
fn textpane_select_range_lands_on_a_token() {
    // The compile-error interaction (UI.md §10.2): set the selection to the
    // offending token's range so the caret lands on it.
    let driver = r#"
| tp |
tp := TextPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 40)).
tp contents: 'foo := bar baz'.
tp selectFrom: 7 to: 10.
Transcript showCr: tp selectedText.
Transcript showCr: tp selectionStart printString , '-' , tp selectionStop printString.
"#;
    assert_eq!(drive("range", driver), "bar\n7-10\n");
}

#[test]
fn listpane_selection_navigation_and_click() {
    let driver = r#"
| lp log |
StrikeFont useClassic.
lp := ListPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 60 y: 40)).
log := OrderedCollection new.
lp items: (Array with: 'Object' with: 'Array' with: 'String').
lp onSelect: [:i | log add: i].
lp select: 2.
Transcript showCr: lp selectedItem.
lp selectNext.
Transcript showCr: lp selectedItem.
lp selectPrevious.
lp selectPrevious.
Transcript showCr: lp selectedItem.
Transcript showCr: (lp selectAtPoint: (Point x: 3 y: 12)) printString.
Transcript showCr: lp selectedItem.
Transcript showCr: log size printString.
"#;
    let expected = "Array\nString\nObject\ntrue\nArray\n5\n";
    assert_eq!(drive("list", driver), expected);
}

#[test]
fn panedview_splits_bounds_equally() {
    let driver = r#"
| pv a b |
pv := PanedView bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 30 y: 10)).
pv horizontal.
a := View bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 1 y: 1)).
b := View bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 1 y: 1)).
pv addPane: a.
pv addPane: b.
pv layout.
Transcript showCr: a bounds printString.
Transcript showCr: b bounds printString.
"#;
    assert_eq!(
        drive("paned", driver),
        "0@0 corner: 15@10\n15@0 corner: 30@10\n"
    );
}

#[test]
fn menupane_resolves_command_at_point() {
    let driver = r#"
| m |
StrikeFont useClassic.
m := MenuPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 40 y: 30)).
m commands: (Array with: #Accept with: #Cancel with: #Format).
Transcript showCr: (m commandAtPoint: (Point x: 5 y: 2)) printString.
Transcript showCr: (m commandAtPoint: (Point x: 5 y: 11)) printString.
Transcript showCr: (m commandAtPoint: (Point x: 5 y: 100)) printString.
"#;
    assert_eq!(drive("menu", driver), "#Accept\n#Cancel\nnil\n");
}

#[test]
fn scrollbar_thumb_tracks_position() {
    let driver = r#"
| sb |
sb := ScrollBar bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 5 y: 100)).
sb total: 200 visible: 50 position: 0.
Transcript showCr: sb thumbRect printString.
sb total: 200 visible: 50 position: 100.
Transcript showCr: sb thumbRect printString.
"#;
    assert_eq!(
        drive("scroll", driver),
        "0@0 corner: 5@25\n0@50 corner: 5@75\n"
    );
}

#[test]
fn listpane_selected_row_is_reverse_video() {
    // The selected row draws as a black bar with white (XOR) glyphs; the others
    // draw normally. Golden-verified 1-bit render.
    let driver = r#"
| f lp c |
StrikeFont useClassic.
f := Form width: 24 height: 29.
lp := ListPane bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 24 y: 29)).
lp items: (Array with: 'AB' with: 'CD' with: 'EF').
lp select: 2.
c := Canvas on: f.
lp displayOn: c.
Transcript show: f asAsciiString.
"#;
    // Items 'AB' (rows 2-10, normal) and 'EF' (rows 20-28, normal) draw black-on-
    // white; the selected 'CD' (rows 11-18) is a black bar with the glyphs XORed
    // to white — reverse video. Row 18 is the bar's fully-inked base line. The
    // top two rows are the pane's topInset.
    let expected = "\
........................
........................
..###....####...........
.#...#...#...#..........
.#...#...#...#..........
.#####...####...........
.#...#...#...#..........
.#...#...#...#..........
.#...#...####...........
........................
........................
##...####...############
#.###.###.##.###########
#.#######.###.##########
#.#######.###.##########
#.#######.###.##########
#.###.###.##.###########
##...####...############
########################
........................
.#####...#####..........
.#.......#..............
.#.......#..............
.####....####...........
.#.......#..............
.#.......#..............
.#####...#..............
........................
........................
";
    assert_eq!(drive("lprender", driver), expected);
}
