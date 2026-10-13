# Verification record

Baseline: fork `jihuayu/clash-verge-rev`, dev `b80bca01`. Host: macOS arm64. No existing checkout was modified. No credentials or live node inventory were read. The running sim-core project was untouched; Rust compilation used two jobs.

| Check | Result / limit |
| --- | --- |
| Pure compiler tests | 11 passed: both platform policies, TUN on/off, rule routing, defaults/disable regeneration, route dedupe, bad inputs, DNS conflicts, Global/Direct/exit-node rejection, strict-route/macOS device checks, process uplink, offline/reconnect interface fixtures, redacted CLI status, localized Windows interface / IPv6 DNS |
| Native integration tests | Linked macOS test binary: 1 update-flag test + 6 TUN configuration regressions passed, all with OS DNS side effects suppressed |
| macOS backend compilation | `cargo check -p clash-verge --features clippy --jobs 2` passed; `clippy` feature skips Tauri resource packaging. No desktop process launched |
| Frontend | `tsc --noEmit` passed; Vite production static build passed; changed-file ESLint/format verified |
| Mihomo static syntax | Installed core `Meta v1.19.21 darwin arm64`, `-t` passed for synthetic system-proxy macOS, TUN macOS and TUN Windows configurations. Core never started listeners/TUN |
| Windows | Same pure policy code exercised with Windows input; no Windows compiler, OS, service, driver or firewall tests executed |
| Live traffic / DNS capture | Not run: requires separate authorization |
| Native app packaging/signing | Not produced; resource-skipping compile check is not a release build |

The initial pure-test attempt mixed rustc selection (dependency metadata from 1.96.1 versus 1.95.0). Retried successfully with explicit RUSTC 1.95.0 and isolated `target/mesh-tests-195`; no source workaround was added. Initial offline full-app checks lacked cached dependencies; downloaded approved build dependencies and retried. No dependency versions were intentionally upgraded.

## Pending device acceptance (both Windows and macOS)

Run only after approval of a concrete isolated test setup and rollback:

1. Record versions, approved interface/daemon names, exact host/subnet routes, DNS owner and proxy/PAC settings. Verify no exit node/default-route takeover and no address/interface ownership conflict. Existing broad bypasses are outside compiler ownership.
2. Tailscale-only baseline: TCP to a designated tailnet host and accepted subnet service by IPv4, IPv6 and full MagicDNS name. Do not use ping as the sole connectivity criterion.
3. Clash system proxy + Rule: same private TCP cases through explicit HTTP/SOCKS proxy, plus a public canary through the selected ordinary proxy. Confirm the private requests are not visible at the public proxy. Test invalid suffix/resolver, IPv6 disabled and nonexistent interface.
4. Test experimental TUN with separately approved routes/DNS/firewall impact: verify actual process-name rules match Tailscale transport, tailscale endpoints use physical uplink, private IPs use native route, public traffic uses proxy. Confirm Windows DNS behavior without automatically disabling strict-route or firewall protections.
5. Observe DNS A/AAAA (and explicitly configure/test PTR if needed) with an approved capture method. Private suffix queries must use the intended resolver; public resolver logs must not receive them. Browser/app DoH needs its own review.
6. Disconnect, reconnect, switch uplink and sleep/wake. Verify stale interface diagnostics, private failure behavior, then restoration using the same interface or a newly reviewed name. TUN-excluded traffic is not automatically fail-closed: validate the separately approved native policy. New exit-node selection after Apply is not continuously watched.
7. Try Rule→Global/Direct in app; expect refusal. Do not bypass with external-controller writes. Disable coexistence, regenerate, and compare with original controls/profile. Verify native Tailscale connection and OS settings match the pre-test state.

Remaining user decisions: exact destinations and MagicDNS suffix; physical/native interface and daemon identification; one DNS owner; whether experimental TUN is needed; an approved disconnect policy if OS-level fail-closed behavior is required. Cloudflare Mesh remains deferred per the narrowed request.
