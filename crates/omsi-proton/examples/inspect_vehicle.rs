//! Inspect a source vehicle package without converting it or altering it.
fn main() -> anyhow::Result<()> {
    let file = std::env::args_os().nth(1).ok_or_else(|| anyhow::anyhow!("usage: inspect_vehicle <vehicle.bus.txt|vehicle.ini>"))?;
    let report = omsi_proton::vehicle::inspect(std::path::Path::new(&file))?;
    println!("{}",serde_json::to_string_pretty(&report)?);
    Ok(())
}
