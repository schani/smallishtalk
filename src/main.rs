//! The VM binary: load a STIM image and run its active process (SPEC §17).

use smallishtalk::vm::{Vm, VmConfig};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: smallishtalk <image.im>");
        std::process::exit(2);
    };
    let mut vm = match Vm::load_image(path, VmConfig::default()) {
        Ok(vm) => vm,
        Err(e) => {
            eprintln!("cannot load image {path}: {e:?}");
            std::process::exit(1);
        }
    };
    // SMALLISHTALK_STATS=1 dumps the counter table on exit (via Drop);
    // SMALLISHTALK_GATE=1 additionally enables the gated hot-path tier
    // (per-opcode histogram, send counts) without image cooperation.
    if std::env::var_os("SMALLISHTALK_GATE").is_some_and(|v| v == "1") {
        vm.counters.gate = true;
    }
    let active = vm.active_process;
    match vm.run(active) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("VM error: {e:?}");
            std::process::exit(1);
        }
    }
}
