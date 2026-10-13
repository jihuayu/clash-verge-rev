param([ValidateSet('Start', 'Stop', 'Status', 'Restart')][string]$Action = 'Start')
$ErrorActionPreference = 'Stop'
$runtimeDir = Join-Path $env:LOCALAPPDATA 'ClashVerge-Tailscale-Test'
$corePath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\mihomo\bin\mihomo-tailscale.exe'))
$sessionPath = Join-Path $runtimeDir 'session.json'
if ($Action -eq 'Status') {
    $session = Get-Content -LiteralPath $sessionPath -Raw | ConvertFrom-Json
    $status = Invoke-RestMethod 'http://127.0.0.1:18498/proxies/VERGE-TAILSCALE/tailscale' -Headers @{Authorization="Bearer $($session.secret)"}
    $status | Select-Object state,hostname,dns_name,ips,tailnet
    exit
}
if ($Action -eq 'Stop' -or $Action -eq 'Restart') {
    $session = Get-Content -LiteralPath $sessionPath -Raw | ConvertFrom-Json
    $process = Get-Process -Id $session.pid -ErrorAction SilentlyContinue
    if ($process -and $process.Path -eq $corePath) { Stop-Process -Id $process.Id; Wait-Process -Id $process.Id -ErrorAction SilentlyContinue }
    if ($Action -eq 'Stop') { Write-Output 'Stopped the isolated core. Login state is retained.'; exit }
    $nextCore = Join-Path (Split-Path $corePath) 'mihomo-tailscale-next.exe'
    if (Test-Path -LiteralPath $nextCore) {
        for ($copyAttempt = 0; ; $copyAttempt++) {
            try {
                Copy-Item -LiteralPath $nextCore -Destination $corePath -Force
                break
            } catch [System.IO.IOException] {
                if ($copyAttempt -ge 9) { throw }
                Start-Sleep -Milliseconds 500
            }
        }
    }
}
if (!(Test-Path -LiteralPath $corePath)) { throw "Build the Mihomo fork first: $corePath" }
if (Get-NetTCPConnection -State Listen -LocalPort 18497,18498 -ErrorAction SilentlyContinue) { throw 'Test ports are occupied; no changes made.' }
New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null
$secret = [guid]::NewGuid().ToString('N')
$configuration = @"
mode: rule
mixed-port: 18497
bind-address: 127.0.0.1
allow-lan: false
external-controller: 127.0.0.1:18498
secret: $secret
log-level: warning
ipv6: true
tun:
  enable: false
dns:
  enable: true
  enhanced-mode: redir-host
  nameserver: [https://1.1.1.1/dns-query]
  nameserver-policy:
    '+.ts.net': tailscale://VERGE-TAILSCALE
proxies:
  - name: VERGE-TAILSCALE
    type: tailscale
    hostname: clash-verge-windows
    state-dir: tailscale/verge
    udp: true
    accept-routes: false
rules:
  - DOMAIN-SUFFIX,ts.net,VERGE-TAILSCALE
  - IP-CIDR,100.64.0.0/10,VERGE-TAILSCALE,no-resolve
  - IP-CIDR6,fd7a:115c:a1e0::/48,VERGE-TAILSCALE,no-resolve
  - MATCH,DIRECT
"@
$configPath = Join-Path $runtimeDir 'config.yaml'
Set-Content -LiteralPath $configPath -Value $configuration -Encoding utf8
$core = Start-Process -FilePath $corePath -ArgumentList @('-d', "`"$runtimeDir`"", '-f', "`"$configPath`"") -WindowStyle Hidden -RedirectStandardOutput (Join-Path $runtimeDir 'stdout.log') -RedirectStandardError (Join-Path $runtimeDir 'stderr.log') -PassThru
@{pid=$core.Id;secret=$secret} | ConvertTo-Json | Set-Content -LiteralPath $sessionPath
Write-Output "Isolated Mihomo started (PID $($core.Id)). Proxy: 127.0.0.1:18497. TUN off."
