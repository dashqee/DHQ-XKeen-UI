import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { IconAlertTriangle, IconRefresh, IconSearch, IconStack2 } from '@tabler/icons-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { apiCall, clashFetch } from '../../lib/api'
import { useAppContext } from '../../lib/store'

interface RuleProvider {
  name: string
  type: string
  vehicleType: string
  ruleCount?: number
  format?: string
  behavior?: string
  updatedAt?: string
}

interface SearchHit {
  name: string
  matches: string[]
  matchCount: number
  truncated: boolean
  error?: string
}

/** A geosite set runs to hundreds of thousands of lines. Putting all of them in
 *  the DOM freezes the tab, and nobody scrolls that far — the rest is one click
 *  away in the config anyway. */
const MAX_RENDERED_LINES = 2000
const SEARCH_DEBOUNCE_MS = 300

const FORMAT_LABELS: Record<string, string> = {
  MrsRule: 'MRS',
  YamlRule: 'YAML',
  TextRule: 'TEXT',
}

function formatDateTime(value?: string | number) {
  if (!value) return '—'
  const date = new Date(typeof value === 'number' ? value * 1000 : value)
  if (Number.isNaN(date.getTime())) return '—'
  return date.toLocaleString('ru-RU', {
    day: '2-digit',
    month: '2-digit',
    year: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/** Splits a line around every case-insensitive occurrence of the query, so the
 *  match can be marked without an innerHTML round trip. */
function highlight(line: string, query: string) {
  if (!query) return line
  const parts: Array<string | { hit: string }> = []
  const haystack = line.toLowerCase()
  const needle = query.toLowerCase()
  let from = 0
  for (;;) {
    const at = haystack.indexOf(needle, from)
    if (at === -1) break
    if (at > from) parts.push(line.slice(from, at))
    parts.push({ hit: line.slice(at, at + needle.length) })
    from = at + needle.length
  }
  parts.push(line.slice(from))
  return parts.map((part, index) =>
    typeof part === 'string' ? (
      part
    ) : (
      <mark key={index} className="bg-chart-2/30 text-foreground rounded-[3px] px-0.5">
        {part.hit}
      </mark>
    )
  )
}

function Lines({ lines, query }: { lines: string[]; query: string }) {
  const shown = lines.slice(0, MAX_RENDERED_LINES)
  return (
    <>
      <pre className="scrollbar-thin bg-input-background max-h-96 overflow-auto rounded-lg p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap">
        {shown.map((line, index) => (
          <div key={index}>{highlight(line, query)}</div>
        ))}
      </pre>
      {lines.length > shown.length && (
        <p className="text-muted-foreground mt-2 text-xs">
          Показаны первые {shown.length} из {lines.length} строк
        </p>
      )}
    </>
  )
}

export function RulesView() {
  const { state, showToast } = useAppContext()
  const isReady = state.currentCore === 'mihomo' && state.serviceStatus === 'running' && !!(state.clashApiPort || state.clashApiUnix)

  const [providers, setProviders] = useState<RuleProvider[]>([])
  const [loading, setLoading] = useState(true)
  const [query, setQuery] = useState('')
  const [applied, setApplied] = useState('')
  const [searching, setSearching] = useState(false)
  const [hits, setHits] = useState<SearchHit[] | null>(null)
  const [contents, setContents] = useState<Record<string, string>>({})
  const [loadingName, setLoadingName] = useState('')
  const [open, setOpen] = useState<string[]>([])

  const loadProviders = useCallback(async () => {
    if (!state.clashApiPort && !state.clashApiUnix) return
    setLoading(true)
    try {
      const data = await clashFetch<{ providers?: Record<string, RuleProvider> }>(state.clashApiPort ?? '', 'providers/rules', {
        secret: state.clashApiSecret,
        unix: state.clashApiUnix ?? null,
      })
      setProviders(Object.values(data.providers ?? {}))
    } catch {
      showToast('Не удалось загрузить наборы правил', 'error')
    } finally {
      setLoading(false)
    }
  }, [showToast, state.clashApiPort, state.clashApiSecret, state.clashApiUnix])

  useEffect(() => {
    if (!isReady) return
    void loadProviders()
  }, [isReady, loadProviders])

  // Debounced so a typed domain is one search, not one per keystroke: each of
  // them walks every set on the router.
  useEffect(() => {
    const trimmed = query.trim()
    if (!trimmed) {
      setApplied('')
      setHits(null)
      setSearching(false)
      return
    }
    setSearching(true)
    const timer = setTimeout(async () => {
      try {
        const res = await apiCall<{ success: boolean; error?: string; results?: SearchHit[] }>(
          'GET',
          `ruleset/search?q=${encodeURIComponent(trimmed)}`
        )
        if (!res.success) throw new Error(res.error ?? 'Нет данных')
        setHits(res.results ?? [])
        setApplied(trimmed)
      } catch (e) {
        setHits([])
        showToast(`Поиск не удался: ${e instanceof Error ? e.message : 'неизвестная ошибка'}`, 'error')
      } finally {
        setSearching(false)
      }
    }, SEARCH_DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [query, showToast])

  const byName = useMemo(() => new Map(providers.map((provider) => [provider.name, provider])), [providers])

  const loadContent = useCallback(
    async (name: string) => {
      if (contents[name] !== undefined) return
      const provider = byName.get(name)
      if (!provider) return
      setLoadingName(name)
      try {
        const params = new URLSearchParams({ name })
        if (provider.format) params.set('format', provider.format)
        if (provider.behavior) params.set('behavior', provider.behavior)
        if (provider.vehicleType) params.set('vehicleType', provider.vehicleType)
        const res = await apiCall<{ success: boolean; error?: string; content?: string }>('GET', `ruleset?${params.toString()}`)
        if (!res.success || res.content === undefined) throw new Error(res.error ?? 'Нет данных')
        setContents((prev) => ({ ...prev, [name]: res.content! }))
      } catch (e) {
        showToast(`Не удалось загрузить «${name}»: ${e instanceof Error ? e.message : 'неизвестная ошибка'}`, 'error')
        setOpen((prev) => prev.filter((item) => item !== name))
      } finally {
        setLoadingName('')
      }
    },
    [byName, contents, showToast]
  )

  const onOpenChange = useCallback(
    (next: string[]) => {
      setOpen(next)
      // Only the freshly opened ones need fetching; the rest are cached already.
      for (const name of next) void loadContent(name)
    },
    [loadContent]
  )

  const searchRef = useRef<HTMLInputElement>(null)

  if (!isReady) {
    return (
      <div className="dhq-empty-state">
        <span>
          <IconStack2 />
        </span>
        <h2>Правила недоступны</h2>
        <p>Запустите ядро Mihomo, чтобы посмотреть наборы правил.</p>
      </div>
    )
  }

  const searching_ = searching && query.trim() !== applied
  const rows = hits
    ? hits.map((hit) => ({ hit, provider: byName.get(hit.name) }))
    : providers.map((provider) => ({ hit: null as SearchHit | null, provider }))

  return (
    <section className="dhq-workspace-card flex flex-col gap-4">
      <header className="flex flex-wrap items-center gap-3">
        <div className="relative min-w-60 flex-1">
          <IconSearch size={16} className="text-muted-foreground pointer-events-none absolute top-1/2 left-3 -translate-y-1/2" />
          <Input
            ref={searchRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Найти домен или адрес во всех наборах"
            className="pl-9"
            aria-label="Поиск по наборам правил"
          />
        </div>
        <Button variant="outline" onClick={() => void loadProviders()} disabled={loading}>
          <IconRefresh size={16} />
          Обновить
        </Button>
      </header>

      {applied && hits && (
        <p className="text-muted-foreground text-sm">
          {hits.length === 0
            ? `Ничего не найдено по «${applied}»`
            : `Найдено в ${hits.length} ${hits.length === 1 ? 'наборе' : 'наборах'} по «${applied}»`}
        </p>
      )}

      {loading ? (
        <div className="flex flex-col gap-2">
          {[0, 1, 2].map((index) => (
            <Skeleton key={index} className="h-14 w-full rounded-xl" />
          ))}
        </div>
      ) : searching_ && !hits ? (
        <div className="flex items-center gap-2 py-8">
          <Spinner /> <span className="text-muted-foreground text-sm">Ищем по наборам…</span>
        </div>
      ) : rows.length === 0 ? (
        <Empty className="border-none">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <IconStack2 className="size-8" />
            </EmptyMedia>
            <EmptyTitle>{applied ? 'Совпадений нет' : 'Наборы правил не настроены'}</EmptyTitle>
          </EmptyHeader>
        </Empty>
      ) : (
        <Accordion type="multiple" value={applied ? rows.map((row) => row.hit!.name) : open} onValueChange={onOpenChange} className="flex flex-col gap-2">
          {rows.map(({ hit, provider }) => {
            const name = hit?.name ?? provider!.name
            const content = contents[name]
            return (
              <AccordionItem key={name} value={name} className="border-border bg-input-background rounded-xl border px-4">
                <AccordionTrigger className="gap-3 hover:no-underline">
                  <span className="flex min-w-0 flex-1 flex-wrap items-center gap-2 text-left">
                    <span className="truncate font-medium">{name}</span>
                    {hit && !hit.error && (
                      <Badge variant="secondary">
                        {hit.matchCount} {hit.matchCount === 1 ? 'совпадение' : 'совпадений'}
                      </Badge>
                    )}
                    {provider?.format && <Badge variant="outline">{FORMAT_LABELS[provider.format] ?? provider.format}</Badge>}
                    {provider?.behavior && <Badge variant="outline">{provider.behavior}</Badge>}
                    {typeof provider?.ruleCount === 'number' && <Badge variant="outline">{provider.ruleCount} правил</Badge>}
                    <span className="text-muted-foreground text-xs">{formatDateTime(provider?.updatedAt)}</span>
                  </span>
                </AccordionTrigger>
                <AccordionContent className="pb-4">
                  {hit?.error ? (
                    <p className="text-destructive flex items-center gap-2 text-sm">
                      <IconAlertTriangle size={16} />
                      {hit.error}
                    </p>
                  ) : hit ? (
                    <>
                      <Lines lines={hit.matches} query={applied} />
                      {hit.truncated && (
                        <p className="text-muted-foreground mt-2 text-xs">
                          Показаны первые {hit.matches.length} совпадений из {hit.matchCount}
                        </p>
                      )}
                    </>
                  ) : loadingName === name || content === undefined ? (
                    <div className="flex items-center gap-2 py-2">
                      <Spinner /> <span className="text-muted-foreground text-sm">Загружаем содержимое…</span>
                    </div>
                  ) : (
                    <Lines lines={content.split('\n')} query="" />
                  )}
                </AccordionContent>
              </AccordionItem>
            )
          })}
        </Accordion>
      )}
    </section>
  )
}
