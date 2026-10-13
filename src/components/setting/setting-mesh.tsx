import {
  CheckCircleRounded,
  ComputerRounded,
  HubRounded,
  LaunchRounded,
  LoginRounded,
  LogoutRounded,
  RefreshRounded,
  SettingsRounded,
} from '@mui/icons-material'
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  Collapse,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Divider,
  Stack,
  TextField,
  Typography,
} from '@mui/material'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useVerge } from '@/hooks/use-verge'
import type { EmbeddedStatus } from '@/services/bindings'
import {
  disconnectEmbeddedTailscale,
  getEmbeddedTailscaleStatus,
  loginEmbeddedTailscale,
  logoutEmbeddedTailscale,
  openEmbeddedTailscaleLogin,
} from '@/services/cmds'

type Status = Required<Pick<EmbeddedStatus, 'state' | 'ips'>> &
  Partial<EmbeddedStatus>

const defaults: Required<IEmbeddedTailscale> = {
  enabled: false,
  hostname: 'clash-verge',
  domains: ['ts.net'],
  cidrs: ['100.64.0.0/10', 'fd7a:115c:a1e0::/48'],
}
const splitLines = (text: string) =>
  text
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean)
const errorText = (error: unknown) =>
  typeof error === 'object' && error && 'detail' in error
    ? String(error.detail)
    : String(error)

export default function SettingMesh() {
  const { verge, patchVerge } = useVerge()
  const { i18n } = useTranslation()
  const zh = i18n.language.startsWith('zh')
  const copy = (cn: string, en: string) => (zh ? cn : en)
  const savedConfig = verge?.embedded_tailscale
  const config = {
    enabled: savedConfig?.enabled ?? defaults.enabled,
    hostname: savedConfig?.hostname ?? defaults.hostname,
    domains: savedConfig?.domains ?? defaults.domains,
    cidrs: savedConfig?.cidrs ?? defaults.cidrs,
  }
  const [open, setOpen] = useState(false)
  const [advanced, setAdvanced] = useState(false)
  const [status, setStatus] = useState<Status>({ state: 'Disabled', ips: [] })
  const [hostname, setHostname] = useState(config.hostname)
  const [domains, setDomains] = useState(config.domains.join('\n'))
  const [cidrs, setCidrs] = useState(config.cidrs.join('\n'))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [confirmLogout, setConfirmLogout] = useState(false)
  const generationRef = useRef(0)
  const connected = config.enabled && status.state === 'Running'
  const waiting =
    config.enabled &&
    ['NeedsLogin', 'Starting', 'NeedsMachineAuth', 'NoState'].includes(
      status.state,
    )
  const label = !config.enabled
    ? copy('未连接', 'Disconnected')
    : connected
      ? copy('已连接', 'Connected')
      : status.state === 'NeedsMachineAuth'
        ? copy('等待管理员批准', 'Awaiting admin approval')
        : waiting
          ? copy('等待授权', 'Awaiting authorization')
          : copy('尚未连接', 'Not connected')

  useEffect(() => {
    const current = ++generationRef.current
    if (!config.enabled) {
      return
    }
    let stopped = false
    const poll = async () => {
      try {
        const next = await getEmbeddedTailscaleStatus()
        if (!stopped && current === generationRef.current) {
          setStatus(next)
        }
      } catch (err) {
        if (!stopped && current === generationRef.current) {
          setStatus({ state: 'Unavailable', ips: [] })
          setError(errorText(err))
        }
      }
    }
    void poll()
    const timer = window.setInterval(
      () => {
        void poll()
      },
      open ? 2500 : 10000,
    )
    return () => {
      stopped = true
      window.clearInterval(timer)
    }
  }, [config.enabled, open])

  const run = async (operation: () => Promise<void>) => {
    if (busy) return
    setBusy(true)
    setError('')
    try {
      await operation()
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }
  const connect = () =>
    run(async () => {
      const legacy = verge?.mesh_coexistence
      await patchVerge({
        embedded_tailscale: {
          enabled: true,
          hostname: hostname.trim(),
          domains: splitLines(domains),
          cidrs: splitLines(cidrs),
        },
        ...(legacy?.enabled
          ? { mesh_coexistence: { ...legacy, enabled: false } }
          : {}),
      })
      setStatus(await loginEmbeddedTailscale())
    })
  const disconnect = () =>
    run(async () => {
      await disconnectEmbeddedTailscale()
      await patchVerge({ embedded_tailscale: { ...config, enabled: false } })
      ++generationRef.current
      setStatus({ state: 'Disabled', ips: [] })
    })
  const logout = () =>
    run(async () => {
      await logoutEmbeddedTailscale()
      await patchVerge({ embedded_tailscale: { ...config, enabled: false } })
      ++generationRef.current
      setStatus({ state: 'Disabled', ips: [] })
      setConfirmLogout(false)
    })

  return (
    <Box sx={{ p: 2, borderTop: 1, borderColor: 'divider' }}>
      <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center' }}>
        <Box
          sx={{
            bgcolor: 'action.hover',
            borderRadius: 2,
            p: 1,
            display: 'flex',
          }}
        >
          <HubRounded color="primary" />
        </Box>
        <Box sx={{ flex: 1 }}>
          <Typography sx={{ fontWeight: 600 }}>Tailscale</Typography>
          <Typography variant="caption" color="text.secondary">
            {copy(
              '安全访问你的设备与私有网络',
              'Secure access to your devices and private network',
            )}
          </Typography>
        </Box>
        <Chip
          label={label}
          size="small"
          color={connected ? 'success' : 'default'}
          variant="outlined"
        />
      </Stack>
      <Button
        sx={{ mt: 1.5 }}
        variant={connected ? 'outlined' : 'contained'}
        disableElevation
        startIcon={connected ? <SettingsRounded /> : <LoginRounded />}
        onClick={() => {
          setHostname(config.hostname)
          setDomains(config.domains.join('\n'))
          setCidrs(config.cidrs.join('\n'))
          setError('')
          setOpen(true)
        }}
      >
        {connected
          ? copy('管理连接', 'Manage connection')
          : copy('连接 Tailscale', 'Connect Tailscale')}
      </Button>
      <Dialog
        open={open}
        onClose={() => {
          if (!busy) setOpen(false)
        }}
        fullWidth
        maxWidth="sm"
      >
        <DialogTitle sx={{ pb: 1 }}>
          <Stack direction="row" spacing={1.5} sx={{ alignItems: 'center' }}>
            <HubRounded color="primary" />
            <Typography component="span" variant="h6">
              Tailscale
            </Typography>
            <Chip
              size="small"
              label={label}
              color={connected ? 'success' : 'default'}
            />
          </Stack>
        </DialogTitle>
        <DialogContent>
          <Stack spacing={2.5} sx={{ pt: 1 }}>
            <Typography variant="body2" color="text.secondary">
              {copy(
                '直接在 Clash Verge 登录。私有网络流量通过 Tailscale，其余流量继续使用你的代理规则。',
                'Sign in directly from Clash Verge. Private traffic uses Tailscale; other traffic follows your proxy rules.',
              )}
            </Typography>
            {error && (
              <Alert severity="error" onClose={() => setError('')}>
                {error}
              </Alert>
            )}
            {connected ? (
              <Box
                sx={{
                  p: 2.5,
                  border: 1,
                  borderColor: 'success.main',
                  borderRadius: 2,
                  bgcolor: 'action.hover',
                }}
              >
                <Stack
                  direction="row"
                  spacing={1}
                  sx={{ mb: 2, alignItems: 'center' }}
                >
                  <CheckCircleRounded color="success" />
                  <Typography sx={{ fontWeight: 600 }}>
                    {copy('已加入你的 tailnet', 'Connected to your tailnet')}
                  </Typography>
                </Stack>
                <Stack spacing={1}>
                  <Typography variant="body2">
                    <ComputerRounded fontSize="inherit" sx={{ mr: 1 }} />
                    {status.hostname ?? config.hostname}
                  </Typography>
                  {status.tailnet && (
                    <Typography variant="body2" color="text.secondary">
                      {status.tailnet}
                    </Typography>
                  )}
                  <Typography
                    variant="body2"
                    sx={{ fontFamily: 'monospace', overflowWrap: 'anywhere' }}
                  >
                    {status.ips.join(' · ')}
                  </Typography>
                  {status.dns_name && (
                    <Typography
                      variant="caption"
                      color="text.secondary"
                      sx={{ overflowWrap: 'anywhere' }}
                    >
                      {status.dns_name}
                    </Typography>
                  )}
                </Stack>
              </Box>
            ) : (
              <>
                <TextField
                  label={copy('设备名称', 'Device name')}
                  value={hostname}
                  onChange={(e) => setHostname(e.target.value)}
                  disabled={busy || config.enabled}
                  helperText={copy(
                    '此名称会显示在 Tailscale 设备列表中',
                    'This name appears in your Tailscale device list',
                  )}
                  fullWidth
                />
                {config.enabled && (
                  <Box
                    sx={{ p: 2.5, bgcolor: 'action.hover', borderRadius: 2 }}
                  >
                    <Stack
                      direction="row"
                      spacing={1.5}
                      sx={{ mb: 1, alignItems: 'center' }}
                    >
                      <CircularProgress size={18} />
                      <Typography sx={{ fontWeight: 600 }}>
                        {status.state === 'NeedsMachineAuth'
                          ? copy(
                              '请管理员批准这台设备',
                              'Ask your administrator to approve this device',
                            )
                          : copy(
                              '在浏览器完成授权',
                              'Complete authorization in your browser',
                            )}
                      </Typography>
                    </Stack>
                    <Typography
                      variant="body2"
                      color="text.secondary"
                      sx={{ mb: 2, alignItems: 'center' }}
                    >
                      {copy(
                        '使用你已有的 Tailscale 账号登录。完成后，此页面会自动更新。',
                        'Use your existing Tailscale account. This page updates automatically when authorization completes.',
                      )}
                    </Typography>
                    <Button
                      variant="contained"
                      disableElevation
                      startIcon={<LaunchRounded />}
                      disabled={busy || !status.auth_url}
                      onClick={() => {
                        void run(async () => {
                          await openEmbeddedTailscaleLogin()
                        })
                      }}
                    >
                      {copy(
                        '打开 Tailscale 授权页',
                        'Open Tailscale authorization',
                      )}
                    </Button>
                    {!status.auth_url && (
                      <Button
                        sx={{ ml: 1 }}
                        disabled={busy}
                        startIcon={<RefreshRounded />}
                        onClick={() => {
                          void run(async () => {
                            setStatus(await loginEmbeddedTailscale())
                          })
                        }}
                      >
                        {copy('重试', 'Retry')}
                      </Button>
                    )}
                  </Box>
                )}
              </>
            )}
            <Box>
              <Button
                size="small"
                startIcon={<SettingsRounded />}
                onClick={() => setAdvanced(!advanced)}
                aria-expanded={advanced}
              >
                {copy('高级设置', 'Advanced settings')}
              </Button>
              <Collapse in={advanced}>
                <Stack spacing={2} sx={{ pt: 2 }}>
                  <TextField
                    label={copy('MagicDNS 域名后缀', 'MagicDNS suffixes')}
                    value={domains}
                    multiline
                    minRows={2}
                    disabled={busy || config.enabled}
                    onChange={(e) => setDomains(e.target.value)}
                    helperText={copy(
                      '每行一个后缀；默认匹配 ts.net，可缩小到你的 tailnet 后缀',
                      'One suffix per line. Narrow ts.net to your own tailnet suffix if preferred.',
                    )}
                  />
                  <TextField
                    label={copy(
                      'Tailscale 地址范围',
                      'Tailscale address ranges',
                    )}
                    value={cidrs}
                    multiline
                    minRows={2}
                    disabled={busy || config.enabled}
                    onChange={(e) => setCidrs(e.target.value)}
                    helperText={copy(
                      '每行一个 CIDR；修改前请先断开连接',
                      'One CIDR per line. Disconnect before editing.',
                    )}
                  />
                  <Alert severity="info">
                    {copy(
                      '本次连接不开启出口节点、子网路由或入站服务。断开会保留登录信息，退出登录会撤销本地会话。',
                      'Exit nodes, subnet routes and inbound services are off. Disconnect keeps your login; sign out ends the local session.',
                    )}
                  </Alert>
                </Stack>
              </Collapse>
            </Box>
            {confirmLogout && (
              <Alert
                severity="warning"
                action={
                  <Button
                    color="inherit"
                    size="small"
                    disabled={busy}
                    onClick={() => {
                      void logout()
                    }}
                  >
                    {copy('确认退出', 'Confirm sign out')}
                  </Button>
                }
              >
                {copy(
                  '退出后需要重新在浏览器授权。',
                  'You will need to authorize again in your browser.',
                )}
              </Alert>
            )}
          </Stack>
        </DialogContent>
        <Divider />
        <DialogActions sx={{ p: 2, justifyContent: 'space-between' }}>
          <Box>
            {config.enabled && (
              <Button
                color="inherit"
                startIcon={<LogoutRounded />}
                disabled={busy}
                onClick={() => setConfirmLogout(!confirmLogout)}
              >
                {copy('退出登录', 'Sign out')}
              </Button>
            )}
          </Box>
          <Stack direction="row" spacing={1}>
            <Button disabled={busy} onClick={() => setOpen(false)}>
              {copy('关闭', 'Close')}
            </Button>
            {config.enabled ? (
              <Button
                variant="outlined"
                disabled={busy}
                onClick={() => {
                  void disconnect()
                }}
              >
                {copy('断开连接', 'Disconnect')}
              </Button>
            ) : (
              <Button
                variant="contained"
                disableElevation
                startIcon={
                  busy ? <CircularProgress size={16} /> : <LoginRounded />
                }
                disabled={busy || !hostname.trim()}
                onClick={() => {
                  void connect()
                }}
              >
                {copy('登录并连接', 'Sign in and connect')}
              </Button>
            )}
          </Stack>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
