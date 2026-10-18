//! CPU-only parser and staging validator for BGFX geometry.
//!
//! Reads either a legally owned OMSI O3D part or a synthetic fixture.
//! This does not initialize a GPU and makes no claim about rendering quality.
use std::path::PathBuf;

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("Usage: bgfx-o3d-check PATH.o3d");
        std::process::exit(2);
    };
    let file = PathBuf::from(path);
    match openomsi_bgfx_port::o3d_bridge::load_file_with_textures(&file) {
        Ok((mesh, pictures)) => {
            println!("BGFX_O3D_STAGING_OK vertices={} indices={} runs={} material_files={}",
                mesh.vertex_count, mesh.index_count, mesh.runs.len(),
                pictures.iter().filter(|p| p.is_some()).count());
        }
        Err(error) => {
            eprintln!("O3D parsing/staging failed: {error}");
            std::process::exit(1);
        }
    }
}
