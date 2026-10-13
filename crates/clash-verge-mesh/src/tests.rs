use super::*;
fn base(tun: bool) -> Mapping {
    serde_yaml_ng::from_str(&format!("mode: rule\nipv6: true\nrules: [MATCH, 'MATCH,ordinary-proxy']\ntun:\n  enable: {tun}\n  route-exclude-address: [192.0.2.1/32]\ndns:\n  nameserver: [https://1.1.1.1/dns-query]\n  fake-ip-filter: [localhost]\n")).unwrap()
}
fn settings() -> MeshConfig {
    MeshConfig {
        enabled: true,
        external_policy_reviewed: true,
        experimental_tun: true,
        internet_interface: "en0".into(),
        transport_processes: vec!["tailscaled".into()],
        providers: vec![Provider {
            kind: Kind::Tailscale,
            interface: "utun8".into(),
            cidrs: vec![
                "100.101.2.3/32".into(),
                "100.100.100.100/32".into(),
                "fd7a:115c:a1e0::123/128".into(),
                "10.24.8.0/24".into(),
            ],
            domains: vec!["example.ts.net".into()],
            dns_server: Some("100.100.100.100".parse().unwrap()),
            default_route: false,
            traffic_and_dns: false,
        }],
    }
}
#[test]
fn both_platforms_system_proxy_and_tun_protect_only_explicit_routes() {
    for os in [Platform::Windows, Platform::Macos] {
        for tun in [true, false] {
            let out = compile(&base(tun), &settings(), os).unwrap();
            let rules = out["rules"].as_sequence().unwrap();
            assert_eq!(
                rules[1].as_str(),
                Some("IP-CIDR,100.101.2.3/32,VERGE-MESH-0,no-resolve")
            );
            assert!(
                rules
                    .iter()
                    .any(|v| v.as_str() == Some("IP-CIDR6,fd7a:115c:a1e0::123/128,VERGE-MESH-0,no-resolve"))
            );
            assert_eq!(rules.last().unwrap().as_str(), Some("MATCH,ordinary-proxy"));
            assert_eq!(out["tun"]["route-exclude-address"].as_sequence().unwrap().len(), 5);
            assert_eq!(out["proxies"][1]["interface-name"].as_str(), Some("utun8"));
            assert_eq!(
                out["dns"]["nameserver-policy"]["+.example.ts.net"][0].as_str(),
                Some("udp://100.100.100.100:53#VERGE-MESH-0")
            );
            assert_eq!(out["dns"]["nameserver"], base(tun)["dns"]["nameserver"]);
            assert_eq!(out["dns"]["fake-ip-filter"].as_sequence().unwrap().len(), 2);
        }
    }
}
#[test]
fn no_config_and_disable_restore_exact_base_without_accumulation() {
    let original = base(true);
    assert_eq!(
        compile(&original, &MeshConfig::default(), Platform::Other).unwrap(),
        original
    );
    let on = compile(&original, &settings(), Platform::Macos).unwrap();
    assert_ne!(on, original);
    assert_eq!(compile(&original, &settings(), Platform::Macos).unwrap(), on);
    let mut off = settings();
    off.enabled = false;
    assert_eq!(compile(&original, &off, Platform::Macos).unwrap(), original);
}
#[test]
fn global_direct_exit_node_and_unreviewed_configs_are_blocked() {
    for os in [Platform::Windows, Platform::Macos] {
        for mode in ["global", "direct"] {
            let mut b = base(false);
            b.insert("mode".into(), mode.into());
            assert!(compile(&b, &settings(), os).is_err());
        }
        let mut s = settings();
        s.providers[0].default_route = true;
        assert!(compile(&base(false), &s, os).is_err());
        s = settings();
        s.external_policy_reviewed = false;
        assert!(compile(&base(false), &s, os).is_err());
        s = settings();
        s.experimental_tun = false;
        assert!(compile(&base(true), &s, os).is_err());
        assert!(compile(&base(false), &s, os).is_ok());
    }
}
#[test]
fn unsafe_addresses_dns_and_rule_injection_are_rejected() {
    for cidr in [
        "100.64.0.0/10",
        "0.0.0.0/0",
        "::/0",
        "10.0.0.0/8",
        "garbage",
        "127.0.0.1/32",
    ] {
        let mut s = settings();
        s.providers[0].cidrs.push(cidr.into());
        assert!(compile(&base(false), &s, Platform::Macos).is_err(), "{cidr}");
    }
    let mut s = settings();
    s.providers[0].domains = vec!["example.ts.net,PROXY".into()];
    assert!(compile(&base(false), &s, Platform::Macos).is_err());
    s = settings();
    s.providers[0].dns_server = Some("8.8.8.8".parse().unwrap());
    assert!(compile(&base(false), &s, Platform::Macos).is_err());
}
#[test]
fn conflicting_dns_and_reserved_proxy_fail_without_mutating_input() {
    for extra in [
        "dns: {fake-ip-filter-mode: whitelist}",
        "dns: {nameserver-policy: {'+.ts.net': 8.8.8.8}}",
        "hosts: {}",
        "sniffer: {enable: true}",
        "proxies: [{name: VERGE-MESH-0, type: direct}]",
    ] {
        let mut b = base(false);
        b.extend(serde_yaml_ng::from_str::<Mapping>(extra).unwrap());
        let original = b.clone();
        assert!(compile(&b, &settings(), Platform::Windows).is_err(), "{extra}");
        assert_eq!(b, original);
    }
}
#[test]
fn platform_conflicts_and_deferred_cloudflare_are_explicit() {
    let mut b = base(true);
    b["tun"]["strict-route"] = true.into();
    assert!(compile(&b, &settings(), Platform::Windows).is_err());
    assert!(compile(&b, &settings(), Platform::Macos).is_ok());
    b["tun"]["device"] = "clash".into();
    assert!(compile(&b, &settings(), Platform::Macos).is_err());
    let mut s = settings();
    s.providers[0].kind = Kind::CloudflareMesh;
    assert!(
        compile(&base(false), &s, Platform::Macos)
            .unwrap_err()
            .contains("deferred")
    );
}
#[test]
fn offline_reconnect_presence_is_never_claimed_as_connectivity() {
    let s = settings();
    assert_eq!(interface_diagnostics(&s, &[])[0].code, "interface-missing");
    assert_eq!(
        interface_diagnostics(&s, &["en0".into(), "utun8".into()])[0].code,
        "connectivity-unverified"
    );
    assert_eq!(
        interface_diagnostics(&s, &["en0".into(), "utun9".into()])[0].code,
        "interface-missing"
    );
    // Stable desired config on disconnect/reconnect; no removal of protection on status loss.
    assert_eq!(
        compile(&base(true), &s, Platform::Macos).unwrap(),
        compile(&base(true), &s, Platform::Macos).unwrap()
    );
}
#[test]
fn canonical_routes_merge_and_deduplicate() {
    let mut s = settings();
    s.providers[0]
        .cidrs
        .extend(["10.24.8.1/24".into(), "100.101.2.3/32".into()]);
    let out = compile(&base(true), &s, Platform::Macos).unwrap();
    assert_eq!(out["tun"]["route-exclude-address"].as_sequence().unwrap().len(), 5);
}

#[test]
fn transport_processes_precede_routes_and_bind_physical_uplink() {
    let mut s = settings();
    for os in [Platform::Windows, Platform::Macos] {
        let out = compile(&base(true), &s, os).unwrap();
        assert_eq!(
            out["rules"][0].as_str(),
            Some("PROCESS-NAME,tailscaled,VERGE-MESH-UPLINK")
        );
        assert_eq!(out["proxies"][0]["interface-name"].as_str(), Some("en0"));
        assert_eq!(out["find-process-mode"].as_str(), Some("always"));
    }
    s.transport_processes.clear();
    assert!(compile(&base(true), &s, Platform::Macos).is_err());
    assert!(compile(&base(false), &s, Platform::Macos).is_ok());
}

#[test]
fn status_is_redacted_and_handles_offline_unknown_and_exit_nodes() {
    let running = parse_tailscale_status(
        br#"{"BackendState":"Running","Self":{"HostName":"PRIVATE"},"Peer":{"key":"PRIVATE"},"ExitNodeStatus":null}"#,
    );
    assert_eq!(running.state, "running");
    assert!(!serde_json::to_string(&running).unwrap().contains("PRIVATE"));
    assert_eq!(parse_tailscale_status(b"not json").state, "unknown");
    assert_eq!(
        parse_tailscale_status(br#"{"BackendState":"Stopped"}"#).state,
        "stopped"
    );
    assert!(
        parse_tailscale_status(br#"{"BackendState":"Running","ExitNodeStatus":{"ID":"PRIVATE"}}"#).exit_node_selected
    );
}

#[test]
fn localized_windows_interface_and_ipv6_dns_are_supported() {
    let mut s = settings();
    s.internet_interface = "以太网 2".into();
    s.providers[0].dns_server = Some("fd7a:115c:a1e0::123".parse().unwrap());
    let out = compile(&base(false), &s, Platform::Windows).unwrap();
    assert_eq!(out["interface-name"].as_str(), Some("以太网 2"));
    assert_eq!(
        out["dns"]["nameserver-policy"]["+.example.ts.net"][0].as_str(),
        Some("udp://[fd7a:115c:a1e0::123]:53#VERGE-MESH-0")
    );
}
