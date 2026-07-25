import {
  IconActivityHeartbeat,
  IconArrowDownRight,
  IconArrowUpRight,
  IconBolt,
  IconCheck,
  IconDeviceDesktop,
  IconPlayerPlayFilled,
  IconRefresh,
  IconRoute,
  IconRouter,
  IconServer,
  IconShieldCheck,
  IconShieldOff,
  IconTopologyStar3,
  IconWorld,
} from '@tabler/icons-react'
import { useEffect, useMemo, useState } from 'react'
import { apiCall, clashFetch } from '../../lib/api'
import { useAppContext, useConnections, useProxiesStore, useWsConnected } from '../../lib/store'
import { cn } from '../../lib/utils'
import { ConnectionsPanel } from '../configuration/mihomo/Connections'
import { SelectorsPanel } from '../configuration/mihomo/Selectors'
import { LogPanel } from '../log/LogPanel'
import { Button } from '../ui/button'
import { Spinner } from '../ui/spinner'

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0 Б'
  const units = ['Б', 'КБ', 'МБ', 'ГБ', 'ТБ']
  const unit = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1)
  const amount = value / 1024 ** unit
  return `${amount >= 100 || unit === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[unit]}`
}

function selectedRoute(proxies: Record<string, any>) {
  const selectors = Object.values(proxies).filter((proxy: any) => proxy?.type === 'Selector' && proxy.now)
  const selector = (proxies.GLOBAL?.now && proxies[proxies.GLOBAL.now]) || selectors[0]
  const selectedName = selector?.now ?? proxies.GLOBAL?.now ?? 'Автоматический маршрут'
  const selected = proxies[selectedName]
  const delay = selected?.history?.at?.(-1)?.delay
  return {
    group: selector?.name ?? proxies.GLOBAL?.now ?? 'Основной маршрут',
    name: selectedName,
    delay: typeof delay === 'number' && delay > 0 ? delay : null,
  }
}

export function RouterDashboard({ onOpenRouting }: { onOpenRouting: () => void }) {
  const { state, dispatch, showToast } = useAppContext({ includeConfigs: true })
  const connections = useConnections()
  const proxies = useProxiesStore((s) => s.proxies)
  const [busy, setBusy] = useState(false)
  const isRunning = state.serviceStatus === 'running'
  const isPending = state.serviceStatus === 'pending' || state.serviceStatus === 'loading'
  const route = useMemo(() => selectedRoute(proxies), [proxies])

  const totals = useMemo(
    () =>
      connections.reduce(
        (result, connection) => ({
          upload: result.upload + (connection.upload || 0),
          download: result.download + (connection.download || 0),
        }),
        { upload: 0, download: 0 }
      ),
    [connections]
  )

  const changeServiceState = async () => {
    if (busy || isPending) return
    setBusy(true)
    const action = isRunning ? 'hardRestart' : 'start'
    dispatch({
      type: 'SET_SERVICE_STATUS',
      status: 'pending',
      pendingText: isRunning ? 'Перезапуск…' : 'Подключение…',
    })
    try {
      const result = await apiCall<{ success: boolean; output?: string; error?: string }>('POST', 'control', { action })
      dispatch({ type: 'SET_SERVICE_STATUS', status: result.success ? 'running' : 'stopped' })
      showToast(result.success ? (isRunning ? 'Маршрутизация перезапущена' : 'Защита включена') : result.output || result.error || 'Команда не выполнена', result.success ? 'success' : 'error')
    } catch (error) {
      dispatch({ type: 'SET_SERVICE_STATUS', status: isRunning ? 'running' : 'stopped' })
      showToast(error instanceof Error ? error.message : 'Не удалось связаться с роутером', 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="dhq-dashboard">
      <section className={cn('dhq-protection-card', isRunning && 'dhq-protection-card--running')}>
        <div className="dhq-protection-copy">
          <span className="dhq-eyebrow">{isRunning ? 'Защита активна' : 'Требуется действие'}</span>
          <div className="dhq-protection-title">
            <div className="dhq-protection-icon">
              {isRunning ? <IconShieldCheck /> : <IconShieldOff />}
            </div>
            <div>
              <h2>{isRunning ? 'Подключено' : isPending ? 'Подключаем…' : 'Отключено'}</h2>
              <p>
                {isRunning
                  ? 'Устройства в домашней сети используют защищённую маршрутизацию.'
                  : 'Включите DHQClash Router, чтобы защитить трафик домашней сети.'}
              </p>
            </div>
          </div>
          <div className="dhq-protection-actions">
            <Button className="dhq-primary-action" onClick={changeServiceState} disabled={busy || isPending}>
              {busy || isPending ? <Spinner /> : isRunning ? <IconRefresh /> : <IconPlayerPlayFilled />}
              {isRunning ? 'Быстрый перезапуск' : 'Включить защиту'}
            </Button>
            <Button variant="outline" onClick={onOpenRouting}>
              <IconRoute data-icon="inline-start" />
              Изменить маршрут
            </Button>
          </div>
        </div>
        <div className="dhq-orbit" aria-hidden="true">
          <span className="dhq-orbit__router"><IconRouter /></span>
          <span className="dhq-orbit__line" />
          <span className="dhq-orbit__world"><IconWorld /></span>
          <span className="dhq-orbit__pulse" />
        </div>
      </section>

      <section className="dhq-route-card">
        <div className="dhq-card-heading">
          <div>
            <span className="dhq-eyebrow">Текущий маршрут</span>
            <h2>{route.name}</h2>
          </div>
          <span className={cn('dhq-delay', !route.delay && 'dhq-delay--unknown')}>
            {route.delay ? `${route.delay} мс` : 'проверяется'}
          </span>
        </div>
        <div className="dhq-route-line">
          <span><IconRouter /> Роутер</span>
          <i />
          <span><IconTopologyStar3 /> {route.group}</span>
          <i />
          <span><IconWorld /> Интернет</span>
        </div>
        <Button variant="ghost" className="px-0 text-[var(--dhq-cyan)] hover:bg-transparent" onClick={onOpenRouting}>
          Открыть маршрутизацию
          <IconRoute data-icon="inline-end" />
        </Button>
      </section>

      <section className="dhq-metrics-grid">
        <article className="dhq-metric-card">
          <span className="dhq-metric-icon"><IconDeviceDesktop /></span>
          <div><small>Активные устройства</small><strong>{new Set(connections.map((connection) => connection.metadata.sourceIP)).size}</strong></div>
          <em>{connections.length} соединений</em>
        </article>
        <article className="dhq-metric-card">
          <span className="dhq-metric-icon"><IconArrowDownRight /></span>
          <div><small>Получено</small><strong>{formatBytes(totals.download)}</strong></div>
          <em>текущая сессия</em>
        </article>
        <article className="dhq-metric-card">
          <span className="dhq-metric-icon"><IconArrowUpRight /></span>
          <div><small>Отправлено</small><strong>{formatBytes(totals.upload)}</strong></div>
          <em>текущая сессия</em>
        </article>
        <article className="dhq-metric-card">
          <span className="dhq-metric-icon"><IconBolt /></span>
          <div><small>Ядро</small><strong>{state.currentCore ? state.currentCore[0].toUpperCase() + state.currentCore.slice(1) : '—'}</strong></div>
          <em>{state.coreVersions[state.currentCore] || 'версия определяется'}</em>
        </article>
      </section>
    </div>
  )
}

export function RoutingView() {
  const { state, showToast } = useAppContext()
  const [mode, setMode] = useState<'rule' | 'global' | 'direct'>('rule')
  const isReady = state.currentCore === 'mihomo' && state.serviceStatus === 'running' && !!(state.clashApiPort || state.clashApiUnix)

  useEffect(() => {
    if (!isReady) return
    clashFetch<{ mode?: 'rule' | 'global' | 'direct' }>(state.clashApiPort ?? '', 'configs', {
      secret: state.clashApiSecret,
      unix: state.clashApiUnix,
    })
      .then((data) => data.mode && setMode(data.mode))
      .catch(() => showToast('Не удалось получить режим маршрутизации', 'error'))
  }, [isReady, showToast, state.clashApiPort, state.clashApiSecret, state.clashApiUnix])

  if (!isReady) {
    return <RuntimeUnavailable title="Маршрутизация недоступна" description="Запустите ядро Mihomo, чтобы выбрать маршрут и проверить задержку." />
  }

  return (
    <section className="dhq-workspace-card">
      <SelectorsPanel
        clashApiPort={state.clashApiPort ?? ''}
        mode={mode}
        clashApiSecret={state.clashApiSecret}
        clashApiUnix={state.clashApiUnix}
      />
    </section>
  )
}

export function DevicesView() {
  const { state } = useAppContext()
  const isReady = state.currentCore === 'mihomo' && state.serviceStatus === 'running' && !!(state.clashApiPort || state.clashApiUnix)
  if (!isReady) {
    return <RuntimeUnavailable title="Нет активных устройств" description="Соединения появятся после запуска ядра Mihomo." />
  }
  return (
    <section className="dhq-workspace-card">
      <ConnectionsPanel clashApiPort={state.clashApiPort ?? ''} clashApiSecret={state.clashApiSecret} clashApiUnix={state.clashApiUnix} />
    </section>
  )
}

export function DiagnosticsView() {
  const { state } = useAppContext()
  const wsConnected = useWsConnected()
  const checks = [
    { name: 'Панель управления', ok: true, detail: state.version ? `DHQClash Router ${state.version}` : 'Панель отвечает' },
    { name: 'Ядро маршрутизации', ok: state.serviceStatus === 'running', detail: state.serviceStatus === 'running' ? `${state.currentCore} работает` : 'Ядро остановлено' },
    { name: 'Clash API', ok: state.currentCore === 'mihomo' && !!(state.clashApiPort || state.clashApiUnix), detail: state.clashApiUnix ? 'Подключено через Unix socket' : state.clashApiPort ? `Порт ${state.clashApiPort}` : 'Интерфейс не найден' },
    { name: 'Поток соединений', ok: wsConnected, detail: wsConnected ? 'Данные обновляются в реальном времени' : 'Ожидание данных' },
  ]

  return (
    <div className="grid gap-5">
      <section className="dhq-diagnostics-grid">
        {checks.map((check) => (
          <article key={check.name} className={cn('dhq-check-card', check.ok && 'dhq-check-card--ok')}>
            <span>{check.ok ? <IconCheck /> : <IconActivityHeartbeat />}</span>
            <div><strong>{check.name}</strong><small>{check.detail}</small></div>
          </article>
        ))}
      </section>
      <section className="dhq-diagnostic-log">
        <div className="dhq-card-heading">
          <div><span className="dhq-eyebrow">Диагностика</span><h2>Журнал работы</h2></div>
          <IconServer className="text-[var(--dhq-cyan)]" />
        </div>
        <LogPanel />
      </section>
    </div>
  )
}

function RuntimeUnavailable({ title, description }: { title: string; description: string }) {
  return (
    <div className="dhq-empty-state">
      <span><IconActivityHeartbeat /></span>
      <h2>{title}</h2>
      <p>{description}</p>
    </div>
  )
}
