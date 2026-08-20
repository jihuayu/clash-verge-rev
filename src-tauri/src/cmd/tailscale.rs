use super::CmdResult;
use crate::feat::{self, TailscaleStatus};

#[tauri::command]
pub async fn get_tailscale_status() -> CmdResult<TailscaleStatus> {
    Ok(feat::get_tailscale_status().await)
}

#[tauri::command]
pub async fn warmup_tailscale() -> CmdResult<TailscaleStatus> {
    let status = feat::get_tailscale_status().await;
    if status.enabled {
        feat::on_tailscale_config_applied().await;
    }
    Ok(status)
}
