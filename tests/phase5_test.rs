//! Phase 5 (SPEC §20): self-hosting and the payoff test.
//!
//! The self-host image contains the kernel plus the compiler's own source,
//! cross-compiled by GST. Running it, the in-image compiler compiles the
//! entire corpus and the kernel; outputs must be **bit-identical** to the
//! GST-cross-compiled outputs. The closure test goes one step further: the
//! in-image compiler compiles the compiler itself, and that output too is
//! bit-identical — after which the in-image compiler is the system
//! compiler and GST is bootstrap-only.

use smallishtalk::vm::{Vm, VmConfig};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn compiler_files() -> Vec<String> {
    [
        "Compat.st",
        "Treaty.st",
        "Platform.st",
        "AST.st",
        "Lexer.st",
        "Parser.st",
        "ChunkReader.st",
        "CodeGen.st",
        "Encoder.st",
        "ImageWriter.st",
        "Compiler.st",
    ]
    .iter()
    .map(|f| format!("{}/st/compiler/{}", root(), f))
    .collect()
}

fn gst_tool(tool: &str, args: &[&str]) {
    let out = Command::new("gst")
        .arg("-Q")
        .args(compiler_files())
        .arg(format!("{}/st/tools/{}", root(), tool))
        .arg("-a")
        .args(args)
        .current_dir(root())
        .output()
        .expect("run gst");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("-WRITTEN"),
        "gst {tool} {args:?} failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The driver program that runs inside the self-host image: compile
/// kernel+program and write the result.
fn corpus_driver(program: &Path, out_image: &Path) -> String {
    format!(
        "| b |\n\
         b := StImageBuilder new.\n\
         b fileInFile: '{}/st/kernel/kernel.st'.\n\
         b programSource: (Platform readFile: '{}').\n\
         b writeTo: '{}'.\n\
         Transcript showCr: 'IN-IMAGE-COMPILE-DONE'\n",
        root(),
        program.display(),
        out_image.display()
    )
}

fn run_image_collect(image: &Path) -> Vec<u8> {
    let mut vm = Vm::load_image(image.to_str().unwrap(), VmConfig::default()).unwrap();
    let active = vm.active_process;
    vm.run_until_idle(active).unwrap();
    std::mem::take(&mut vm.stdout_capture)
}

#[test]
fn payoff_in_image_compiler_output_is_bit_identical() {
    let dir = std::env::temp_dir().join(format!("smallishtalk-p5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let corpus_dir = PathBuf::from(root()).join("corpus");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&corpus_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "st"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty());

    let kernel = format!("{}/st/kernel/kernel.st", root());
    let mut failures = Vec::new();
    for program in &entries {
        let name = program.file_stem().unwrap().to_string_lossy().into_owned();
        // Reference: GST cross-compiles the target image.
        let reference = dir.join(format!("{name}.gst.im"));
        gst_tool(
            "build_corpus_image.st",
            &[&kernel, program.to_str().unwrap(), reference.to_str().unwrap()],
        );
        // Self-host image whose startUp performs the same compile in-image.
        let in_image_out = dir.join(format!("{name}.inimage.im"));
        let driver = dir.join(format!("{name}.driver.st"));
        std::fs::write(&driver, corpus_driver(program, &in_image_out)).unwrap();
        let selfhost = dir.join(format!("{name}.selfhost.im"));
        gst_tool(
            "build_selfhost_image.st",
            &[&kernel, driver.to_str().unwrap(), selfhost.to_str().unwrap()],
        );
        let out = run_image_collect(&selfhost);
        if !String::from_utf8_lossy(&out).contains("IN-IMAGE-COMPILE-DONE") {
            failures.push(format!(
                "{name}: in-image compile failed: {:?}",
                String::from_utf8_lossy(&out)
            ));
            continue;
        }
        // THE test: byte identity.
        let a = std::fs::read(&reference).unwrap();
        let b = std::fs::read(&in_image_out).unwrap();
        if a != b {
            failures.push(format!(
                "{name}: outputs differ (gst {} bytes, in-image {} bytes)",
                a.len(),
                b.len()
            ));
            continue;
        }
        // And the in-image-compiled image must actually run correctly.
        let expected = std::fs::read(program.with_extension("expected")).unwrap();
        let run_out = run_image_collect(&in_image_out);
        if run_out != expected {
            failures.push(format!("{name}: in-image-compiled image output diverged"));
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(failures.is_empty(), "phase-5 failures:\n{}", failures.join("\n"));
}

#[test]
fn closure_the_compiler_compiles_itself_bit_identically() {
    let dir = std::env::temp_dir().join(format!("smallishtalk-p5c-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let kernel = format!("{}/st/kernel/kernel.st", root());

    let inner_prog = dir.join("inner.st");
    std::fs::write(&inner_prog, "Transcript showCr: 'third generation alive'").unwrap();

    // Reference: GST builds selfhost(startUp = inner program).
    let reference = dir.join("gen2.gst.im");
    gst_tool(
        "build_selfhost_image.st",
        &[&kernel, inner_prog.to_str().unwrap(), reference.to_str().unwrap()],
    );

    // gen1: a self-host image whose startUp rebuilds that same image
    // in-image (the compiler compiling itself).
    let gen2_in_image = dir.join("gen2.inimage.im");
    let sources = [
        "st/kernel/kernel.st",
        "st/selfhost/PlatformImage.st",
        "st/compiler/Treaty.st",
        "st/compiler/AST.st",
        "st/compiler/Lexer.st",
        "st/compiler/Parser.st",
        "st/compiler/ChunkReader.st",
        "st/compiler/CodeGen.st",
        "st/compiler/Encoder.st",
        "st/compiler/ImageWriter.st",
        "st/compiler/Compiler.st",
    ];
    let mut driver = String::from("| b |\nb := StImageBuilder new.\n");
    for f in sources {
        driver.push_str(&format!("b fileInFile: '{}/{}'.\n", root(), f));
    }
    driver.push_str(&format!(
        "b programSource: (Platform readFile: '{}').\n\
         b writeTo: '{}'.\n\
         Transcript showCr: 'COMPILER-SELF-COMPILED'\n",
        inner_prog.display(),
        gen2_in_image.display()
    ));
    let driver_path = dir.join("closure_driver.st");
    std::fs::write(&driver_path, driver).unwrap();
    let gen1 = dir.join("gen1.im");
    gst_tool(
        "build_selfhost_image.st",
        &[&kernel, driver_path.to_str().unwrap(), gen1.to_str().unwrap()],
    );

    let out = run_image_collect(&gen1);
    assert!(
        String::from_utf8_lossy(&out).contains("COMPILER-SELF-COMPILED"),
        "gen1 failed: {:?}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(
        std::fs::read(&reference).unwrap(),
        std::fs::read(&gen2_in_image).unwrap(),
        "self-compiled compiler image is not bit-identical"
    );
    // The third generation lives.
    let gen3_out = run_image_collect(&gen2_in_image);
    assert_eq!(
        String::from_utf8_lossy(&gen3_out),
        "third generation alive\n"
    );
    std::fs::remove_dir_all(&dir).ok();
}
