//! M5 — the Class Browser (UI.md §10), the headline deliverable, tested on the
//! VM. Navigate categories -> classes -> protocols -> selectors -> retained
//! source; edit and ACCEPT to recompile live; do-it. Same gst-built UI-image
//! pattern as tests/ui_gfx.rs.

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
    let dir = std::env::temp_dir().join(format!("smallishtalk-uibr-{}-{name}", std::process::id()));
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
        "browser image build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).expect("load");
    let active = vm.active_process;
    vm.run(active).expect("run");
    let result = String::from_utf8_lossy(&std::mem::take(&mut vm.stdout_capture)).into_owned();
    std::fs::remove_dir_all(&dir).ok();
    result
}

/// Build+run a driver with host events injected before the run, so the image's
/// pumpEvents drains them (used to test click routing).
fn drive_with_events(name: &str, driver_src: &str, events: &[[i64; 5]]) -> String {
    let dir = std::env::temp_dir().join(format!("smallishtalk-uibr-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join(format!("{name}.im"));
    let driver_path = dir.join(format!("{name}.driver.st"));
    std::fs::write(&driver_path, driver_src).unwrap();
    let tool = format!("{}/st/tools/build_ui_image.st", root());
    let out = Command::new("gst")
        .arg("-Q").args(compiler_sources()).arg(&tool)
        .arg("-a").arg(&driver_path).arg(&image)
        .current_dir(root()).output().expect("run gst");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("IMAGE-WRITTEN"),
        "build failed:\n{}", String::from_utf8_lossy(&out.stderr)
    );
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).expect("load");
    for e in events {
        vm.host.push_event(*e);
    }
    let active = vm.active_process;
    vm.run(active).expect("run");
    let result = String::from_utf8_lossy(&std::mem::take(&mut vm.stdout_capture)).into_owned();
    std::fs::remove_dir_all(&dir).ok();
    result
}

#[test]
fn browser_navigates_the_reflection_model() {
    let driver = r#"
| b |
Smalltalk organization classify: (Smalltalk classNamed: 'Array') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'String') under: 'Collections'.
b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
Transcript showCr: (b categoryNames includes: 'Collections') printString.
b selectCategoryNamed: 'Collections'.
Transcript showCr: b classNames size printString.
Transcript showCr: (b classNames at: 1).
b selectClassNamed: 'Array'.
Transcript showCr: (b protocolNames includes: 'unclassified') printString.
b selectProtocolNamed: 'unclassified'.
Transcript showCr: (b selectorNames includes: 'printString') printString.
b selectSelectorNamed: 'printString'.
Transcript showCr: (b sourcePane contents size > 0) printString.
Transcript showCr: (b currentClass name asString).
"#;
    // Collections has Array+String; Array's own methods default to
    // 'unclassified'; printString source is retained and non-empty.
    // classNames is class-index order, so String precedes Array.
    let expected = "true\n2\nString\ntrue\ntrue\ntrue\nArray\n";
    assert_eq!(drive("nav", driver), expected);
}

#[test]
fn browser_accept_recompiles_live() {
    // The headline gesture: edit the source pane, accept, and the method is
    // installed live — a subsequent send observes it, and the selector list
    // refreshes to include it.
    let driver = r#"
| b |
Smalltalk organization classify: (Smalltalk classNamed: 'Array') under: 'Collections'.
b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
b selectCategoryNamed: 'Collections'.
b selectClassNamed: 'Array'.
b selectProtocolNamed: 'unclassified'.
b sourcePane contents: 'uiBrowserAccept ^777'.
b accept.
Transcript showCr: ((Array new: 3) uiBrowserAccept) printString.
Transcript showCr: (b selectorNames includes: 'uiBrowserAccept') printString.
b sourcePane contents: '6 * 7'.
Transcript showCr: b doIt printString.
"#;
    let expected = "777\ntrue\n42\n";
    assert_eq!(drive("accept", driver), expected);
}

#[test]
fn browser_renders_a_framed_five_pane_layout() {
    // Structural render check: the browser paints a full outer frame, a
    // horizontal rule above the source pane, and 3 vertical column rules.
    let driver = r#"
| b f c top |
Smalltalk organization classify: (Smalltalk classNamed: 'Array') under: 'Collections'.
b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 40 y: 30)).
b selectCategoryNamed: 'Collections'.
f := Form width: 40 height: 30.
c := Canvas on: f.
b displayOn: c.
"top border row fully inked"
top := true.
0 to: 39 do: [:x | (f pixelValueAtX: x y: 0) = 1 ifFalse: [top := false]].
Transcript showCr: 'topBorder=' , top printString.
"the list/source divider is at 2/5 of height = row 12"
Transcript showCr: 'divider=' , (f pixelValueAtX: 20 y: 12) printString.
"a column rule exists at x = width//4 = 10 within the list area"
Transcript showCr: 'col=' , (f pixelValueAtX: 10 y: 5) printString.
"#;
    assert_eq!(drive("render", driver), "topBorder=true\ndivider=1\ncol=1\n");
}

#[test]
fn browser_opens_and_screenshots_the_full_stack() {
    // The agent/dev workflow (UI.md §4A): navigate the browser to a real method
    // and save the whole rendered screen to a viewable PNG — exercising every
    // layer L0..L5 headlessly (reflection -> widgets -> BitBlt/font -> PNG seam).
    let dir = std::env::temp_dir().join(format!("smallishtalk-uibr-shot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("browser.png");
    let driver = format!(
        r#"
| b f |
Smalltalk organization classify: (Smalltalk classNamed: 'Object') under: 'Kernel'.
b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 320 y: 200)).
b selectCategoryNamed: 'Kernel'.
b selectClassNamed: 'Object'.
b selectProtocolNamed: 'unclassified'.
b selectSelectorNamed: 'yourself'.
f := Form width: 320 height: 200.
b displayOn: (Canvas on: f).
f saveTo: '{}'.
Transcript showCr: 'saved ' , (b currentSelector) asString.
"#,
        png.display()
    );
    let out = drive("shot", &driver);
    assert_eq!(out, "saved yourself\n");
    let bytes = std::fs::read(&png).expect("browser PNG written");
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10], "valid PNG screenshot");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_do_it_and_print_it() {
    // M6: the Workspace — type an expression and evaluate it live.
    let driver = r#"
| w |
w := Workspace bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 100 y: 50)).
w contents: '3 * 14'.
Transcript showCr: w doIt printString.
w contents: '10 - 3'.
w printIt.
Transcript showCr: w contents.
"#;
    assert_eq!(drive("workspace", driver), "42\n10 - 3 7\n");
}

#[test]
fn browser_click_navigates() {
    // A real mouse-down routed through the UISupervisor -> the browser -> the
    // class list pane under the pointer selects a class and navigation fires.
    // This is what makes a windowed browser interactive; tested headlessly.
    let driver = r#"
| d b s |
Smalltalk organization classify: (Smalltalk classNamed: 'Array') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'String') under: 'Collections'.
d := Display width: 240 height: 120.
b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 240 y: 120)).
b selectCategoryNamed: 'Collections'.
s := UISupervisor on: d.
s rootView: b.
s pumpEvents.
Transcript showCr: b currentClass isNil printString.
b currentClass notNil ifTrue: [Transcript showCr: b currentClass name asString].
Transcript showCr: b protocolNames isEmpty printString.
"#;
    // The class-list pane occupies x[60,120), y[0,48); a click at (65,4) hits
    // its first row = String (class-index order in 'Collections').
    let events = [[2i64, 65, 4, 1, 1]]; // mouse button, x=65 y=4, left, down
    let out = drive_with_events("click", driver, &events);
    assert_eq!(out, "false\nString\nfalse\n");
}
