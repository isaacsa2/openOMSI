//! Convert a fresh local extraction without loading the generated map into the renderer.
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let source = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: import_map <source-folder> <output-package-folder> [name]"))?;
    let output = args.next().ok_or_else(|| anyhow::anyhow!("missing output package folder"))?;
    let name = args.next().unwrap_or_else(|| "LocalTest".into());
    let report = omsi_proton::package::convert_folder(
        std::path::Path::new(&source),
        std::path::Path::new(&output),
        &name.to_string_lossy(),
    )?;
    println!("{report:?}");
    Ok(())
}
