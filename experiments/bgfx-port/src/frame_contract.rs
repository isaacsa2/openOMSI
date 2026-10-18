//! Backend-independent command contract for the renderer migration.
//!
//! This is CPU-only and not yet connected to the openOMSI world/scene API.
//! In particular, submission order is the *source* order: never group away
//! ordered transparent materials just to reduce bgfx state changes.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba8Srgb,
    Rgba8Linear,
    Bc1,
    Bc3,
    Bc5,
    Depth32Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Opaque,
    AlphaTest,
    AlphaBlend,
    Additive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthMode {
    Disabled,
    TestWrite,
    TestOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Main,
    Mirror(u8),
    Navigation,
    Ui,
    Photo,
    VrEye(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshUpload {
    pub vertex_stride: u16,
    pub vertex_count: u32,
    pub index_count: u32,
    pub index_32bit: bool,
}

impl MeshUpload {
    pub fn validate(self) -> Result<(), &'static str> {
        if self.vertex_stride == 0 || self.vertex_count == 0 || self.index_count == 0 {
            return Err("mesh must contain vertices, indices and nonzero stride");
        }
        if self.index_count % 3 != 0 {
            return Err("triangle-list mesh index count must be divisible by three");
        }
        if !self.index_32bit && self.vertex_count > 65_536 {
            return Err("16-bit indexed mesh cannot address more than 65536 vertices");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub mesh: ResourceId,
    pub material: ResourceId,
    pub view: View,
    pub blend: BlendMode,
    pub depth: DepthMode,
    /// Model-to-world, column-major. World-origin rebasing must happen before
    /// this reaches the native backend to retain precision on large OMSI maps.
    pub transform: [f32; 16],
    /// Source-defined per-object order; preserve when blending, glass, text
    /// displays, terrain/transmap layers or cabin materials require it.
    pub authored_order: u64,
}

impl Draw {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.transform.iter().all(|x| x.is_finite()) {
            return Err("model matrix contains a non-finite value");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameCommands {
    pub viewport: (u32, u32),
    pub draws: Vec<Draw>,
}

impl FrameCommands {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.viewport.0 == 0 || self.viewport.1 == 0 {
            return Err("viewport must have nonzero dimensions");
        }
        for draw in &self.draws {
            draw.validate()?;
        }
        Ok(())
    }

    /// Opaque batches may eventually reorder; transparent and authored
    /// orders must be preserved. This API deliberately returns references
    /// in their original sequence until visual parity is proven.
    pub fn ordered_draws(&self) -> impl Iterator<Item = &Draw> {
        self.draws.iter()
    }
}

/// Future adapter boundary. Each native backend owns its GPU resources and
/// converts `FrameCommands` to its own passes and shaders. Nothing here
/// assumes a wgpu device, a bgfx program or a particular swapchain.
///
/// THIS TRAIT DOES NOT YET RENDER openOMSI. Adapters must implement full
/// texture, material and feature parity before production use.
pub trait NativeRenderer {
    type Error;
    fn resize(&mut self, width: u32, height: u32) -> Result<(), Self::Error>;
    fn upload_mesh(&mut self, desc: MeshUpload, vertices: &[u8], indices: &[u8])
        -> Result<ResourceId, Self::Error>;
    fn draw_frame(&mut self, frame: &FrameCommands) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(order: u64, blend: BlendMode) -> Draw {
        Draw {
            mesh: ResourceId(1),
            material: ResourceId(2),
            view: View::Main,
            blend,
            depth: DepthMode::TestOnly,
            transform: [
                1., 0., 0., 0., 0., 1., 0., 0.,
                0., 0., 1., 0., 0., 0., 0., 1.,
            ],
            authored_order: order,
        }
    }

    #[test]
    fn keep_transparent_objects_in_authored_order() {
        let frame = FrameCommands {
            viewport: (1366, 768),
            draws: vec![
                example(300, BlendMode::AlphaBlend),
                example(100, BlendMode::AlphaBlend),
            ],
        };
        frame.validate().unwrap();
        assert_eq!(
            frame.ordered_draws().map(|d| d.authored_order).collect::<Vec<_>>(),
            vec![300, 100]
        );
    }

    #[test]
    fn reject_invalid_model_matrix() {
        let mut frame = FrameCommands {
            viewport: (800, 600),
            draws: vec![example(0, BlendMode::Opaque)],
        };
        frame.draws[0].transform[4] = f32::NAN;
        assert!(frame.validate().is_err());
    }

    #[test]
    fn reject_bad_mesh_ranges() {
        assert!(MeshUpload {
            vertex_stride: 32,
            vertex_count: 70000,
            index_count: 3,
            index_32bit: false,
        }.validate().is_err());
        assert!(MeshUpload {
            vertex_stride: 32,
            vertex_count: 70000,
            index_count: 3,
            index_32bit: true,
        }.validate().is_ok());
    }
}
