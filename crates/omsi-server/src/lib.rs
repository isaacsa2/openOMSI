//! Dedicated openOMSI host services.
//!
//! This crate is deliberately **below** the graphical application: no wgpu,
//! renderer, scene graph, UI or audio dependency is permitted here.
//! The transport, administration and console can be reused by a true headless
//! runtime while the application-specific world adapters are being extracted.

pub mod console;
pub mod commands;
pub mod fleet;
pub mod weather;
pub mod world;
pub mod admin;
