import { Button, TextField, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useCallback, useEffect, useState, type ChangeEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { delayProxyByName } from 'tauri-plugin-mihomo-api'

import { Switch, TooltipIcon } from '@/components/base'
import { useClash } from '@/hooks/use-clash'
import { useVerge } from '@/hooks/use-verge'
import {
  getTailscaleStatus,
  openWebUrl,
  warmupTailscale,
} from '@/services/cmds'
import { subscribeVergeEvents } from '@/services/events'
import { useQuery } from '@/services/query-client'
import { DEFAULT_DELAY_TIMEOUT } from '@/utils/delay'
import { isTailscaleCoreSupported } from '@/utils/tailscale-core'

import { GuardState } from './mods/guard-state'
import { SettingItem, SettingList } from './mods/setting-comp'

interface Props {
  onError?: (err: Error) => void
}

const SettingTailscale = ({ onError }: Props) => {
  const { t } = useTranslation()
  const { verge, mutateVerge, patchVerge } = useVerge()
  const { version } = useClash()
  const coreSupported = isTailscaleCoreSupported(version)

  const { enable_tailscale, tailscale_auth_key, tailscale_control_url } =
    verge ?? {}

  const enabled = enable_tailscale ?? false
  const [eventLoginUrl, setEventLoginUrl] = useState<string | null>(null)

  const { data: status, refetch } = useQuery({
    queryKey: ['getTailscaleStatus'],
    queryFn: getTailscaleStatus,
    enabled,
    refetchInterval: enabled ? 3000 : false,
  })

  const loginUrl = enabled ? (eventLoginUrl ?? status?.loginUrl ?? null) : null

  useEffect(() => {
    return subscribeVergeEvents({
      'verge://tailscale-login-url': (url) => {
        setEventLoginUrl(url)
      },
    })
  }, [])

  const onSwitchFormat = (_e: ChangeEvent<HTMLInputElement>, value: boolean) =>
    value
  const onTextFormat = (event: ChangeEvent<HTMLInputElement>) =>
    event.target.value

  const onChangeData = (patch: Partial<IVergeConfig>) => {
    mutateVerge({ ...verge, ...patch }, false)
  }

  const onToggle = useLockFn(async (next: boolean) => {
    onChangeData({ enable_tailscale: next })
    try {
      await patchVerge({ enable_tailscale: next })
      if (next) {
        setEventLoginUrl(null)
        await warmupTailscale().catch(() => undefined)
        void delayProxyByName(
          'Tailscale',
          'https://www.gstatic.com/generate_204',
          DEFAULT_DELAY_TIMEOUT,
        ).catch(() => undefined)
        await refetch()
      } else {
        setEventLoginUrl(null)
      }
    } catch (error) {
      onChangeData({ enable_tailscale: !next })
      throw error
    }
  })

  const onOpenLogin = useCallback(() => {
    if (!loginUrl) return
    void openWebUrl(loginUrl)
  }, [loginUrl])

  const statusLabel = (() => {
    if (!coreSupported) {
      return t('settings.sections.tailscale.status.unsupportedCore')
    }
    if (!enabled) {
      return t('settings.sections.tailscale.status.disabled')
    }
    if (status?.alive) {
      return t('settings.sections.tailscale.status.connected')
    }
    if (loginUrl || status?.needsLogin) {
      return t('settings.sections.tailscale.status.needsLogin')
    }
    if (status?.available) {
      return t('settings.sections.tailscale.status.connecting')
    }
    return t('settings.sections.tailscale.status.waiting')
  })()

  return (
    <SettingList title={t('settings.sections.tailscale.title')}>
      <SettingItem
        label={t('settings.sections.tailscale.fields.enable')}
        extra={
          <TooltipIcon
            title={t('settings.sections.tailscale.tooltips.enable')}
            sx={{ opacity: '0.7' }}
          />
        }
      >
        <GuardState
          value={enabled}
          valueProps="checked"
          onCatch={onError}
          onFormat={onSwitchFormat}
          onChange={(value) => onChangeData({ enable_tailscale: value })}
          onGuard={onToggle}
        >
          <Switch edge="end" disabled={!coreSupported} />
        </GuardState>
      </SettingItem>

      <SettingItem label={t('settings.sections.tailscale.fields.status')}>
        <Typography
          variant="body2"
          color="text.secondary"
          sx={{ maxWidth: 220, textAlign: 'right' }}
        >
          {statusLabel}
        </Typography>
      </SettingItem>

      {enabled && (loginUrl || status?.needsLogin) ? (
        <SettingItem label={t('settings.sections.tailscale.fields.login')}>
          <Button
            size="small"
            variant="outlined"
            disabled={!loginUrl}
            onClick={onOpenLogin}
          >
            {t('settings.sections.tailscale.actions.openLogin')}
          </Button>
        </SettingItem>
      ) : null}

      <SettingItem
        label={t('settings.sections.tailscale.fields.authKey')}
        extra={
          <TooltipIcon
            title={t('settings.sections.tailscale.tooltips.authKey')}
            sx={{ opacity: '0.7' }}
          />
        }
      >
        <GuardState
          value={tailscale_auth_key ?? ''}
          waitTime={400}
          onCatch={onError}
          onFormat={onTextFormat}
          onChange={(value) => onChangeData({ tailscale_auth_key: value })}
          onGuard={(value) => patchVerge({ tailscale_auth_key: value })}
        >
          <TextField
            size="small"
            type="password"
            autoComplete="new-password"
            placeholder={t('settings.sections.tailscale.placeholders.authKey')}
            sx={{ width: 220 }}
          />
        </GuardState>
      </SettingItem>

      <SettingItem
        label={t('settings.sections.tailscale.fields.controlUrl')}
        extra={
          <TooltipIcon
            title={t('settings.sections.tailscale.tooltips.controlUrl')}
            sx={{ opacity: '0.7' }}
          />
        }
      >
        <GuardState
          value={tailscale_control_url ?? ''}
          waitTime={400}
          onCatch={onError}
          onFormat={onTextFormat}
          onChange={(value) => onChangeData({ tailscale_control_url: value })}
          onGuard={(value) => patchVerge({ tailscale_control_url: value })}
        >
          <TextField
            size="small"
            autoComplete="off"
            placeholder={t(
              'settings.sections.tailscale.placeholders.controlUrl',
            )}
            sx={{ width: 220 }}
          />
        </GuardState>
      </SettingItem>
    </SettingList>
  )
}

export default SettingTailscale
