//! Smoke-test CLI for the isolated bgfx experiment; not the openOMSI executable.
use openomsi_bgfx_port::{run, Backend, Trial};

fn usage() -> &'static str {
    "Usage: cargo run --manifest-path experiments/bgfx-port/Cargo.toml -- \\n  [--backend auto|dx11|dx12|vulkan|gl|metal] [--size WIDTHxHEIGHT] [--frames N]\\n\\nNote: tests headless bgfx initialization and empty-frame CPU submission only.\\nIt does NOT measure openOMSI rendering performance."
}

fn parse_args() -> Result<Trial, String> {
    let mut trial = Trial { backend: Backend::Auto, width: 640, height: 360, frames: 120 };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Err(usage().to_owned()),
            "--backend" => {
                let name = args.next().ok_or("missing --backend value")?;
                trial.backend = Backend::parse(&name).ok_or_else(|| format!("unsupported backend: {name}"))?;
            }
            "--frames" => {
                let value = args.next().ok_or("missing --frames value")?;
                trial.frames = value.parse().map_err(|_| format!("invalid frame count: {value}"))?;
            }
            "--size" => {
                let size = args.next().ok_or("missing --size value")?;
                let (w, h) = size.split_once('x').ok_or("size must be WIDTHxHEIGHT")?;
                trial.width = w.parse().map_err(|_| "invalid width")?;
                trial.height = h.parse().map_err(|_| "invalid height")?;
            }
            _ => return Err(format!("unrecognized option: {arg}\\n{}", usage())),
        }
    }
    trial.validate().map_err(str::to_owned)
}

fn main() {
    let trial = match parse_args() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    match run(trial) {
        Ok(result) => {
            println!(
                "bgfx smoke: requested={:?} selected={} frames={} cpu_submission_ms_per_frame={:.4}",
                result.requested,
                result.native_renderer,
                result.frames,
                result.cpu_submission_ms_per_frame()
            );
            println!("NOT a game FPS benchmark: no swapchain present, meshes, materials, shaders, UI or simulation");
        }
        Err(e) => {
            eprintln!("bgfx smoke failed: {e}");
            std::process::exit(1);
        }
    }
}
