import { IconCheck, IconClock, IconDeviceFloppy, IconLink } from '@tabler/icons-react'
import { useCallback, useEffect, useState } from 'react'
import { apiCall } from '../../lib/api'
import { useAppContext } from '../../lib/store'
import { Button } from '../ui/button'
import { InputGroup, InputGroupAddon, InputGroupInput } from '../ui/input-group'
import { Spinner } from '../ui/spinner'
import { Switch } from '../ui/switch'

type SettingsResponse = {
  success: boolean
  error?: string
  warning?: string
}

function normalizeRouterConfigUrl(value: string): string {
  const trimmed = value.trim()
  const unwrapped =
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'")) ||
    (trimmed.startsWith('`') && trimmed.endsWith('`')) ||
    (trimmed.startsWith('<') && trimmed.endsWith('>'))
      ? trimmed.slice(1, -1).trim()
      : trimmed
  return unwrapped.replaceAll('&amp;', '&')
}

function isYamlHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(value)
    return ['http:', 'https:'].includes(parsed.protocol) && parsed.pathname.toLowerCase().endsWith('.yaml')
  } catch {
    return false
  }
}

export function RouterConfigCard() {
  const { state, dispatch, showToast } = useAppContext({ includeSettings: true })
  const { settings } = state
  const [url, setUrl] = useState(settings.routerConfigUrl)
  const [busy, setBusy] = useState(false)
  const normalizedUrl = normalizeRouterConfigUrl(url)
  const hasChanges = normalizedUrl !== settings.routerConfigUrl

  useEffect(() => setUrl(settings.routerConfigUrl), [settings.routerConfigUrl])

  const saveUrl = useCallback(async () => {
    if (!isYamlHttpUrl(normalizedUrl)) {
      showToast('Укажите HTTP(S)-ссылку на файл .yaml', 'error')
      return
    }

    const autoUpdate = settings.routerConfigUrl ? settings.routerConfigAutoUpdate : true
    setBusy(true)
    try {
      const result = await apiCall<SettingsResponse>('PATCH', 'settings', {
        router_config: { url: normalizedUrl, auto_update: autoUpdate },
      })
      if (!result.success) {
        showToast(`Ошибка: ${result.error ?? 'Не удалось сохранить ссылку'}`, 'error')
        return
      }
      dispatch({
        type: 'SET_SETTINGS',
        settings: { routerConfigUrl: normalizedUrl, routerConfigAutoUpdate: autoUpdate },
      })
      setUrl(normalizedUrl)
      if (result.warning) showToast(result.warning, 'error')
      else showToast('Конфигурация роутера обновлена', 'success')
    } catch (error) {
      showToast(error instanceof Error ? error.message : 'Не удалось сохранить ссылку', 'error')
    } finally {
      setBusy(false)
    }
  }, [dispatch, normalizedUrl, settings.routerConfigAutoUpdate, settings.routerConfigUrl, showToast])

  const toggleAutoUpdate = useCallback(
    async (value: boolean) => {
      setBusy(true)
      try {
        const result = await apiCall<SettingsResponse>('PATCH', 'settings', {
          router_config: { auto_update: value },
        })
        if (!result.success) {
          showToast(`Ошибка: ${result.error ?? 'Не удалось изменить расписание'}`, 'error')
          return
        }
        dispatch({ type: 'SET_SETTINGS', settings: { routerConfigAutoUpdate: value } })
        if (result.warning) showToast(result.warning, 'error')
        else showToast(value ? 'Ежедневное обновление включено' : 'Ежедневное обновление отключено', 'success')
      } catch (error) {
        showToast(error instanceof Error ? error.message : 'Не удалось изменить расписание', 'error')
      } finally {
        setBusy(false)
      }
    },
    [dispatch, showToast]
  )

  return (
    <section className="dhq-router-config-card">
      <div className="dhq-router-config-copy">
        <span className="dhq-eyebrow">Автообновление</span>
        <h2>Конфигурация роутера</h2>
        <p>
          Вставьте персональную ссылку на <code>.yaml</code>. Конфигурация загрузится сразу после сохранения.
        </p>
      </div>

      <div className="dhq-router-config-controls">
        <div className="dhq-router-config-input">
          <InputGroup>
            <InputGroupAddon>
              <IconLink />
            </InputGroupAddon>
            <InputGroupInput
              id="router-config-url"
              type="url"
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              onKeyDown={(event) => event.key === 'Enter' && hasChanges && !busy && void saveUrl()}
              placeholder="https://example.com/config.yaml"
              aria-label="Ссылка на конфигурацию роутера"
            />
          </InputGroup>
          <Button className="dhq-primary-action" onClick={() => void saveUrl()} disabled={!hasChanges || !normalizedUrl || busy}>
            {busy ? <Spinner /> : settings.routerConfigUrl && !hasChanges ? <IconCheck /> : <IconDeviceFloppy />}
            {busy ? 'Сохраняем…' : settings.routerConfigUrl && !hasChanges ? 'Сохранено' : 'Сохранить'}
          </Button>
        </div>

        <div className="dhq-router-config-schedule">
          <span className="dhq-router-config-schedule__icon">
            <IconClock />
          </span>
          <div>
            <strong>Ежедневное обновление</strong>
            <small>Автоматически проверять конфигурацию каждый день в 03:00</small>
          </div>
          <Switch
            id="router-config-auto-update"
            checked={settings.routerConfigAutoUpdate}
            disabled={!settings.routerConfigUrl || busy}
            onCheckedChange={(value) => void toggleAutoUpdate(value)}
            aria-label="Ежедневное обновление конфигурации"
          />
        </div>
      </div>
    </section>
  )
}
