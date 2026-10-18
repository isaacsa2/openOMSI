//! Backend-independent GPU command contract shared by openOMSI's CPU scene and native renderers.
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
    /// O3D objects are usually one-sided; generated terrain can be two-sided.
    pub one_sided: bool,
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
    /// First triangle-index and count within the native mesh, or None for
    /// the whole mesh. Stock OMSI materials split one model into many runs.
    pub range: Option<(u32, u32)>,
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
        if let Some((_, count)) = self.range {
            if count == 0 || count % 3 != 0 {
                return Err("GPU draw index range must be nonempty triangle list");
            }
        }
        Ok(())
    }
}

/// Backend-neutral camera for each view: the scene supplies GPU-ready
/// matrices after world-origin rebasing. The projection must use the target
/// BGFX backend's clip-depth convention (see bgfx caps homogeneousDepth).
#[derive(Clone, Debug, PartialEq)]
pub struct ViewCamera {
    pub view: View,
    pub world_to_camera: [f32; 16],
    pub camera_to_clip: [f32; 16],
    /// The shipping renderer uses reversed-Z; keep this explicit so BGFX
    /// selects GREATER depth testing and clears depth to 0 instead of 1.
    pub reversed_z: bool,
}

impl ViewCamera {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.world_to_camera.iter().chain(&self.camera_to_clip)
            .all(|x| x.is_finite()) {
            return Err("BGFX camera matrices must contain only finite values");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameCommands {
    pub viewport: (u32, u32),
    pub cameras: Vec<ViewCamera>,
    pub draws: Vec<Draw>,
}

impl FrameCommands {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.viewport.0 == 0 || self.viewport.1 == 0 {
            return Err("viewport must have nonzero dimensions");
        }
        for camera in &self.cameras {
            camera.validate()?;
        }
        for i in 0..self.cameras.len() {
            if self.cameras[..i].iter().any(|a| a.view == self.cameras[i].view) {
                return Err("duplicate BGFX view camera");
            }
        }
        for draw in &self.draws {
            draw.validate()?;
            if !self.cameras.iter().any(|a| a.view == draw.view) {
                return Err("draw has no view/projection camera");
            }
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

/// Metadata for one source scene texture slot. The generation changes when
/// the slot is recycled, so BGFX can invalidate the corresponding C++ handle.
/// This deliberately contains no pixels: those must come from the original
/// texture decode/upload path, not a blocking GPU readback from wgpu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureSlot {
    pub resource: ResourceId,
    pub width: u32,
    pub height: u32,
    pub generation: u64,
}

/// Native RGBA8 image upload. Other formats (BC, HDR, depth and mip chains)
/// will have distinct descriptors rather than being silently reinterpreted.
/// Resources have stable IDs within a native renderer instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureUpload {
    pub width: u32,
    pub height: u32,
}

impl TextureUpload {
    pub fn validate(self, rgba: &[u8]) -> Result<(), &'static str> {
        if self.width == 0 || self.height == 0 || self.width > u16::MAX as u32 || self.height > u16::MAX as u32 {
            return Err("invalid BGFX RGBA8 texture dimensions");
        }
        let bytes = (self.width as usize).checked_mul(self.height as usize)
            .and_then(|p| p.checked_mul(4))
            .ok_or("RGBA8 texture dimensions overflow")?;
        if bytes > 256 * 1024 * 1024 || bytes != rgba.len() {
            return Err("RGBA8 data must match image dimensions and stay within 256MiB");
        }
        Ok(())
    }
}

/// Material identity and its diffuse image. BGFX implements this subset first;
/// the full OMSI material representation (all lightmaps, masks, wetness, etc.)
/// must be added before the shipping renderer can be replaced.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialUpload {
    pub diffuse_texture: Option<ResourceId>,
    pub color: [f32; 4],
}

impl MaterialUpload {
    pub fn validate(self) -> Result<(), &'static str> {
        if self.color.iter().any(|c| !c.is_finite() || !(0.0..=1.0).contains(c)) {
            return Err("native material color must be finite and in [0, 1]");
        }
        Ok(())
    }
}

/// GPU resource ownership is separate from drawing: maps swap textures, buses
/// repaint and meshes stream independently of the view's frame cadence.
/// IDs are in separate domains for meshes, materials and textures.
pub trait NativeResourceRenderer: NativeRenderer {
    fn upload_texture_rgba8(
        &mut self, desc: TextureUpload, rgba: &[u8],
    ) -> Result<ResourceId, Self::Error>;
    /// Install or replace the image at a stable source texture ID. Scene
    /// snapshots use source slot + 1 and cannot rely on allocation sequence.
    fn upsert_texture_rgba8(
        &mut self, id: ResourceId, desc: TextureUpload, rgba: &[u8],
    ) -> Result<(), Self::Error>;
    fn upsert_material(
        &mut self, id: ResourceId, material: MaterialUpload,
    ) -> Result<(), Self::Error>;
    fn release_mesh(&mut self, id: ResourceId) -> Result<(), Self::Error>;
    fn release_texture(&mut self, id: ResourceId) -> Result<(), Self::Error>;
    fn release_material(&mut self, id: ResourceId) -> Result<(), Self::Error>;
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

    #[test]
    fn native_texture_material_contract_rejects_bad_inputs() {
        assert!(TextureUpload { width: 2, height: 2 }.validate(&[255; 16]).is_ok());
        assert!(TextureUpload { width: 2, height: 2 }.validate(&[255; 15]).is_err());
        assert!(TextureUpload { width: 0, height: 2 }.validate(&[]).is_err());
        assert!(MaterialUpload { diffuse_texture: None, color: [1.0; 4] }.validate().is_ok());
        assert!(MaterialUpload { diffuse_texture: None, color: [f32::NAN, 0., 0., 1.] }.validate().is_err());
    }

    fn example(order: u64, blend: BlendMode) -> Draw {
        Draw {
            mesh: ResourceId(1),
            material: ResourceId(2),
            view: View::Main,
            blend,
            depth: DepthMode::TestOnly,
            range: None,
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
            cameras: vec![ViewCamera { view: View::Main,
                world_to_camera: [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.],
                camera_to_clip: [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.], reversed_z: false }],
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
            cameras: vec![ViewCamera { view: View::Main,
                world_to_camera: [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.],
                camera_to_clip: [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.], reversed_z: false }],
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
            one_sided: true,
        }.validate().is_err());
        assert!(MeshUpload {
            vertex_stride: 32,
            vertex_count: 70000,
            index_count: 3,
            index_32bit: true,
            one_sided: true,
        }.validate().is_ok());
    }
}
