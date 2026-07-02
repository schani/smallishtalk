//! Phase 2→3 bridge test: a mini-image cross-compiled by the GST heap
//! writer loads in the Rust VM and *executes* — relocation, frame layout,
//! method headers, and bytecode all agree across the two codebases.

use smallishtalk::value::Value;
use smallishtalk::vm::{Vm, VmConfig};
use std::process::Command;

pub fn gst_compile_program(program: &str, image_path: &std::path::Path) {
    let root = env!("CARGO_MANIFEST_DIR");
    let dir = image_path.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let script = dir.join(format!(
        "build_{}.st",
        image_path.file_stem().unwrap().to_string_lossy()
    ));
    std::fs::write(
        &script,
        format!(
            r#"| b |
b := StImageBuilder new.
b writer defineClass: 'Object' superclass: nil instVarNames: OrderedCollection new.
b writer defineClass: 'UndefinedObject' superclass: 'Object' instVarNames: OrderedCollection new.
b writer defineClass: 'True' superclass: 'Object' instVarNames: OrderedCollection new.
b writer defineClass: 'False' superclass: 'Object' instVarNames: OrderedCollection new.
b writer defineClass: 'SystemDictionary' superclass: 'Object' instVarNames: OrderedCollection new.
b programSource: '{}'.
b writeTo: '{}'.
Transcript showCr: 'IMAGE-WRITTEN'.
"#,
            program.replace('\'', "''"),
            image_path.display()
        ),
    )
    .unwrap();
    let out = Command::new("gst")
        .arg("-Q")
        .args([
            &format!("{root}/st/compiler/Compat.st"),
            &format!("{root}/st/compiler/Treaty.st"),
            &format!("{root}/st/compiler/Platform.st"),
            &format!("{root}/st/compiler/AST.st"),
            &format!("{root}/st/compiler/Lexer.st"),
            &format!("{root}/st/compiler/Parser.st"),
            &format!("{root}/st/compiler/CodeGen.st"),
            &format!("{root}/st/compiler/Encoder.st"),
            &format!("{root}/st/compiler/ImageWriter.st"),
            &format!("{root}/st/compiler/Compiler.st"),
            &format!("{root}/st/compiler/ChunkReader.st"),
        ])
        .arg(&script)
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IMAGE-WRITTEN"),
        "gst failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_mini(program: &str, name: &str) -> Value {
    let dir = std::env::temp_dir().join(format!("smallishtalk-mini-{}", std::process::id()));
    let image = dir.join(format!("{name}.im"));
    gst_compile_program(program, &image);
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).unwrap();
    let active = vm.active_process;
    vm.run(active).unwrap()
}

#[test]
fn cross_compiled_arithmetic_executes() {
    assert_eq!(run_mini("^6 * 7", "mul"), Value::from_int(42));
}

#[test]
fn cross_compiled_temps_and_control_flow() {
    assert_eq!(
        run_mini(
            "| s i | s := 0. i := 10. [i > 0] whileTrue: [s := s + i. i := i - 1]. ^s",
            "loop"
        ),
        Value::from_int(55)
    );
}

#[test]
fn cross_compiled_comparison_returns_heap_true() {
    let dir = std::env::temp_dir().join(format!("smallishtalk-mini-{}", std::process::id()));
    let image = dir.join("cmp.im");
    gst_compile_program("^3 < 4", &image);
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).unwrap();
    let active = vm.active_process;
    let r = vm.run(active).unwrap();
    assert_eq!(r, vm.true_v(), "the image's true singleton");
}
