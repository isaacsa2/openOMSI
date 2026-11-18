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
pub mod fleet;
pub mod interest;
pub mod lanes;
pub mod metar;
pub mod simulation;
pub mod startup;
pub mod stops;
pub mod tick;
pub mod traffic;
pub mod transport;
pub mod weather;
pub mod world;
