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
    let active = vm.active_process;
    match vm.run(active) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("VM error: {e:?}");
            std::process::exit(1);
        }
    }
}
