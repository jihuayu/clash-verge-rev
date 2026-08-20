const MIN_TAILSCALE_CORE = {
  major: 1,
  minor: 19,
  patch: 25,
} as const

/** Mihomo gained `type: tailscale` in v1.19.25. Alpha builds omit a stable triple. */
export function isTailscaleCoreSupported(version?: string | null): boolean {
  if (!version || version === '-') {
    return false
  }

  const match = version.match(/v?(\d+)\.(\d+)\.(\d+)/)
  if (!match) {
    return true
  }

  const major = Number(match[1])
  const minor = Number(match[2])
  const patch = Number(match[3])
  if (major !== MIN_TAILSCALE_CORE.major) {
    return major > MIN_TAILSCALE_CORE.major
  }
  if (minor !== MIN_TAILSCALE_CORE.minor) {
    return minor > MIN_TAILSCALE_CORE.minor
  }
  return patch >= MIN_TAILSCALE_CORE.patch
}
