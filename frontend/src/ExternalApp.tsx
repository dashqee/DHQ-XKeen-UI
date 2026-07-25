import {
  IconActivity,
  IconBook2,
  IconBrandSpeedtest,
  IconKey,
  IconListTree,
  IconPlugConnected,
  IconRefresh,
  IconRoute,
  IconShieldCheck,
  IconWorld,
} from '@tabler/icons-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { BrandMark } from './components/brand/BrandMark'
import { RoutingView } from './components/dashboard/RouterViews'
import { RouterShell, type ShellNavigationItem } from './components/layout/RouterShell'
import { ConnectionsPanel } from './components/configuration/mihomo/Connections'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Spinner } from './components/ui/spinner'
import { Toast } from './components/ui/toast'
import { clashFetch } from './lib/api'
import {
  fetchClashProxies,
  useAppActions,
  useConnections,
  useConnectionsSync,
  useProxiesStore,
} from './lib/store'
import { cn } from './lib/utils'

const SECRET_KEY = 'dhqclash-external-ui-secret'

interface MihomoVersion {
  version?: string
  meta?: boolean
}

function ExternalConnect({ onConnected }: { onConnected: (secret: string, version: string) => void }) {
  const [secret, setSecret] = useState(() => sessionStorage.getItem(SECRET_KEY) ?? '')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const connect = useCallback(async () => {
    setBusy(true)
    setError('')
    try {
      const data = await clashFetch<MihomoVersion>('external', 'version', { secret, retry: false })
      sessionStorage.setItem(SECRET_KEY, secret)
      onConnected(secret, data.version ?? 'Mihomo')
    } catch {
      setError('Не удалось подключиться. Проверьте ключ API в конфигурации Mihomo.')
    } finally {
      setBusy(false)
    }
  }, [onConnected, secret])

  useEffect(() => {
    void connect()
  }, [])

  return (
    <div className="dhq-external-gate">
      <section className="dhq-external-gate__card">
        <BrandMark className="size-20" />
        <span className="dhq-eyebrow">Mihomo external-ui</span>
        <h1>DHQClash Router</h1>
        <p>Панель подключится к Mihomo на этом устройстве. Ключ остаётся только в текущей вкладке браузера.</p>
        <label>
          <span>Ключ API</span>
          <div className="relative">
            <IconKey className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-[var(--dhq-muted)]" />
            <Input
              type="password"
              value={secret}
              className="h-12 pl-10"
              placeholder="secret из config.yaml"
              autoComplete="current-password"
              onChange={(event) => setSecret(event.target.value)}
              onKeyDown={(event) => event.key === 'Enter' && void connect()}
            />
          </div>
        </label>
        {error && <div className="dhq-external-error">{error}</div>}
        <Button className="dhq-primary-action h-12" disabled={busy} onClick={() => void connect()}>
          {busy ? <Spinner /> : <IconPlugConnected />}
          Подключиться
        </Button>
      </section>
    </div>
  )
}

function ExternalOverview({ version, onNavigate }: { version: string; onNavigate: (section: string) => void }) {
  const connections = useConnections()
  const proxies = useProxiesStore((state) => state.proxies)
  const selector = useMemo(
    () => Object.values(proxies).find((proxy: any) => proxy?.type === 'Selector' && proxy.now) as any,
    [proxies]
  )
  const totals = useMemo(
    () => connections.reduce((sum, connection) => sum + connection.upload + connection.download, 0),
    [connections]
  )
  const sources = new Set(connections.map((connection) => connection.metadata.sourceIP)).size

  return (
    <div className="dhq-external-overview">
      <section className="dhq-external-status">
        <div>
          <span className="dhq-eyebrow">Защита активна</span>
          <h2><IconShieldCheck /> Подключено</h2>
          <p>Mihomo принимает соединения и применяет правила маршрутизации.</p>
        </div>
        <div className="dhq-external-status__mark"><BrandMark className="size-28" /></div>
      </section>
      <section className="dhq-metrics-grid">
        <article className="dhq-metric-card"><span className="dhq-metric-icon"><IconRoute /></span><div><small>Текущий маршрут</small><strong>{selector?.now ?? 'Авто'}</strong></div><em>{selector?.name ?? 'Основной селектор'}</em></article>
        <article className="dhq-metric-card"><span className="dhq-metric-icon"><IconListTree /></span><div><small>Соединения</small><strong>{connections.length}</strong></div><em>{sources} устройств</em></article>
        <article className="dhq-metric-card"><span className="dhq-metric-icon"><IconActivity /></span><div><small>Трафик сессии</small><strong>{formatBytes(totals)}</strong></div><em>в активных соединениях</em></article>
        <article className="dhq-metric-card"><span className="dhq-metric-icon"><IconBrandSpeedtest /></span><div><small>Ядро</small><strong>Mihomo</strong></div><em>{version}</em></article>
      </section>
      <section className="dhq-external-actions">
        <button type="button" onClick={() => onNavigate('routing')}><IconRoute /><span><strong>Маршрутизация</strong><small>Выбрать узел и проверить задержку</small></span></button>
        <button type="button" onClick={() => onNavigate('connections')}><IconListTree /><span><strong>Соединения</strong><small>Посмотреть активный трафик</small></span></button>
        <button type="button" onClick={() => onNavigate('rules')}><IconBook2 /><span><strong>Правила</strong><small>Проверить логику маршрутов</small></span></button>
      </section>
    </div>
  )
}

function ExternalConnections({ secret }: { secret: string }) {
  return (
    <section className="dhq-workspace-card">
      <ConnectionsPanel clashApiPort="external" clashApiSecret={secret} />
    </section>
  )
}

function ExternalRules({ secret }: { secret: string }) {
  const [rules, setRules] = useState<Array<{ type?: string; payload?: string; proxy?: string }>>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const data = await clashFetch<{ rules?: Array<{ type?: string; payload?: string; proxy?: string }> }>('external', 'rules', { secret })
      setRules(data.rules ?? [])
    } catch {
      setError('Не удалось загрузить правила.')
    } finally {
      setLoading(false)
    }
  }, [secret])

  useEffect(() => {
    void load()
  }, [load])

  return (
    <section className="dhq-external-list">
      <div className="dhq-card-heading">
        <div><span className="dhq-eyebrow">Mihomo</span><h2>Правила маршрутизации</h2></div>
        <Button variant="outline" size="icon" onClick={() => void load()} aria-label="Обновить"><IconRefresh /></Button>
      </div>
      {loading ? <div className="dhq-external-loading"><Spinner /> Загружаем правила…</div> : error ? <div className="dhq-external-error">{error}</div> : (
        <div className="dhq-rule-list">
          {rules.map((rule, index) => (
            <article key={`${rule.type}-${rule.payload}-${index}`}>
              <span>{rule.type || 'MATCH'}</span>
              <strong>{rule.payload || 'Все соединения'}</strong>
              <em>{rule.proxy || 'DIRECT'}</em>
            </article>
          ))}
        </div>
      )}
    </section>
  )
}

function ExternalLogs({ secret }: { secret: string }) {
  const [lines, setLines] = useState<Array<{ type: string; payload: string }>>([])
  const [connected, setConnected] = useState(false)
  const bottomRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const protocol = location.protocol === 'https:' ? 'wss' : 'ws'
    const params = new URLSearchParams({ level: 'info' })
    if (secret) params.set('token', secret)
    const socket = new WebSocket(`${protocol}://${location.host}/logs?${params}`)
    socket.onopen = () => setConnected(true)
    socket.onclose = () => setConnected(false)
    socket.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as { type?: string; payload?: string }
        setLines((current) => [...current.slice(-499), { type: data.type ?? 'info', payload: data.payload ?? event.data }])
      } catch {
        setLines((current) => [...current.slice(-499), { type: 'info', payload: event.data }])
      }
    }
    return () => socket.close()
  }, [secret])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'end' })
  }, [lines])

  return (
    <section className="dhq-external-list">
      <div className="dhq-card-heading">
        <div><span className="dhq-eyebrow">В реальном времени</span><h2>Журнал Mihomo</h2></div>
        <span className={cn('dhq-external-live', connected && 'dhq-external-live--connected')}><i /> {connected ? 'Подключено' : 'Ожидание'}</span>
      </div>
      <div className="dhq-external-log">
        {lines.length === 0 && <div className="dhq-external-loading">Ожидаем события ядра…</div>}
        {lines.map((line, index) => <div key={index}><span>{line.type}</span><code>{line.payload}</code></div>)}
        <div ref={bottomRef} />
      </div>
    </section>
  )
}

function formatBytes(value: number): string {
  if (!value) return '0 Б'
  const units = ['Б', 'КБ', 'МБ', 'ГБ']
  const unit = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1)
  return `${(value / 1024 ** unit).toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`
}

export default function ExternalApp() {
  const { dispatch } = useAppActions()
  const [connection, setConnection] = useState<{ secret: string; version: string } | null>(null)
  const [section, setSection] = useState('overview')

  const onConnected = useCallback((secret: string, version: string) => {
    setConnection({ secret, version })
    dispatch({ type: 'SET_CORE_INFO', currentCore: 'mihomo', coreVersions: { mihomo: version }, availableCores: ['mihomo'] })
    dispatch({ type: 'SET_SERVICE_STATUS', status: 'running' })
    dispatch({ type: 'SET_DASHBOARD_PORT', port: 'external', secret })
    void fetchClashProxies('external', secret)
  }, [dispatch])

  useConnectionsSync(connection ? 'external' : null, connection?.secret, connection ? 'running' : 'stopped')

  if (!connection) return <><ExternalConnect onConnected={onConnected} /><Toast /></>

  const navigation: ShellNavigationItem[] = [
    { id: 'overview', label: 'Обзор', description: 'Состояние ядра', icon: IconWorld },
    { id: 'routing', label: 'Маршрутизация', description: 'Селекторы и задержка', icon: IconRoute },
    { id: 'connections', label: 'Соединения', description: 'Активный трафик', icon: IconListTree },
    { id: 'rules', label: 'Правила', description: 'Логика маршрутов', icon: IconBook2 },
  ]
  const titles: Record<string, string> = { overview: 'Обзор', routing: 'Маршрутизация', connections: 'Соединения', rules: 'Правила', logs: 'Логи' }

  return (
    <>
      <RouterShell
        activeItem={section}
        onNavigate={setSection}
        navigation={navigation}
        secondaryNavigation={[{ id: 'logs', label: 'Логи', description: 'События Mihomo', icon: IconActivity }]}
        title={titles[section] ?? 'DHQClash Router'}
        eyebrow="Mihomo external-ui"
        status="running"
        onLogout={() => {
          sessionStorage.removeItem(SECRET_KEY)
          setConnection(null)
        }}
      >
        {section === 'routing' ? <RoutingView /> :
          section === 'connections' ? <ExternalConnections secret={connection.secret} /> :
            section === 'rules' ? <ExternalRules secret={connection.secret} /> :
              section === 'logs' ? <ExternalLogs secret={connection.secret} /> :
                <ExternalOverview version={connection.version} onNavigate={setSection} />}
      </RouterShell>
      <Toast />
    </>
  )
}
