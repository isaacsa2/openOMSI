//! Convert a declared Proton vehicle into an OpenOMSI-only Mods package.
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2,
        "usage: import_vehicle <original .bus.txt or .ini> <new output folder>"
    );
    let report = omsi_proton::vehicle_package::convert(
        std::path::Path::new(&args[0]),
        std::path::Path::new(&args[1]),
        || false,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
