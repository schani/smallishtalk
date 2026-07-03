//! M4 — source retention + the reflection API (UI.md §9), tested on the VM.
//! Enumerate classes, read retained method source, walk subclasses, and query
//! the SystemOrganization. Same gst-built UI-image pattern as tests/ui_gfx.rs.

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
    let dir = std::env::temp_dir().join(format!("smallishtalk-uiref-{}-{name}", std::process::id()));
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
        "reflection image build failed:\n{stdout}\n{}",
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
fn enumerate_classes_and_selectors() {
    let driver = r#"
| arr obj |
Transcript showCr: (Smalltalk allClasses size > 30) printString.
arr := Smalltalk classNamed: 'Array'.
Transcript showCr: arr name asString.
Transcript showCr: (arr includesSelector: #printString) printString.
Transcript showCr: (arr includesSelector: #fooBarBaz) printString.
obj := Smalltalk classNamed: 'Object'.
Transcript showCr: (obj includesSelector: #yourself) printString.
Transcript showCr: ((Smalltalk classNamed: 'Boolean') subclasses size) printString.
Transcript showCr: obj category.
"#;
    let expected = "true\nArray\ntrue\nfalse\ntrue\n2\nKernel\n";
    assert_eq!(drive("enum", driver), expected);
}

#[test]
fn retained_method_source_is_readable() {
    // The reserved source slot is no longer nilled: the exact chunk source of a
    // kernel method round-trips into the image and is readable by reflection.
    let driver = r#"
| src |
src := (Smalltalk classNamed: 'Object') sourceCodeAt: #yourself.
Transcript showCr: (src isNil) printString.
Transcript show: src.
Transcript showCr: '<<'.
"#;
    // Object>>yourself's retained chunk source is "\nyourself\n\t^self" (the
    // leading newline is the chunk boundary). Locked verbatim.
    let expected = "false\n\nyourself\n\t^self<<\n";
    assert_eq!(drive("source", driver), expected);
}

#[test]
fn organization_classifies_and_queries() {
    let driver = r#"
| org arr |
org := Smalltalk organization.
arr := Smalltalk classNamed: 'Array'.
org classify: arr under: 'Collections-Sequenceable'.
Transcript showCr: (org categoryOf: arr).
Transcript showCr: ((org classesIn: 'Collections-Sequenceable') size) printString.
org classify: #printString under: #printing for: arr.
Transcript showCr: (org protocolOf: arr selector: #printString) printString.
Transcript showCr: (org protocolOf: arr selector: #size) printString.
"#;
    // Array reclassified; printOn: gets #printing, unspecified selectors default.
    let expected = "Collections-Sequenceable\n1\n#printing\n#unclassified\n";
    assert_eq!(drive("org", driver), expected);
}

#[test]
fn live_compile_and_accept() {
    // The headline: compile a method live, run it, recompile it, and see a
    // warm caller observe the NEW behavior (gated by live_install_test). Plus
    // do-it evaluation. This exercises the full runtime reifier.
    let driver = r#"
Object compile: 'uiTestAnswer ^42' classified: #testing.
Transcript showCr: (Object new uiTestAnswer) printString.
Object compile: 'uiTestAnswer ^99' classified: #testing.
Transcript showCr: (Object new uiTestAnswer) printString.
Transcript show: (Object sourceCodeAt: #uiTestAnswer).
Transcript showCr: '<<'.
Transcript showCr: (Smalltalk evaluate: '3 + 4') printString.
Transcript showCr: (Smalltalk evaluate: '[:a :b | a + b] value: 10 value: 5') printString.
"#;
    let expected = "42\n99\nuiTestAnswer ^99<<\n7\n15\n";
    assert_eq!(drive("live", driver), expected);
}
