use super::{CmdResult, CommandFailure};
use clash_verge_mesh::{Diagnostic, MeshConfig, Platform};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use serde::Serialize;
use serde_yaml_ng::Mapping;

pub const fn mesh_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else {
        Platform::Other
    }
}

#[derive(Serialize)]
pub struct MeshPreview {
    pub yaml: String,
    pub diagnostics: Vec<Diagnostic>,
    pub tailscale: clash_verge_mesh::TailscaleStatus,
}

/// Pure preview plus read-only interface presence; never enrolls or changes client/OS state.
#[tauri::command]
pub async fn preview_mesh_coexistence(base: Mapping, mesh: MeshConfig) -> CmdResult<MeshPreview> {
    let mut diagnostics = mesh.validate(&base, mesh_platform()).map_err(CommandFailure::plain)?;
    match NetworkInterface::show() {
        Ok(interfaces) => diagnostics.extend(clash_verge_mesh::interface_diagnostics(
            &mesh,
            &interfaces.into_iter().map(|i| i.name).collect::<Vec<_>>(),
        )),
        Err(_) => diagnostics.push(Diagnostic {
            code: "interface-unavailable",
            message: "Interface inventory unavailable; provider connectivity is unknown.",
        }),
    }
    let config = clash_verge_mesh::compile(&base, &mesh, mesh_platform()).map_err(CommandFailure::plain)?;
    let yaml = serde_yaml_ng::to_string(&config).map_err(CommandFailure::plain)?;
    let tailscale = if mesh.enabled {
        read_tailscale_status().await
    } else {
        clash_verge_mesh::parse_tailscale_status(&[])
    };
    if tailscale.exit_node_selected {
        return Err(CommandFailure::plain(
            "A Tailscale exit node is selected; disable it separately before applying coexistence",
        ));
    }
    Ok(MeshPreview {
        yaml,
        diagnostics,
        tailscale,
    })
}

/// Fixed, read-only CLI invocation. Raw stdout/stderr never enters logs or IPC responses.
pub async fn read_tailscale_status() -> clash_verge_mesh::TailscaleStatus {
    #[cfg(target_os = "macos")]
    let candidates = vec![
        std::path::PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale"),
        std::path::PathBuf::from("/usr/local/bin/tailscale"),
        std::path::PathBuf::from("/opt/homebrew/bin/tailscale"),
    ];
    #[cfg(target_os = "windows")]
    let candidates = std::env::var_os("ProgramFiles")
        .map(|root| vec![std::path::PathBuf::from(root).join("Tailscale/tailscale.exe")])
        .unwrap_or_default();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let candidates: Vec<std::path::PathBuf> = vec![];
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let mut command = tokio::process::Command::new(path);
        command
            .args(["status", "--json", "--peers=false"])
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null());
        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        if let Ok(Ok(output)) = tokio::time::timeout(std::time::Duration::from_secs(3), command.output()).await {
            if output.status.success() {
                return clash_verge_mesh::parse_tailscale_status(&output.stdout);
            }
        }
    }
    clash_verge_mesh::parse_tailscale_status(&[])
}
