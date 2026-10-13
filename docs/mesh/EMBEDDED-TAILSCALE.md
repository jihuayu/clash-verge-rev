# Embedded Tailscale Windows preview

The settings page enrolls an embedded `tsnet` node inside Mihomo. A separate
Tailscale installation is not needed. Clash Verge handles configuration, browser
login and status; the Mihomo fork exposes management through its existing
authenticated controller. Native client coexistence and embedded mode cannot be
enabled together.

## Source and build

- Verge branch: `feat/mesh-coexistence`.
- Mihomo fork: https://github.com/jihuayu/mihomo/tree/5d52770
- Mihomo management source commit: `5d52770` (`codex/tailscale-login`).
- Node management endpoint: `/proxies/{name}/tailscale` with GET (status), POST
  (login/resume), PATCH (disconnect while retaining identity), DELETE (logout).
- Status responses use `Cache-Control: no-store`, omit peer lists and keys, and
  redact internal failures. Login URLs are opened only on `https://login.tailscale.com`.

Build the pinned Mihomo checkout with Go 1.26.5 and `CGO_ENABLED=0`:

```powershell
$env:GOTOOLCHAIN = 'go1.26.5'
$env:CGO_ENABLED = '0'
go test -tags with_gvisor ./hub/route -run Tailscale -count=1
go build -tags with_gvisor -ldflags '-X github.com/metacubex/mihomo/constant.Version=mesh-tailscale-20261013.1' -o bin/mihomo-tailscale.exe .
```

In the Verge checkout, install dependencies, prepare resources, then replace both
sidecars **after** prebuild (prebuild downloads the stock core):

```powershell
pnpm install --frozen-lockfile --ignore-scripts
node scripts/prebuild.mjs x86_64-pc-windows-msvc
Copy-Item ../mihomo/bin/mihomo-tailscale.exe src-tauri/sidecar/verge-mihomo-x86_64-pc-windows-msvc.exe -Force
Copy-Item ../mihomo/bin/mihomo-tailscale.exe src-tauri/sidecar/verge-mihomo-alpha-x86_64-pc-windows-msvc.exe -Force
$env:CARGO_PROFILE_DEV_DEBUG = '0'
pnpm tauri build --debug --no-bundle --features coexistence-test --config src-tauri/tauri.coexistence-test.conf.json
```

The Windows preview was built with Rust 1.97.1 (`RUSTUP_TOOLCHAIN=stable`). It is an
unsigned debug build of app version 2.5.3, identified by the release tag rather
than a stable version bump. Distribution includes `clash-verge.exe`, both sidecar
executables and `resources/`. Extract the entire archive before running; WebView2
Runtime must be installed. App configuration remains in its own AppData directory,
so the ZIP is an unpacked distribution, not a portable settings directory.

## Usage and isolation

Open Settings → Tailscale, choose a device name, and select login/connect. When
the authorization link is ready, open the browser and authorize the device in
your tailnet. The UI polls the core and shows the node name, addresses and tailnet
after login. Disconnect retains the stored identity; sign out requires browser
authorization again.

The `coexistence-test` build uses a separate application ID and data directory,
defaults to proxy port 17897, blocks TUN/service mutations and disables updater
checks and automatic DNS writes. It does not automatically enable the system
proxy. It does not reuse a node enrolled by the standalone test script.

The optional `scripts/tailscale-local-test.ps1` is a source-checkout test harness:
it expects the sibling Mihomo checkout and keeps private runtime data under
`%LOCALAPPDATA%\ClashVerge-Tailscale-Test`. Its proxy/controller listen on loopback
18497/18498, with a generated controller secret and TUN disabled. Never include
that runtime directory, node identity or session file in a release.

## Verification and limits (2026-10-13)

- Rust configuration tests: 13 passed; frontend unit tests: 15 passed.
- TypeScript, frontend lint, production web build and Windows desktop build passed.
- Mihomo management regression tests include the actual parsed proxy wrapper,
  HTTP method dispatch and error redaction.
- A standalone core enrolled through Chrome and reached `Running`; private DNS
  returned the expected node address. HTTP requests through the explicit local
  proxy reached the approved peer by IP and full MagicDNS name on ports 5000 and
  8080 (dashboard), returning 200.
- Short single-label names returned 502. Use full `device.tailnet.ts.net` names;
  automatic search-suffix expansion is not implemented.
- Desktop login/status/disconnect layouts were checked with mocked IPC. Native
  desktop enrollment, disconnect/resume and logout have not received full live
  end-to-end validation. The latest explicit disconnect API has regression tests;
  it was added after the standalone enrollment test.
- Exit nodes, subnet route acceptance and inbound services are off. This preview
  does not validate UDP, sleep/resume, TUN, other operating systems or stable
  updater delivery. It is a pre-release for testing.
