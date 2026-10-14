//! What the scene's draws are made of on the GPU: meshes, textures, materials, and their
//! slots let go of and taken over again.

mod free;
mod materials;
mod meshes;
mod script_upload;
mod textures;
mod uniform_pages;

pub(crate) use free::*;
pub use materials::*;
pub use meshes::*;
pub use script_upload::ScriptWrite;
pub use textures::*;
pub(crate) use uniform_pages::{stride as uniform_stride, UniformPages, UniformSlot};
