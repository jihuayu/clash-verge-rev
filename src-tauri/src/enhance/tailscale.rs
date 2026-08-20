use serde_yaml_ng::{Mapping, Sequence, Value};

pub const PROXY_NAME: &str = "Tailscale";

const TAILNET_V4: &str = "100.64.0.0/10";
const TAILNET_V6: &str = "fd7a:115c:a1e0::/48";
const MAGIC_DNS_DOMAIN: &str = "ts.net";
const MAGIC_DNS_FILTER: &str = "+.ts.net";
const MAGIC_DNS_POLICY: &str = "+.ts.net";

pub struct TailscaleEnhanceOpts<'a> {
    pub enable: bool,
    pub state_dir: &'a str,
    pub auth_key: Option<&'a str>,
    pub control_url: Option<&'a str>,
}

pub fn use_tailscale(config: Mapping, opts: &TailscaleEnhanceOpts<'_>) -> Mapping {
    if !opts.enable {
        return config;
    }
    inject(config, opts)
}

fn inject(mut config: Mapping, opts: &TailscaleEnhanceOpts<'_>) -> Mapping {
    let proxy_name = inject_proxy(&mut config, opts);
    prepend_rules(&mut config, &proxy_name);
    inject_dns(&mut config, &proxy_name);
    config
}

fn inject_proxy(config: &mut Mapping, opts: &TailscaleEnhanceOpts<'_>) -> String {
    let proxies_key = Value::from("proxies");
    let mut proxies = sequence_of(config.get(&proxies_key));

    if let Some(existing) = existing_proxy(&proxies, PROXY_NAME) {
        return existing;
    }

    let mut proxy = Mapping::new();
    proxy.insert(Value::from("name"), Value::from(PROXY_NAME));
    proxy.insert(Value::from("type"), Value::from("tailscale"));
    proxy.insert(Value::from("udp"), Value::from(true));
    if !opts.state_dir.is_empty() {
        proxy.insert(Value::from("state-dir"), Value::from(opts.state_dir));
    }
    if let Some(auth_key) = nonempty(opts.auth_key) {
        proxy.insert(Value::from("auth-key"), Value::from(auth_key));
    }
    if let Some(control_url) = nonempty(opts.control_url) {
        proxy.insert(Value::from("control-url"), Value::from(control_url));
    }

    proxies.push(Value::Mapping(proxy));
    config.insert(proxies_key, Value::Sequence(proxies));
    PROXY_NAME.to_owned()
}

fn existing_proxy(proxies: &Sequence, name: &str) -> Option<String> {
    for proxy in proxies {
        let Some(map) = proxy.as_mapping() else {
            continue;
        };
        let Some(existing_name) = map.get(Value::from("name")).and_then(Value::as_str) else {
            continue;
        };
        if existing_name == name {
            return Some(existing_name.to_owned());
        }
    }
    None
}

fn prepend_rules(config: &mut Mapping, proxy_name: &str) {
    let rules_key = Value::from("rules");
    let mut rules = sequence_of(config.get(&rules_key));
    for rule in injected_rules(proxy_name).into_iter().rev() {
        if !contains_string(&rules, &rule) {
            rules.insert(0, Value::from(rule));
        }
    }
    config.insert(rules_key, Value::Sequence(rules));
}

fn injected_rules(proxy_name: &str) -> [String; 3] {
    [
        format!("IP-CIDR,{TAILNET_V4},{proxy_name},no-resolve"),
        format!("IP-CIDR6,{TAILNET_V6},{proxy_name},no-resolve"),
        format!("DOMAIN-SUFFIX,{MAGIC_DNS_DOMAIN},{proxy_name}"),
    ]
}

fn inject_dns(config: &mut Mapping, proxy_name: &str) {
    let dns_key = Value::from("dns");
    let Some(Value::Mapping(mut dns)) = config.remove(&dns_key) else {
        return;
    };
    if dns.get(Value::from("enable")).and_then(Value::as_bool) != Some(true) {
        config.insert(dns_key, Value::Mapping(dns));
        return;
    }

    inject_nameserver_policy(&mut dns, proxy_name);
    inject_fake_ip_filter(&mut dns);
    config.insert(dns_key, Value::Mapping(dns));
}

fn inject_nameserver_policy(dns: &mut Mapping, proxy_name: &str) {
    let policy_key = Value::from("nameserver-policy");
    let resolver = format!("ts://{proxy_name}");
    match dns.get(&policy_key) {
        Some(Value::Sequence(seq)) => {
            if nameserver_policy_covers(seq, MAGIC_DNS_POLICY) {
                return;
            }
            let mut seq = seq.clone();
            seq.push(Value::from(format!("{MAGIC_DNS_POLICY}={resolver}")));
            dns.insert(policy_key, Value::Sequence(seq));
        }
        Some(Value::Mapping(map)) => {
            let mut map = map.clone();
            map.insert(Value::from(MAGIC_DNS_POLICY), Value::from(resolver));
            dns.insert(policy_key, Value::Mapping(map));
        }
        _ => {
            let mut map = Mapping::new();
            map.insert(Value::from(MAGIC_DNS_POLICY), Value::from(resolver));
            dns.insert(policy_key, Value::Mapping(map));
        }
    }
}

fn nameserver_policy_covers(seq: &Sequence, domain: &str) -> bool {
    let prefix = format!("{domain}=");
    seq.iter().any(|item| match item {
        Value::String(entry) => entry == domain || entry.starts_with(&prefix),
        Value::Mapping(map) => map.contains_key(Value::from(domain)),
        _ => false,
    })
}

fn inject_fake_ip_filter(dns: &mut Mapping) {
    // Whitelist mode means "only fake these domains". Adding `+.ts.net` there would
    // replace MagicDNS answers with fake-ips instead of keeping the 100.x addresses.
    if dns.get(Value::from("fake-ip-filter-mode")).and_then(Value::as_str) == Some("whitelist") {
        return;
    }

    let filter_key = Value::from("fake-ip-filter");
    let mut filter = sequence_of(dns.get(&filter_key));
    if !contains_string(&filter, MAGIC_DNS_FILTER) {
        filter.push(Value::from(MAGIC_DNS_FILTER));
        dns.insert(filter_key, Value::Sequence(filter));
    }
}

fn sequence_of(value: Option<&Value>) -> Sequence {
    value.and_then(Value::as_sequence).cloned().unwrap_or_default()
}

fn contains_string(seq: &Sequence, expected: &str) -> bool {
    seq.iter().any(|item| item.as_str() == Some(expected))
}

const fn nonempty(value: Option<&str>) -> Option<&str> {
    match value {
        Some(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, reason = "tests assert by panicking")]
mod tests {
    use super::{Mapping, TailscaleEnhanceOpts, Value, use_tailscale};

    fn mapping(pairs: &[(&str, Value)]) -> Mapping {
        let mut map = Mapping::new();
        for (key, value) in pairs {
            map.insert(Value::from(*key), value.clone());
        }
        map
    }

    fn enabled_opts<'a>(auth_key: Option<&'a str>, control_url: Option<&'a str>) -> TailscaleEnhanceOpts<'a> {
        TailscaleEnhanceOpts {
            enable: true,
            state_dir: "/tmp/verge/tailscale",
            auth_key,
            control_url,
        }
    }

    fn sequence_strings(config: &Mapping, key: &str) -> Vec<String> {
        config
            .get(Value::from(key))
            .and_then(Value::as_sequence)
            .expect("sequence field")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }

    fn proxy_named<'a>(config: &'a Mapping, name: &str) -> &'a Mapping {
        config
            .get(Value::from("proxies"))
            .and_then(Value::as_sequence)
            .expect("proxies")
            .iter()
            .filter_map(Value::as_mapping)
            .find(|proxy| proxy.get(Value::from("name")).and_then(Value::as_str) == Some(name))
            .expect("named proxy")
    }

    #[test]
    fn disabled_leaves_the_config_alone() {
        let config = mapping(&[("proxies", Value::Sequence(vec![]))]);
        let result = use_tailscale(
            config.clone(),
            &TailscaleEnhanceOpts {
                enable: false,
                state_dir: "/tmp/verge/tailscale",
                auth_key: None,
                control_url: None,
            },
        );
        assert_eq!(result, config);
    }

    #[test]
    fn enabling_injects_the_outbound_and_top_rules() {
        let result = use_tailscale(Mapping::new(), &enabled_opts(None, None));
        let proxy = proxy_named(&result, "Tailscale");
        assert_eq!(proxy.get(Value::from("type")), Some(&Value::from("tailscale")));
        assert_eq!(proxy.get(Value::from("udp")), Some(&Value::from(true)));
        assert_eq!(
            proxy.get(Value::from("state-dir")),
            Some(&Value::from("/tmp/verge/tailscale"))
        );
        assert!(!proxy.contains_key(Value::from("auth-key")));

        let rules = sequence_strings(&result, "rules");
        assert_eq!(
            rules,
            vec![
                "IP-CIDR,100.64.0.0/10,Tailscale,no-resolve".to_owned(),
                "IP-CIDR6,fd7a:115c:a1e0::/48,Tailscale,no-resolve".to_owned(),
                "DOMAIN-SUFFIX,ts.net,Tailscale".to_owned(),
            ]
        );
    }

    #[test]
    fn auth_key_and_control_url_are_copied_when_present() {
        let result = use_tailscale(
            Mapping::new(),
            &enabled_opts(Some("tskey-auth-test"), Some("https://headscale.example")),
        );
        let proxy = proxy_named(&result, "Tailscale");
        assert_eq!(
            proxy.get(Value::from("auth-key")),
            Some(&Value::from("tskey-auth-test"))
        );
        assert_eq!(
            proxy.get(Value::from("control-url")),
            Some(&Value::from("https://headscale.example"))
        );
    }

    #[test]
    fn an_existing_same_name_proxy_is_left_in_place() {
        let existing = mapping(&[("name", Value::from("Tailscale")), ("type", Value::from("socks5"))]);
        let config = mapping(&[("proxies", Value::Sequence(vec![Value::Mapping(existing)]))]);
        let result = use_tailscale(config, &enabled_opts(None, None));
        let proxies = result
            .get(Value::from("proxies"))
            .and_then(Value::as_sequence)
            .expect("proxies");
        assert_eq!(proxies.len(), 1);
        assert_eq!(
            proxy_named(&result, "Tailscale").get(Value::from("type")),
            Some(&Value::from("socks5"))
        );
        let rules = sequence_strings(&result, "rules");
        assert!(
            rules
                .iter()
                .any(|rule| rule.ends_with(",Tailscale,no-resolve") || rule.ends_with(",Tailscale"))
        );
    }

    #[test]
    fn dns_policy_is_only_written_when_dns_is_enabled() {
        let disabled = mapping(&[("dns", Value::Mapping(mapping(&[("enable", Value::from(false))])))]);
        let result = use_tailscale(disabled, &enabled_opts(None, None));
        let dns = result.get(Value::from("dns")).and_then(Value::as_mapping).expect("dns");
        assert!(!dns.contains_key(Value::from("nameserver-policy")));

        let enabled = mapping(&[(
            "dns",
            Value::Mapping(mapping(&[
                ("enable", Value::from(true)),
                ("fake-ip-filter", Value::Sequence(vec![Value::from("+.lan")])),
            ])),
        )]);
        let result = use_tailscale(enabled, &enabled_opts(None, None));
        let dns = result.get(Value::from("dns")).and_then(Value::as_mapping).expect("dns");
        let policy = dns
            .get(Value::from("nameserver-policy"))
            .and_then(Value::as_mapping)
            .expect("nameserver-policy");
        assert_eq!(
            policy.get(Value::from("+.ts.net")),
            Some(&Value::from("ts://Tailscale"))
        );
        let filter = dns
            .get(Value::from("fake-ip-filter"))
            .and_then(Value::as_sequence)
            .expect("fake-ip-filter");
        assert_eq!(filter, &vec![Value::from("+.lan"), Value::from("+.ts.net")]);
    }

    #[test]
    fn existing_nameserver_policy_entries_are_kept() {
        let enabled = mapping(&[(
            "dns",
            Value::Mapping(mapping(&[
                ("enable", Value::from(true)),
                (
                    "nameserver-policy",
                    Value::Mapping(mapping(&[("+.corp.local", Value::from("10.0.0.1"))])),
                ),
            ])),
        )]);
        let result = use_tailscale(enabled, &enabled_opts(None, None));
        let policy = result
            .get(Value::from("dns"))
            .and_then(Value::as_mapping)
            .and_then(|dns| dns.get(Value::from("nameserver-policy")))
            .and_then(Value::as_mapping)
            .expect("nameserver-policy");
        assert_eq!(policy.get(Value::from("+.corp.local")), Some(&Value::from("10.0.0.1")));
        assert_eq!(
            policy.get(Value::from("+.ts.net")),
            Some(&Value::from("ts://Tailscale"))
        );
    }

    #[test]
    fn sequence_nameserver_policy_is_appended_not_replaced() {
        let enabled = mapping(&[(
            "dns",
            Value::Mapping(mapping(&[
                ("enable", Value::from(true)),
                (
                    "nameserver-policy",
                    Value::Sequence(vec![Value::from("+.corp.local=10.0.0.1")]),
                ),
            ])),
        )]);
        let result = use_tailscale(enabled, &enabled_opts(None, None));
        let policy = result
            .get(Value::from("dns"))
            .and_then(Value::as_mapping)
            .and_then(|dns| dns.get(Value::from("nameserver-policy")))
            .and_then(Value::as_sequence)
            .expect("nameserver-policy sequence");
        assert_eq!(
            policy,
            &vec![
                Value::from("+.corp.local=10.0.0.1"),
                Value::from("+.ts.net=ts://Tailscale"),
            ]
        );
    }

    #[test]
    fn fake_ip_whitelist_is_not_given_magic_dns() {
        let enabled = mapping(&[(
            "dns",
            Value::Mapping(mapping(&[
                ("enable", Value::from(true)),
                ("fake-ip-filter-mode", Value::from("whitelist")),
                ("fake-ip-filter", Value::Sequence(vec![Value::from("+.example.com")])),
            ])),
        )]);
        let result = use_tailscale(enabled, &enabled_opts(None, None));
        let dns = result.get(Value::from("dns")).and_then(Value::as_mapping).expect("dns");
        let filter = dns
            .get(Value::from("fake-ip-filter"))
            .and_then(Value::as_sequence)
            .expect("fake-ip-filter");
        assert_eq!(filter, &vec![Value::from("+.example.com")]);
    }

    #[test]
    fn injected_rules_are_not_duplicated() {
        let config = mapping(&[(
            "rules",
            Value::Sequence(vec![
                Value::from("IP-CIDR,100.64.0.0/10,Tailscale,no-resolve"),
                Value::from("MATCH,PROXY"),
            ]),
        )]);
        let result = use_tailscale(config, &enabled_opts(None, None));
        let rules = sequence_strings(&result, "rules");
        let matches = rules
            .iter()
            .filter(|rule| *rule == "IP-CIDR,100.64.0.0/10,Tailscale,no-resolve")
            .count();
        assert_eq!(matches, 1);
        assert_eq!(rules.last().map(String::as_str), Some("MATCH,PROXY"));
    }
}
