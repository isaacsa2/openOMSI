//! The lights a scene is given: point and spot lights (map lights, interior lights,
//! headlights), which path draws them, and the light coronas.

#[cfg(doc)]
use super::Renderer;
use glam::{DVec3, Vec3};

/// A point light in world space (`[maplight]`, `[interiorlight]`, headlights).
#[derive(Debug, Clone, Copy)]
pub struct PointLight {
    pub position: DVec3,
    pub radius: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    /// A spot light's direction (zero = a point light) and the cosines of its inner and
    /// outer cone (enhanced path).
    pub direction: Vec3,
    pub cone: [f32; 2],
    /// The radius within which the light is at full strength (`[maplight]`'s); 0 = an
    /// eighth of `radius` (enhanced path; vanilla always takes the eighth).
    pub core: f32,
    /// A headlamp (enhanced path), lit by a road lamp's profile instead of the cone: 1 a low
    /// beam, with its cut-off at the lamp's horizon, -1 a full beam, without; 0 any other light.
    pub beam: f32,
    /// A lamp in a housing - a street lamp's head, a platform's light (`[maplight]`): the
    /// enhanced path sends its light down and out, a few per cent above its horizon.
    pub housed: bool,
    /// Which path draws the light.
    pub mode: LightMode,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            position: DVec3::ZERO,
            radius: 0.0,
            color: [1.0; 3],
            intensity: 1.0,
            direction: Vec3::ZERO,
            cone: [1.0, 0.0],
            core: 0.0,
            beam: 0.0,
            housed: false,
            mode: LightMode::Both,
        }
    }
}

/// Which renderer a light belongs to: a vehicle's headlight is three point lights along
/// its axis for the vanilla path and one real spot light for the enhanced one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightMode {
    #[default]
    Both,
    Vanilla,
    Enhanced,
}

/// A light corona sprite (`[light_enh]`, `[light_enh_2]`).
#[derive(Debug, Clone, Copy)]
pub struct Corona {
    pub position: DVec3,
    pub size: f32,
    pub color: [f32; 3],
    /// 0..2 (a `[light_enh_2]` fading variable of 2 is double brightness).
    pub brightness: f32,
    /// Facing direction (zero = omnidirectional) and cosine of the visibility half-cone
    /// (the outer cone: where it begins to be seen).
    pub direction: Vec3,
    pub cone_cos: f32,
    /// Cosine of the inner cone (full brightness); below `cone_cos` = no inner cone.
    pub inner_cos: f32,
    /// `[light_enh_2]` rotating: 0 a flat sprite facing `direction`, 1 turned to the viewer
    /// about `up`, 2 turned to the viewer about every axis (a billboard).
    pub rotating: u8,
    pub up: Vec3,
    /// How far the spot is moved from its place towards the viewer (m), so that a lamp
    /// inside its housing still shows; negative = the old default (half its size, at most
    /// half a metre).
    pub z_offset: f32,
    /// `[light_enh_2]` parameter bits: 1 star, 2 no fog, 4 only effects.
    pub flags: u8,
    /// Its picture: 0 the standard glow, else one registered with
    /// [`Renderer::set_corona_texture`] (a light's own `bitmap`, the fog cone's picture).
    pub texture: u16,
    /// A light's cone in the fog rather than its glow (OMSI's `light_cone.bmp` fan, see
    /// corona.wgsl): `size` is the fan's radius, `cone_cos` and `inner_cos` hold the outer
    /// and inner half angles (radians), `beam_width` the fog's visibility (m).
    pub beam: bool,
    pub beam_width: f32,
    /// The halo round a light in fog: a billboard of `size`, pulled
    /// towards the viewer, seen from in front of the light; the angles and the visibility
    /// travel as for a cone.
    pub halo: bool,
}

impl Default for Corona {
    fn default() -> Self {
        Corona {
            position: DVec3::ZERO,
            size: 0.1,
            color: [1.0; 3],
            brightness: 0.0,
            direction: Vec3::ZERO,
            cone_cos: -1.0,
            inner_cos: -2.0,
            rotating: 2,
            up: Vec3::Z,
            z_offset: -1.0,
            flags: 0,
            texture: 0,
            beam: false,
            beam_width: 0.0,
            halo: false,
        }
    }
}
