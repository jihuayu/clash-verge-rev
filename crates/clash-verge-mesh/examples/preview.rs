//! Offline compiler: cargo run -p clash-verge-mesh --example preview -- BASE.yaml PLAN.yaml
use clash_verge_mesh::{MeshConfig, Platform, compile};
use serde_yaml_ng::Mapping;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: preview BASE.yaml PLAN.yaml windows|macos".into());
    }
    let base: Mapping = serde_yaml_ng::from_str(&std::fs::read_to_string(&args[1])?)?;
    let mesh: MeshConfig = serde_yaml_ng::from_str(&std::fs::read_to_string(&args[2])?)?;
    let platform = match args[3].as_str() {
        "windows" => Platform::Windows,
        "macos" => Platform::Macos,
        _ => return Err("unsupported platform".into()),
    };
    let output = compile(&base, &mesh, platform)?;
    print!("{}", serde_yaml_ng::to_string(&output)?);
    Ok(())
}
