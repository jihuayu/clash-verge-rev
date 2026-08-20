use crate::{
    config::Config,
    core::{handle, manager::CoreManager},
    enhance::tailscale::PROXY_NAME,
    process::AsyncHandler,
};
use clash_verge_logging::{Type, logging};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde::Serialize;
use std::time::Duration;

const LOGIN_WATCH_ATTEMPTS: usize = 30;
const LOGIN_WATCH_INTERVAL: Duration = Duration::from_secs(2);

static LAST_EMITTED_LOGIN_URL: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static LOGIN_URL_RE: Lazy<Regex> = Lazy::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r#"https://login\.tailscale\.com/[^\s"'<>]+"#).expect("login URL regex")
});
static GENERIC_URL_RE: Lazy<Regex> = Lazy::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r#"https://[^\s"'<>]+"#).expect("generic URL regex")
});

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStatus {
    pub enabled: bool,
    pub available: bool,
    pub alive: bool,
    pub delay: Option<u32>,
    pub login_url: Option<String>,
    pub needs_login: bool,
}

impl TailscaleStatus {
    const fn disabled() -> Self {
        Self {
            enabled: false,
            available: false,
            alive: false,
            delay: None,
            login_url: None,
            needs_login: false,
        }
    }
}

pub async fn get_tailscale_status() -> TailscaleStatus {
    let enabled = Config::verge().await.latest_arc().enable_tailscale.unwrap_or(false);
    if !enabled {
        LAST_EMITTED_LOGIN_URL.lock().take();
        return TailscaleStatus::disabled();
    }

    let login_url = login_url_from_logs().await;
    let (available, alive, delay) = match handle::Handle::mihomo().get_proxies().await {
        Ok(proxies) => proxies.proxies.get(PROXY_NAME).map_or((false, false, None), |proxy| {
            let delay = proxy.history.last().map(|item| u32::from(item.delay));
            (true, proxy.alive, delay)
        }),
        Err(_) => (false, false, None),
    };

    TailscaleStatus {
        enabled: true,
        available,
        alive,
        delay,
        needs_login: login_url.is_some() && !alive,
        login_url,
    }
}

pub async fn on_tailscale_config_applied() {
    let enabled = Config::verge().await.latest_arc().enable_tailscale.unwrap_or(false);
    if !enabled {
        LAST_EMITTED_LOGIN_URL.lock().take();
        return;
    }
    AsyncHandler::spawn(|| async {
        let _ = handle::Handle::mihomo()
            .delay_proxy_by_name(PROXY_NAME, "https://www.gstatic.com/generate_204", 10_000)
            .await;
        watch_login_url().await;
    });
}

async fn watch_login_url() {
    for _ in 0..LOGIN_WATCH_ATTEMPTS {
        if let Some(url) = login_url_from_logs().await {
            emit_login_url(url);
            return;
        }
        tokio::time::sleep(LOGIN_WATCH_INTERVAL).await;
    }
}

fn emit_login_url(url: String) {
    let mut last = LAST_EMITTED_LOGIN_URL.lock();
    if last.as_deref() == Some(url.as_str()) {
        return;
    }
    *last = Some(url.clone());
    drop(last);
    logging!(info, Type::Core, "Tailscale login URL captured");
    handle::Handle::notify_tailscale_login_url(&url);
}

async fn login_url_from_logs() -> Option<String> {
    let logs = CoreManager::global().get_clash_logs().await.ok()?;
    extract_login_url(logs.iter())
}

fn first_url_in(line: &str, re: &Regex) -> Option<String> {
    re.find(line)
        .map(|m| m.as_str().trim_end_matches(['.', ',', ';']).to_owned())
}

pub fn extract_login_url<I, S>(logs: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut found = None;
    for line in logs {
        let line = line.as_ref();
        if let Some(url) = first_url_in(line, &LOGIN_URL_RE) {
            found = Some(url);
            continue;
        }
        if is_auth_hint_line(line)
            && let Some(url) = first_url_in(line, &GENERIC_URL_RE)
        {
            found = Some(url);
        }
    }
    found
}

fn is_auth_hint_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("tailscale")
        && (lower.contains("auth")
            || lower.contains("login")
            || lower.contains("authenticate")
            || lower.contains("visit"))
}

#[cfg(test)]
mod tests {
    use super::extract_login_url;

    #[test]
    fn official_login_urls_are_taken_from_the_latest_matching_line() {
        let logs = [
            r#"time="2026-08-20T00:00:00Z" level=info msg="[Tailscale](Tailscale) backend: authentication required; visit https://login.tailscale.com/a/old""#,
            r"[Tailscale](Tailscale) To authenticate, visit https://login.tailscale.com/a/new,",
        ];
        assert_eq!(
            extract_login_url(logs),
            Some("https://login.tailscale.com/a/new".to_owned())
        );
    }

    #[test]
    fn headscale_urls_are_accepted_on_auth_hint_lines() {
        let logs = ["[Tailscale](Tailscale) AuthURL is https://headscale.example/a/xyz"];
        assert_eq!(
            extract_login_url(logs),
            Some("https://headscale.example/a/xyz".to_owned())
        );
    }

    #[test]
    fn unrelated_https_links_are_ignored() {
        let logs = ["GET https://example.com/health"];
        assert_eq!(extract_login_url(logs), None);
    }
}
