//! The VM binary: load a STIM image and run its active process (SPEC §17).

use smallishtalk::vm::{Vm, VmConfig};

fn usage() -> ! {
    eprintln!(
        "usage: smallishtalk [options] <image.im>\n\
         \n\
         UI options (UI.md §4A):\n\
         \x20 --ui              open a real window (requires the `ui` build feature)\n\
         \x20 --virtual-clock   use the deterministic virtual clock (reproducible runs)\n\
         \x20 --ui-stats        print the VM counter table (incl. UI work metrics) on exit\n\
         \n\
         Note: headless-first driving, --scenario and --shots require the in-image\n\
         UIDriver, which lands with the event loop in a later milestone."
    );
    std::process::exit(2);
}

fn main() {
    let mut image: Option<String> = None;
    let mut windowed = false;
    let mut virtual_clock = false;
    let mut ui_stats = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--ui" => windowed = true,
            "--virtual-clock" => virtual_clock = true,
            "--ui-stats" => ui_stats = true,
            "-h" | "--help" => usage(),
            s if s.starts_with("--") => {
                eprintln!("unknown option: {s}");
                usage();
            }
            s => {
                if image.replace(s.to_string()).is_some() {
                    eprintln!("multiple image paths given");
                    usage();
                }
            }
        }
    }

    let Some(path) = image else { usage() };

    let mut vm = match Vm::load_image(&path, VmConfig::default()) {
        Ok(vm) => vm,
        Err(e) => {
            eprintln!("cannot load image {path}: {e:?}");
            std::process::exit(1);
        }
    };

    if virtual_clock {
        vm.host.use_virtual_clock();
    }
    if windowed {
        #[cfg(feature = "ui")]
        {
            vm.host.windowed = true;
        }
        #[cfg(not(feature = "ui"))]
        eprintln!("note: --ui ignored — rebuild with `--features ui` for a window");
    }

    // SMALLISHTALK_STATS=1 dumps the counter table on exit (via Drop);
    // SMALLISHTALK_GATE=1 additionally enables the gated hot-path tier
    // (per-opcode histogram, send counts) without image cooperation.
    if std::env::var_os("SMALLISHTALK_GATE").is_some_and(|v| v == "1") {
        vm.counters.gate = true;
    }

    let active = vm.active_process;
    let result = vm.run(active);
    if ui_stats {
        eprint!("{}", vm.format_stats());
    }
    if let Err(e) = result {
        eprintln!("VM error: {e:?}");
        std::process::exit(1);
    }
}
