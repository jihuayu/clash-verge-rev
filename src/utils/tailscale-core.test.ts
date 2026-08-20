import { describe, expect, it } from 'vitest'

import { isTailscaleCoreSupported } from './tailscale-core'

describe('isTailscaleCoreSupported', () => {
  it('accepts the first release that shipped the outbound', () => {
    expect(isTailscaleCoreSupported('v1.19.25 Mihomo')).toBe(true)
    expect(isTailscaleCoreSupported('1.19.30')).toBe(true)
  })

  it('rejects older stable cores', () => {
    expect(isTailscaleCoreSupported('v1.19.24 Mihomo')).toBe(false)
    expect(isTailscaleCoreSupported('1.18.0')).toBe(false)
  })

  it('allows alpha or unknown version strings', () => {
    expect(isTailscaleCoreSupported('Alpha-abc123 Mihomo')).toBe(true)
  })

  it('rejects an unloaded version', () => {
    expect(isTailscaleCoreSupported('-')).toBe(false)
    expect(isTailscaleCoreSupported(undefined)).toBe(false)
  })
})
