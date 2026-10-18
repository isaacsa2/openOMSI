//! Use exactly the backend-independent scene command contract exported by
//! the production openOMSI renderer. No experimental fork of these types:
//! future wgpu and BGFX backends must consume the same CPU frame packets.

pub use omsi_geometry::gpu_contract::*;
