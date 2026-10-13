import {
  Alert,
  Box,
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Stack,
  TextField,
  Typography,
} from '@mui/material'
import { invoke } from '@tauri-apps/api/core'
import { useState } from 'react'

import { useClash } from '@/hooks/use-clash'
import { useVerge } from '@/hooks/use-verge'

const initial: IMeshCoexistence = {
  enabled: false,
  external_policy_reviewed: false,
  experimental_tun: false,
  internet_interface: '',
  transport_processes: [],
  providers: [
    {
      kind: 'tailscale',
      interface: '',
      cidrs: [],
      domains: [],
      dns_server: null,
      default_route: false,
      traffic_and_dns: false,
    },
  ],
}
interface Preview {
  tailscale: { state: string; exit_node_selected: boolean }
  yaml: string
  diagnostics: { code: string; message: string }[]
}

export default function SettingMesh() {
  const { verge, patchVerge } = useVerge()
  const { clash } = useClash()
  const [open, setOpen] = useState(false)
  const [text, setText] = useState('')
  const [preview, setPreview] = useState<Preview>()
  const [reviewed, setReviewed] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  const inspect = async () => {
    setBusy(true)
    setError('')
    setPreview(undefined)
    setReviewed('')
    try {
      // This is a controls-only preview. Save validates the full enhanced profile again.
      const result = await invoke<Preview>('preview_mesh_coexistence', {
        mesh: JSON.parse(text) as IMeshCoexistence,
        base: {
          mode: clash?.mode ?? 'rule',
          ipv6: clash?.ipv6 ?? false,
          tun: { enable: verge?.enable_tun_mode ?? false },
        },
      })
      setPreview(result)
      setReviewed(text)
    } catch (err) {
      setError(
        typeof err === 'object' && err && 'detail' in err
          ? String(err.detail)
          : String(err),
      )
    } finally {
      setBusy(false)
    }
  }
  const save = async () => {
    setBusy(true)
    setError('')
    try {
      await patchVerge({
        mesh_coexistence: JSON.parse(text) as IMeshCoexistence,
      })
      setOpen(false)
    } catch (err) {
      setError(
        typeof err === 'object' && err && 'detail' in err
          ? String(err.detail)
          : String(err),
      )
    } finally {
      setBusy(false)
    }
  }

  return (
    <Box sx={{ p: 2 }}>
      <Typography>Tailscale coexistence · Experimental</Typography>
      <Button
        onClick={() => {
          setText(JSON.stringify(verge?.mesh_coexistence ?? initial, null, 2))
          setPreview(undefined)
          setReviewed('')
          setError('')
          setOpen(true)
        }}
      >
        Configure / Preview
      </Button>
      <Dialog
        open={open}
        onClose={() => {
          if (!busy) setOpen(false)
        }}
        fullWidth
        maxWidth="md"
      >
        <DialogTitle>Tailscale coexistence</DialogTitle>
        <DialogContent>
          <Stack spacing={2}>
            <Alert severity="warning">
              Rule mode only. Review actual Tailscale host/subnet routes, native
              client transport exclusions and one OS DNS owner. Exit nodes are
              unsupported. Cloudflare Mesh is deferred. No enrollment or OS
              settings are changed by Preview.
            </Alert>
            <Typography variant="body2">
              Set enabled to true only after review. Enter your actual interface
              names and CIDRs; no CGNAT or private ranges are added
              automatically. Each private DNS server must be listed in cidrs.
              TUN requires separate experimental_tun opt-in. To restore the
              original generated configuration, set enabled to false and Apply.
            </Typography>
            <TextField
              multiline
              minRows={14}
              label="Coexistence configuration (JSON)"
              value={text}
              onChange={(e) => {
                setText(e.target.value)
                setPreview(undefined)
                setReviewed('')
              }}
              spellCheck={false}
              disabled={busy}
            />
            {error && <Alert severity="error">{error}</Alert>}
            {preview && (
              <>
                <Alert severity="info">
                  Tailscale client: {preview.tailscale.state}. Running is not
                  proof of TCP connectivity.
                </Alert>
                {preview.diagnostics.map((d) => (
                  <Alert key={d.code} severity="warning">
                    {d.message}
                  </Alert>
                ))}
                <Typography variant="body2">
                  Controls-only preview; Apply revalidates the full profile
                  including scripts and DNS settings. Interface presence does
                  not prove connectivity. TUN-excluded OS traffic requires
                  native firewall protection during disconnect.
                </Typography>
                <TextField
                  multiline
                  minRows={8}
                  label="Generated configuration preview"
                  value={preview.yaml}
                  slotProps={{ input: { readOnly: true } }}
                />
              </>
            )}
          </Stack>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setOpen(false)} disabled={busy}>
            Cancel
          </Button>
          <Button
            onClick={() => {
              void inspect()
            }}
            disabled={busy}
          >
            Preview / Refresh diagnostics
          </Button>
          <Button
            onClick={() => {
              void save()
            }}
            disabled={busy || reviewed !== text || !preview}
          >
            Apply
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
