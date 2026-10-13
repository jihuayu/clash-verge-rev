use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

pub const EMBEDDED_PROXY: &str = "VERGE-TAILSCALE";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default, deny_unknown_fields)]
pub struct EmbeddedTailscale {
    pub enabled: bool,
    pub hostname: String,
    pub domains: Vec<String>,
    pub cidrs: Vec<String>,
}

impl Default for EmbeddedTailscale {
    fn default() -> Self {
        Self {
            enabled: false,
            hostname: "clash-verge".into(),
            domains: vec!["ts.net".into()],
            cidrs: vec!["100.64.0.0/10".into(), "fd7a:115c:a1e0::/48".into()],
        }
    }
}

pub fn compile_embedded(base: &Mapping, config: &EmbeddedTailscale) -> Result<Mapping, String> {
    if !config.enabled {
        return Ok(base.clone());
    }
    if base.get("mode").and_then(Value::as_str).unwrap_or("rule") != "rule" {
        return Err("Tailscale requires Rule mode".into());
    }
    if config.hostname.is_empty()
        || config.hostname.len() > 63
        || config.hostname.starts_with('-')
        || config.hostname.ends_with('-')
        || !config.hostname.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Device name must contain 1–63 letters, numbers or hyphens".into());
    }
    let mut out = base.clone();
    for key in ["proxies", "proxy-groups"] {
        if out.get(key).and_then(Value::as_sequence).is_some_and(|items| {
            items
                .iter()
                .any(|v| v.get("name").and_then(Value::as_str) == Some(EMBEDDED_PROXY))
        }) {
            return Err("Reserved Tailscale proxy name already exists".into());
        }
    }
    let mut proxy = Mapping::new();
    proxy.insert("name".into(), EMBEDDED_PROXY.into());
    proxy.insert("type".into(), "tailscale".into());
    proxy.insert("hostname".into(), config.hostname.clone().into());
    proxy.insert("state-dir".into(), "tailscale/verge".into());
    proxy.insert("udp".into(), true.into());
    proxy.insert("accept-routes".into(), false.into());
    super::append(&mut out, "proxies", proxy.into())?;
    let mut rules = vec![];
    for cidr in &config.cidrs {
        let net: ipnet::IpNet = cidr.parse().map_err(|_| "Invalid Tailscale CIDR")?;
        if net.prefix_len() == 0
            || net.network().is_loopback()
            || net.network().is_unspecified()
            || net.network().is_multicast()
        {
            return Err("Default, loopback and multicast routes cannot be Tailscale destinations".into());
        }
        // Embedded traffic must enter Mihomo, never escape via native TUN exclusions.
        if let Some(exclusions) = out
            .get("tun")
            .and_then(|v| v.get("route-exclude-address"))
            .and_then(Value::as_sequence)
        {
            for exclusion in exclusions {
                if let Some(n) = exclusion.as_str().and_then(|s| s.parse::<ipnet::IpNet>().ok())
                    && super::overlap(&net, &n)
                {
                    return Err("A TUN exclusion overlaps an embedded Tailscale destination".into());
                }
            }
        }
        let family = if matches!(net, ipnet::IpNet::V4(_)) {
            "IP-CIDR"
        } else {
            "IP-CIDR6"
        };
        rules.push(Value::from(format!(
            "{family},{},{EMBEDDED_PROXY},no-resolve",
            net.trunc()
        )));
    }
    for domain in &config.domains {
        let domain = domain.to_ascii_lowercase();
        if !super::domain_valid(&domain) {
            return Err("Invalid MagicDNS suffix".into());
        }
        let dns = super::map_mut(&mut out, "dns")?;
        if dns
            .get("fake-ip-filter-mode")
            .and_then(Value::as_str)
            .is_some_and(|v| v != "blacklist")
        {
            return Err("Tailscale DNS requires fake-ip-filter-mode blacklist".into());
        }
        dns.insert("enable".into(), true.into());
        dns.insert("direct-nameserver-follow-policy".into(), true.into());
        let policy = super::map_mut(dns, "nameserver-policy")?;
        let key = format!("+.{domain}");
        if policy.contains_key(&key) {
            return Err("An existing DNS policy conflicts with Tailscale".into());
        }
        policy.insert(key.clone().into(), Value::from(format!("tailscale://{EMBEDDED_PROXY}")));
        super::append(dns, "fake-ip-filter", key.into())?;
        rules.push(Value::from(format!("DOMAIN-SUFFIX,{domain},{EMBEDDED_PROXY}")));
    }
    if let Some(existing) = out.get("rules") {
        rules.extend(existing.as_sequence().ok_or("rules must be a list")?.clone());
    }
    out.insert("rules".into(), rules.into());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_routes_stay_inside_core_and_disable_restores_profile() {
        let base: Mapping =
            serde_yaml_ng::from_str("mode: rule\ntun: {enable: true}\nrules: ['MATCH,DIRECT']").unwrap();
        let config = EmbeddedTailscale {
            enabled: true,
            ..Default::default()
        };
        let out = compile_embedded(&base, &config).unwrap();
        assert_eq!(out["proxies"][0]["type"], "tailscale");
        assert!(out["tun"].get("route-exclude-address").is_none());
        assert_eq!(
            out["dns"]["nameserver-policy"]["+.ts.net"],
            "tailscale://VERGE-TAILSCALE"
        );
        assert_eq!(out["rules"].as_sequence().unwrap().last().unwrap(), "MATCH,DIRECT");
        assert_eq!(compile_embedded(&base, &EmbeddedTailscale::default()).unwrap(), base);
    }
    #[test]
    fn native_route_exclusions_and_reserved_names_are_rejected() {
        let config = EmbeddedTailscale {
            enabled: true,
            ..Default::default()
        };
        for text in [
            "tun: {route-exclude-address: ['100.64.0.0/10']}",
            "proxies: [{name: VERGE-TAILSCALE}]",
        ] {
            assert!(compile_embedded(&serde_yaml_ng::from_str(text).unwrap(), &config).is_err());
        }
    }
}
