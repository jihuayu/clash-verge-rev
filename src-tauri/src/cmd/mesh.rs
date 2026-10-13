use super::{CmdResult, CommandFailure};
use clash_verge_mesh::{Diagnostic, MeshConfig, Platform};
use network_interface::{NetworkInterface, NetworkInterfaceConfig as _};
use serde::Serialize;

#[derive(Default, serde::Deserialize, Serialize, specta::Type)]
pub struct EmbeddedStatus {
    pub state: String,
    #[serde(default)]
    pub auth_url: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub dns_name: Option<String>,
    #[serde(default)]
    pub ips: Vec<String>,
    #[serde(default)]
    pub tailnet: Option<String>,
}

async fn embedded_request(method: reqwest::Method) -> CmdResult<EmbeddedStatus> {
    let request = crate::core::handle::Handle::mihomo()
        .load_ctx()
        .build_request(method.clone(), "/proxies/VERGE-TAILSCALE/tailscale")
        .map_err(CommandFailure::plain)?;
    let response = request
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| CommandFailure::plain("无法连接内核，请确认内核正在运行"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CommandFailure::coded(
            "TAILSCALE_CORE_REQUIRED",
            "当前内核不支持登录管理，请使用带 Tailscale 登录接口的 Mihomo 内核",
        ));
    }
    if !response.status().is_success() {
        return Err(CommandFailure::plain("Tailscale 操作失败，请稍后重试"));
    }
    if method == reqwest::Method::DELETE {
        return Ok(EmbeddedStatus {
            state: "NeedsLogin".into(),
            ..Default::default()
        });
    }
    if method == reqwest::Method::PATCH {
        return Ok(EmbeddedStatus {
            state: "Stopped".into(),
            ..Default::default()
        });
    }
    response
        .json()
        .await
        .map_err(|_| CommandFailure::plain("内核返回了无法识别的 Tailscale 状态"))
}

#[tauri::command]
#[specta::specta]
pub async fn get_embedded_tailscale_status() -> CmdResult<EmbeddedStatus> {
    if !crate::config::Config::verge()
        .await
        .latest_arc()
        .embedded_tailscale
        .as_ref()
        .is_some_and(|m| m.enabled)
    {
        return Ok(EmbeddedStatus {
            state: "Disabled".into(),
            ..Default::default()
        });
    }
    embedded_request(reqwest::Method::GET).await
}

#[tauri::command]
#[specta::specta]
pub async fn login_embedded_tailscale() -> CmdResult<EmbeddedStatus> {
    embedded_request(reqwest::Method::POST).await
}

#[tauri::command]
#[specta::specta]
pub async fn logout_embedded_tailscale() -> CmdResult<EmbeddedStatus> {
    embedded_request(reqwest::Method::DELETE).await
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect_embedded_tailscale() -> CmdResult<EmbeddedStatus> {
    embedded_request(reqwest::Method::PATCH).await
}

#[tauri::command]
#[specta::specta]
pub async fn open_embedded_tailscale_login() -> CmdResult<()> {
    let state = embedded_request(reqwest::Method::GET).await?;
    let url = state
        .auth_url
        .ok_or_else(|| CommandFailure::plain("授权链接尚未就绪，请稍后重试"))?;
    let parsed = reqwest::Url::parse(&url).map_err(CommandFailure::plain)?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("login.tailscale.com")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
    {
        return Err(CommandFailure::plain("内核返回了非官方 Tailscale 授权链接"));
    }
    open::that(url).map_err(CommandFailure::plain)
}

pub const fn mesh_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else {
        Platform::Other
    }
}

#[derive(Serialize, specta::Type)]
pub struct MeshPreview {
    pub yaml: String,
    pub diagnostics: Vec<Diagnostic>,
    pub tailscale: clash_verge_mesh::TailscaleStatus,
}

/// Pure preview plus read-only interface presence; never enrolls or changes client/OS state.
#[tauri::command]
#[specta::specta]
pub async fn preview_mesh_coexistence(base: crate::ipc::YamlMapping, mesh: MeshConfig) -> CmdResult<MeshPreview> {
    let base = base.0;
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
        if let Ok(Ok(output)) = tokio::time::timeout(std::time::Duration::from_secs(3), command.output()).await
            && output.status.success()
        {
            return clash_verge_mesh::parse_tailscale_status(&output.stdout);
        }
    }
    clash_verge_mesh::parse_tailscale_status(&[])
}
