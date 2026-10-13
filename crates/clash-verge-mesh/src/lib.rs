//! Pure, opt-in native-client coexistence planning. No commands, OS writes or credentials.
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};
use std::net::IpAddr;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MeshConfig {
    pub enabled: bool,
    /// Explicit acknowledgement of the external clients' split routes and one DNS owner.
    pub external_policy_reviewed: bool,
    /// Native TUN combinations require separate device validation.
    pub experimental_tun: bool,
    pub internet_interface: String,
    /// Exact native daemon process names reviewed on this device (required for TUN).
    pub transport_processes: Vec<String>,
    pub providers: Vec<Provider>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Tailscale,
    CloudflareMesh,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    pub kind: Kind,
    pub interface: String,
    pub cidrs: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    /// Literal private resolver IP; no system/public resolver fallback.
    pub dns_server: Option<IpAddr>,
    #[serde(default)]
    pub default_route: bool,
    /// Cloudflare One must be enrolled and in Traffic and DNS mode.
    #[serde(default)]
    pub traffic_and_dns: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: &'static str,
}

fn interface_valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.chars().all(|c| c.is_alphanumeric() || " _-.{}()#[]".contains(c))
}
fn domain_valid(domain: &str) -> bool {
    domain.len() <= 253
        && domain.contains('.')
        && domain.split('.').all(|s| {
            !s.is_empty()
                && s.len() <= 63
                && !s.starts_with('-')
                && !s.ends_with('-')
                && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
fn overlap(a: &IpNet, b: &IpNet) -> bool {
    a.contains(&b.network()) || b.contains(&a.network())
}
fn suffix_overlap(a: &str, b: &str) -> bool {
    a == b || a.ends_with(&format!(".{b}")) || b.ends_with(&format!(".{a}"))
}
fn flag(map: &Mapping, key: &str) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(false)
}

impl MeshConfig {
    pub fn active(&self) -> bool {
        self.enabled
    }

    pub fn validate(&self, base: &Mapping, platform: Platform) -> Result<Vec<Diagnostic>, String> {
        if !self.enabled {
            return Ok(vec![]);
        }
        if platform == Platform::Other {
            return Err("Mesh coexistence targets Windows and macOS only".into());
        }
        if base.get("mode").and_then(Value::as_str).unwrap_or("rule") != "rule" {
            return Err("Mesh coexistence requires Rule mode; Global/Direct bypass protected rules".into());
        }
        if !self.external_policy_reviewed {
            return Err(
                "Review native client split routes, transport exclusions and a single OS DNS owner first".into(),
            );
        }
        if !interface_valid(&self.internet_interface) {
            return Err("Specify the reviewed Internet egress interface".into());
        }
        if self.providers.len() != 1 || self.providers[0].kind != Kind::Tailscale {
            return Err("This release supports one native Tailscale provider; Cloudflare Mesh is deferred".into());
        }
        let tun = base.get("tun").and_then(Value::as_mapping).cloned().unwrap_or_default();
        if self.transport_processes.iter().any(|name| {
            name.is_empty()
                || name.len() > 128
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || " _-.".contains(c))
        }) {
            return Err("Invalid native transport process name".into());
        }
        if flag(&tun, "enable") {
            if self.transport_processes.is_empty() {
                return Err(
                    "TUN requires reviewed native Tailscale transport process names to avoid recursive proxying".into(),
                );
            }
            if !self.experimental_tun {
                return Err("TUN coexistence requires explicit experimental_tun opt-in and device validation".into());
            }
            if platform == Platform::Windows && flag(&tun, "strict-route") {
                return Err(
                    "Windows strict-route can block native-client DNS; review it explicitly before enabling Mesh TUN"
                        .into(),
                );
            }
            if platform == Platform::Macos
                && tun
                    .get("device")
                    .and_then(Value::as_str)
                    .is_some_and(|v| !v.starts_with("utun"))
            {
                return Err("macOS TUN device must have an utun prefix".into());
            }
        }
        let dns = base.get("dns").and_then(Value::as_mapping).cloned().unwrap_or_default();
        if dns
            .get("fake-ip-filter-mode")
            .and_then(Value::as_str)
            .is_some_and(|v| v != "blacklist")
        {
            return Err("Mesh domain protection requires fake-ip-filter-mode blacklist".into());
        }
        // Existing custom DNS selection can win over suffix policy or perform public resolution.
        if self.providers.iter().any(|p| !p.domains.is_empty()) {
            for key in ["nameserver-policy", "proxy-server-nameserver-policy"] {
                if dns.get(key).and_then(Value::as_mapping).is_some_and(|m| !m.is_empty()) {
                    return Err("Review/remove existing DNS policies before using Mesh domain policies".into());
                }
            }
            if base.contains_key("hosts")
                || base
                    .get("sniffer")
                    .and_then(Value::as_mapping)
                    .is_some_and(|m| flag(m, "enable"))
            {
                return Err("Mesh domain policies currently require no custom hosts or enabled sniffer".into());
            }
        }
        let mut owned: Vec<(usize, IpNet)> = vec![];
        let mut domains: Vec<(usize, String)> = vec![];
        for (i, p) in self.providers.iter().enumerate() {
            if !interface_valid(&p.interface) || p.interface == self.internet_interface {
                return Err("Each provider needs a separate reviewed native interface".into());
            }
            if self.providers[..i]
                .iter()
                .any(|q| q.kind == p.kind || q.interface == p.interface)
            {
                return Err("Duplicate provider or shared native interface".into());
            }
            if p.default_route {
                return Err(
                    "Exit nodes and native-client default routes are not supported by this split-route plan".into(),
                );
            }
            if p.kind == Kind::CloudflareMesh && !p.traffic_and_dns {
                return Err("Cloudflare Mesh requires enrolled One Client in Traffic and DNS mode".into());
            }
            if p.cidrs.is_empty() {
                return Err("Explicit Mesh host/subnet CIDRs are required; no ranges are inferred".into());
            }
            for cidr in &p.cidrs {
                let n: IpNet = cidr.parse().map_err(|_| "Invalid Mesh CIDR")?;
                // Never silently exclude default/private/CGNAT aggregates.
                if n.prefix_len() == 0
                    || [
                        "100.64.0.0/10",
                        "10.0.0.0/8",
                        "172.16.0.0/12",
                        "192.168.0.0/16",
                        "fc00::/7",
                    ]
                    .contains(&n.trunc().to_string().as_str())
                {
                    return Err(
                        "Use actual assigned hosts or routed subnets, not an entire default/private/CGNAT aggregate"
                            .into(),
                    );
                }
                if n.network().is_loopback() || n.network().is_unspecified() || n.network().is_multicast() {
                    return Err("Invalid Mesh destination network".into());
                }
                if owned.iter().any(|(owner, other)| *owner != i && overlap(&n, other)) {
                    return Err("Mesh providers overlap: exclusions cannot resolve destination ownership".into());
                }
                owned.push((i, n));
            }
            if !p.domains.is_empty() && p.dns_server.is_none() {
                return Err("Every Mesh domain requires an explicit private DNS server".into());
            }
            if let Some(ip) = p.dns_server
                && !owned.iter().any(|(owner, n)| *owner == i && n.contains(&ip))
            {
                return Err("DNS server must be included in its provider's protected CIDRs".into());
            }
            for d in &p.domains {
                let d = d.to_ascii_lowercase();
                if !domain_valid(&d) {
                    return Err("Use a fully qualified DNS suffix without wildcards or rule syntax".into());
                }
                if domains
                    .iter()
                    .any(|(owner, other)| *owner != i && suffix_overlap(&d, other))
                {
                    return Err("Mesh DNS suffix ownership overlaps".into());
                }
                domains.push((i, d));
            }
        }
        let mut notes = vec![
            Diagnostic {
                code: "external-policy",
                message: "Native routes, transport exclusions and OS DNS ownership are operator-reviewed, not automatically verified.",
            },
            Diagnostic {
                code: "offline",
                message: "Interface binding protects proxied traffic from ordinary proxy fallback. TUN-excluded and proxy-bypassing OS traffic needs native-client firewall protection on disconnect.",
            },
            Diagnostic {
                code: "reconnect",
                message: "After reconnect/sleep, refresh diagnostics; if native interface names change, update the plan before reuse.",
            },
        ];
        if self.providers.len() == 2 {
            notes.push(Diagnostic { code: "shared-cgnat", message: "Tailscale and Cloudflare default pools overlap, including Quad100. Review actual assigned routes; configured disjoint CIDRs alone do not prove native-route isolation." });
        }
        if platform == Platform::Windows {
            notes.push(Diagnostic {
                code: "windows-inbound",
                message: "Inbound Mesh TCP services may require separately approved Windows Firewall port rules.",
            });
        }
        if !flag(base, "ipv6") {
            notes.push(Diagnostic {
                code: "ipv6-disabled",
                message: "Mihomo IPv6 is disabled; IPv6 Mesh access is not validated by this plan.",
            });
        }
        Ok(notes)
    }
}

fn map_mut<'a>(map: &'a mut Mapping, key: &str) -> Result<&'a mut Mapping, String> {
    map.entry(Value::from(key))
        .or_insert_with(|| Value::Mapping(Mapping::new()))
        .as_mapping_mut()
        .ok_or_else(|| format!("{key} must be a mapping"))
}
fn append(map: &mut Mapping, key: &str, value: Value) -> Result<(), String> {
    let seq = map
        .entry(Value::from(key))
        .or_insert_with(|| Value::Sequence(vec![]))
        .as_sequence_mut()
        .ok_or_else(|| format!("{key} must be a list"))?;
    if !seq.contains(&value) {
        seq.push(value);
    }
    Ok(())
}

/// Always compile from the original profile; disabling returns it unchanged. No OS bypass edits.
pub fn compile(base: &Mapping, mesh: &MeshConfig, platform: Platform) -> Result<Mapping, String> {
    mesh.validate(base, platform)?;
    if !mesh.enabled {
        return Ok(base.clone());
    }
    let mut out = base.clone();
    let mut rules = vec![];
    out.insert("interface-name".into(), mesh.internet_interface.clone().into());
    let tun = map_mut(&mut out, "tun")?;
    tun.insert("auto-detect-interface".into(), false.into());
    if !mesh.transport_processes.is_empty() {
        let name = "VERGE-MESH-UPLINK";
        for key in ["proxies", "proxy-groups"] {
            if out
                .get(key)
                .and_then(Value::as_sequence)
                .is_some_and(|seq| seq.iter().any(|p| p.get("name").and_then(Value::as_str) == Some(name)))
            {
                return Err("Reserved VERGE-MESH proxy name already exists".into());
            }
        }
        let mut uplink = Mapping::new();
        uplink.insert("name".into(), name.into());
        uplink.insert("type".into(), "direct".into());
        uplink.insert("interface-name".into(), mesh.internet_interface.clone().into());
        uplink.insert("udp".into(), true.into());
        append(&mut out, "proxies", uplink.into())?;
        out.insert("find-process-mode".into(), "always".into());
        for process in &mesh.transport_processes {
            let rule = Value::from(format!("PROCESS-NAME,{process},{name}"));
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
    }
    for (i, p) in mesh.providers.iter().enumerate() {
        let name = format!("VERGE-MESH-{i}");
        for key in ["proxies", "proxy-groups"] {
            if out.get(key).and_then(Value::as_sequence).is_some_and(|s| {
                s.iter()
                    .any(|v| v.get("name").and_then(Value::as_str) == Some(name.as_str()))
            }) {
                return Err("Reserved VERGE-MESH proxy name already exists".into());
            }
        }
        let mut proxy = Mapping::new();
        proxy.insert("name".into(), name.clone().into());
        proxy.insert("type".into(), "direct".into());
        proxy.insert("interface-name".into(), p.interface.clone().into());
        proxy.insert("udp".into(), true.into());
        append(&mut out, "proxies", proxy.into())?;
        for cidr in &p.cidrs {
            let n: IpNet = cidr.parse().map_err(|_| "Invalid Mesh CIDR")?;
            let n = n.trunc();
            let family = if matches!(n, IpNet::V4(_)) {
                "IP-CIDR"
            } else {
                "IP-CIDR6"
            };
            let rule = Value::from(format!("{family},{n},{name},no-resolve"));
            if !rules.contains(&rule) {
                rules.push(rule);
            }
            append(map_mut(&mut out, "tun")?, "route-exclude-address", n.to_string().into())?;
        }
        for domain in &p.domains {
            let domain = domain.to_ascii_lowercase();
            let rule = Value::from(format!("DOMAIN-SUFFIX,{domain},{name}"));
            if !rules.contains(&rule) {
                rules.push(rule);
            }
            let dns = map_mut(&mut out, "dns")?;
            dns.insert("enable".into(), true.into());
            dns.insert("direct-nameserver-follow-policy".into(), true.into());
            // Explicit proxy selector binds DNS to the same native interface, without system fallback.
            let ip = p.dns_server.ok_or("Missing private resolver")?;
            let resolver = match ip {
                IpAddr::V4(_) => format!("udp://{ip}:53#{name}"),
                IpAddr::V6(_) => format!("udp://[{ip}]:53#{name}"),
            };
            map_mut(dns, "nameserver-policy")?
                .insert(format!("+.{domain}").into(), Value::Sequence(vec![resolver.into()]));
            append(dns, "fake-ip-filter", format!("+.{domain}").into())?;
        }
    }
    if let Some(old) = out.get("rules") {
        rules.extend(old.as_sequence().ok_or("rules must be a list")?.clone());
    }
    out.insert("rules".into(), rules.into());
    Ok(out)
}

/// Read-only interface presence is evidence of installation/attachment, not connectivity.
pub fn interface_diagnostics(mesh: &MeshConfig, names: &[String]) -> Vec<Diagnostic> {
    if !mesh.enabled {
        return vec![];
    }
    let missing = std::iter::once(&mesh.internet_interface)
        .chain(mesh.providers.iter().map(|p| &p.interface))
        .any(|name| !names.contains(name));
    vec![if missing {
        Diagnostic {
            code: "interface-missing",
            message: "A configured interface is absent. Provider is offline, renamed or unavailable; revalidate before applying.",
        }
    } else {
        Diagnostic {
            code: "connectivity-unverified",
            message: "Configured interfaces are present. Enrollment, routing, DNS ownership and TCP reachability remain unverified.",
        }
    }]
}

#[cfg(test)]
mod tests;

/// Redacted status: never exposes peers, addresses, user profiles or raw command output.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TailscaleStatus {
    pub state: &'static str,
    pub exit_node_selected: bool,
}

pub fn parse_tailscale_status(output: &[u8]) -> TailscaleStatus {
    let unknown = TailscaleStatus {
        state: "unknown",
        exit_node_selected: false,
    };
    #[derive(Deserialize)]
    struct StatusFields {
        #[serde(rename = "BackendState")]
        state: Option<String>,
        #[serde(rename = "ExitNodeStatus")]
        exit_node: Option<serde::de::IgnoredAny>,
    }
    let Ok(value) = serde_json::from_slice::<StatusFields>(output) else {
        return unknown;
    };
    let state = match value.state.as_deref() {
        Some("Running") => "running",
        Some("Stopped") => "stopped",
        Some("NeedsLogin") => "needs-login",
        Some("Starting") => "starting",
        Some("NeedsMachineAuth") => "needs-machine-auth",
        _ => "unknown",
    };
    TailscaleStatus {
        state,
        exit_node_selected: value.exit_node.is_some(),
    }
}
