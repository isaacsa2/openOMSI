//! Dedicated openOMSI host services.
//!
//! This crate is deliberately **below** the graphical application: no wgpu,
//! renderer, scene graph, UI or audio dependency is permitted here.
//! The transport, administration and console can be reused by a true headless
//! runtime while the application-specific world adapters are being extracted.

pub mod admin;
pub mod clock;
pub mod commands;
pub mod console;
pub mod transport;
pub mod lanes;
pub mod fleet;
pub mod metar;
pub mod startup;
pub mod weather;
pub mod world;
