# Tailscale coexistence (review implementation)

Scope was narrowed to **Tailscale first** on 2026-10-13. Cloudflare Mesh is deferred and rejected by the compiler. This is an opt-in implementation for review, not a certification of simultaneous VPN operation. Nothing was installed, enrolled, launched as a proxy, or applied to the developer machine's networking during development.

## Architecture and behavior

- `mesh_coexistence` in `verge.yaml` defaults to absent/disabled. One `tailscale` native provider is supported. Existing profiles are unchanged without opt-in.
- `clash-verge-mesh` is a pure configuration compiler: validates first, clones the base, adds only explicit routes, domains and resolvers, and leaves the original intact. No default CGNAT/private pool exclusions or peer discovery are performed.
- The final enhancement stage runs **after** merge/scripts and authoritative app settings. Protected rules precede subscription rules. Mesh destinations use a named `type: direct` adapter bound to the configured Tailscale interface. The Internet uplink is explicitly bound to the reviewed physical interface.
- Native transport process names, when configured, precede destination rules and use a separate physical-interface direct adapter. They are required for experimental TUN. Process identification and actual daemon names must be validated on each device; the code does not guess a macOS Network Extension name.
- Each private suffix receives `nameserver-policy` pointing to its explicitly configured literal resolver through the bound adapter, plus a `fake-ip-filter` entry and `direct-nameserver-follow-policy`. Resolver IPs must be part of that provider's protected CIDRs. No `system` or public resolver fallback is inserted. Use full MagicDNS names; short-name expansion, private reverse DNS/PTR, application-owned DoH and DNS outside Mihomo are not covered automatically.
- Explicit IPv4/IPv6 CIDRs merge into `tun.route-exclude-address`, with deduplication. Linux-only exclusion sets are not used. These exclusions take effect only when TUN is enabled.
- macOS coexistence suppresses this fork's pre-existing public-DNS rewrite in `use_tun`. It does not install a different OS DNS configuration. With coexistence disabled, normal upstream behavior remains.
- App mode changes reject Global and Direct while enabled; they would bypass protected routing rules. External-controller clients can still change the core directly: do not use them to switch mode while coexistence is active.
- The preview reads interface names and runs only `tailscale status --json --peers=false` from standard install paths. It returns an allowlisted state and an exit-node boolean. It never exposes raw CLI output, identities, node keys, peer lists, login URLs or addresses. CLI absence, timeout (3 seconds per candidate), errors and unknown JSON yield `unknown`, not `offline` or `connected`. No native CLI was invoked against the developer's actual Tailscale session during tests.
- Exit-node selection is rejected in preview and at configuration generation when detected. A manually declared `default_route` is always rejected. There is no background watcher and no automatic editing of routes or interfaces. Reconnect/sleep requires refreshed diagnostics; renamed/reused interfaces require operator review.
- Disabling regenerates from the unchanged original profile, removing generated rules, DNS policies, adapters and exclusions. It never removes entries from the source profile. Apply uses the existing checked core update / Verge draft transaction.

## Supported configuration boundaries

| Combination | This implementation | Device verification |
| --- | --- | --- |
| Windows/macOS, system proxy + Rule | Primary path; protected explicit IPs/FQDNs use native Tailscale; remaining traffic follows profile | Not run |
| Windows/macOS, TUN + Rule | Experimental opt-in; explicit transport processes and routes required | Not run |
| System proxy + TUN together | Same compiler protections, same experimental prerequisites | Not run |
| Global/Direct while enabled | Rejected by app APIs / configuration generation | Logic tested |
| Exit node / VPN default route | Rejected when declared or detected; selection after apply is not monitored | Not supported |
| DNS whitelist/rule fake-IP filter, pre-existing DNS policy, custom hosts or enabled sniffer with protected domains | Rejected pending explicit conflict resolution | Logic tested |
| Windows TUN `strict-route: true` | Rejected because it can block the other client's DNS | Logic tested |
| IPv6 | CIDR6, IPv6 route exclusions and resolver address syntax; no silent global toggle | Static/logic tested only |
| Cloudflare Mesh / three clients | Deferred; no Cloudflare enrollment or settings code | Not supported |

Interface-bound adapters do not fall through to the subscription's ordinary proxy rule when dialing fails. **This is not an OS kill switch.** Programs bypassing the system proxy and packets excluded from TUN follow the OS routing table. When Tailscale disconnects, those routes may disappear. Preventing all such traffic from escaping requires a separately reviewed native firewall/routing policy; no such policy is installed here. Interface reuse and native DNS/routing changes can also invalidate the reviewed plan. Do not label the implementation “leak proof” or “automatically recovers every reconnect.”

The compiler validates the user's declared routes, not ownership of live routes. It does not infer subnet acceptance from peer-advertised routes. Existing OS proxy bypass/PAC configuration is left alone; applications bypassing the proxy rely on native routing. A full DNS-owner/default-route audit remains an acceptance prerequisite.

## Review and usage

1. Build in an isolated checkout. Do not launch a second Clash instance alongside the user's active setup.
2. Before real activation, obtain approval for any required change to the active proxy/TUN/native-client settings, and record current settings plus rollback. Prefer starting with Clash TUN off; if the old build already rewrote macOS DNS, restore/verify it explicitly before enabling this feature.
3. In Settings, open **Tailscale coexistence → Configure / Preview**. The advanced JSON editor starts disabled and empty.
4. Enter the actual physical Internet interface and native Tailscale interface, selected tailnet hosts (`/32`, `/128`), accepted subnet routes, the actual tailnet DNS suffix, and the resolver IP. Quad100 `100.100.100.100/32` is an explicit optional choice, never added automatically. Confirm one OS DNS owner and transport split routes; then set `external_policy_reviewed: true` and `enabled: true`.
5. Preview shows generated controls plus read-only status. It intentionally excludes subscription contents/credentials. Apply revalidates the complete enhanced profile, so a preview can succeed while Apply reports a profile-level conflict. Review errors rather than removing security settings blindly.
6. Keep `experimental_tun: false` for the initial system-proxy evaluation. Enabling TUN later additionally requires real daemon names in `transport_processes`, validated process matching and an approved disconnect policy. Windows strict-route is not silently relaxed.
7. To roll back, set `enabled: false`, Preview, Apply. If app access is unavailable, stop the test instance before removing `mesh_coexistence` from its isolated `verge.yaml`; regenerate/restart only under the agreed device-test procedure. Disabling returns ordinary upstream DNS behavior, which must be included in the real-device rollback review.

All files in `fixtures/` contain synthetic examples. `en0`, `utun8`, `Wi-Fi`, `Tailscale`, `tailscaled`, addresses and suffixes are not discovered settings and must not be copied into a real configuration without review.

Offline generation (no OS/network access):

```sh
cargo test -p clash-verge-mesh
cargo run -p clash-verge-mesh --example preview -- \
  docs/mesh/fixtures/base.yaml docs/mesh/fixtures/tailscale.yaml macos
```

`macos` / `windows` select policy checks; selecting Windows on a Mac is **not** Windows execution. Core static validation uses `mihomo -t -d <isolated-dir> -f <generated-file>` only, never starts TUN.

## Upstream/dependency impact

No Mihomo, Tailscale, sysproxy, service or other upstream source patch is required for the current feature. It uses existing Mihomo direct adapters, interface binding, process rules, DNS selectors and TUN exclusions. The new Rust crate uses already present serde, serde_json, serde_yaml_ng and ipnet versions. Existing pinned git dependencies are unchanged. A separate `coexistence-test` build isolates app data/IPC, blocks service/TUN/DNS mutations, and supports initial system-proxy acceptance. See [acceptance plan](ACCEPTANCE-PLAN.zh-CN.md), [safety review](SECURITY-REVIEW.md), and [build results/signing limits](BUILD-RESULTS.zh-CN.md). Windows runtime tests remain separate work.

## Evidence and sources

Consulted official documentation/source on 2026-10-13:

- [Tailscale VPN coexistence limitations](https://tailscale.com/docs/reference/faq/other-vpns)
- [Tailscale status flags and version-sensitive JSON](https://github.com/tailscale/tailscale/blob/main/cmd/tailscale/cli/status.go)
- [Mihomo TUN options and platform restrictions](https://wiki.metacubex.one/en/config/inbound/tun/)
- [Mihomo interface-bound direct adapter](https://wiki.metacubex.one/en/config/proxies/direct/)
- [Mihomo DNS policy and proxy/interface selector](https://wiki.metacubex.one/en/config/dns/)
- [Mihomo adapter and dial error handling](https://github.com/MetaCubeX/mihomo/blob/Meta/adapter/outbound/direct.go)

Cloudflare research retained only to explain the deferral: [Mesh](https://developers.cloudflare.com/mesh/) uses One Client (formerly WARP Connector/peer-to-peer), distinct from Tunnel. [Client access](https://developers.cloudflare.com/mesh/guides/connect-client-devices/) requires Traffic and DNS and correct split routes. The default `100.96.0.0/12` is inside Tailscale's CGNAT pool and includes Quad100. This is a conflict risk requiring examination of actual ownership, not proof that all combinations fail. No unverified three-client workaround is implemented.
