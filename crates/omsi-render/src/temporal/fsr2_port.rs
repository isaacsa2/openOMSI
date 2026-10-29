//! FSR 2 WGSL backend adapted from stevepryde/sp-fidelity-wgpu 0.2.0 (MIT).
//! Source: https://github.com/stevepryde/sp-fidelity
//! Changes: target patched openOMSI wgpu 29; omit upstream GPU test/readback.
//! The shaders and CPU SDK port are provided by sp-fidelity 0.2.0, not by FSR1.
//! This backend is opt-in and never compiled for normal game installations.
mod dispatch_grid;
include!("fsr2_port/part1.rs");
include!("fsr2_port/part2.rs");
include!("fsr2_port/part3.rs");
