import { AnimatePresence, motion } from 'framer-motion'
import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { LoginForm } from './components/auth/Login'
import type { CodeMirrorRef } from './components/configuration/CodeMirror'
import { ConfigPanel } from './components/configuration/ConfigPanel'
import { DevicesView, DiagnosticsView, RouterDashboard, RoutingView } from './components/dashboard/RouterViews'
import { RouterShell, type ShellNavigationItem } from './components/layout/RouterShell'
import { Toast } from './components/ui/toast'
import { apiCall } from './lib/api'
import { LazyBoundary, lazyLoad, useLazyMount } from './lib/loader'
import {
  fetchClashProxies,
  getAppState,
  useAppActions,
  useAppContext,
  useConnectionsSync,
  useModalContext,
  useSettings,
} from './lib/store'
import { applyTheme, THEME_MEDIA_QUERY } from './lib/theme'
import { DEFAULT_PING_TEST_TIMEOUT, DEFAULT_PING_TEST_URL, type Config, type ThemeMode } from './lib/types'
import { parseClashApiCredentials } from './lib/utils'

const UpdateModal = lazyLoad(() => import('./components/modals/Update'), 'UpdateModal')
const ImportModal = lazyLoad(() => import('./components/modals/AddProxy'), 'ImportModal')
const TemplateModal = lazyLoad(() => import('./components/modals/Templates'), 'TemplateModal')
const SettingsModal = lazyLoad(() => import('./components/modals/Settings'), 'SettingsModal')

function useThemeMode(theme: ThemeMode) {
  useEffect(() => {
    applyTheme(theme)
    if (theme !== 'auto') return

    const media = window.matchMedia(THEME_MEDIA_QUERY)
    const syncTheme = () => applyTheme('auto')

    media.addEventListener('change', syncTheme)
    return () => media.removeEventListener('change', syncTheme)
  }, [theme])
}

interface ModalManagerProps {
  onInstalled: () => void
  onGenerate: (uri: string) => { content: string; type: string } | null
  onAddToConfig: (content: string, type: string, position: 'start' | 'end') => void
  onImportTemplate: (url: string) => Promise<void>
}

const ModalManager = memo(function ModalManager({
  onInstalled,
  onGenerate,
  onAddToConfig,
  onImportTemplate,
}: ModalManagerProps) {
  const { modals } = useModalContext()

  const mountUpdate = useLazyMount(modals.showUpdateModal)
  const mountImport = useLazyMount(modals.showImportModal)
  const mountTemplate = useLazyMount(modals.showTemplateModal)
  const mountSettings = useLazyMount(modals.showSettingsModal)

  return (
    <>
      {mountUpdate && (
        <LazyBoundary>
          <UpdateModal onInstalled={onInstalled} />
        </LazyBoundary>
      )}
      {mountImport && (
        <LazyBoundary>
          <ImportModal onGenerate={onGenerate} onAddToConfig={onAddToConfig} />
        </LazyBoundary>
      )}
      {mountTemplate && (
        <LazyBoundary>
          <TemplateModal onImport={onImportTemplate} />
        </LazyBoundary>
      )}
      {mountSettings && (
        <LazyBoundary>
          <SettingsModal />
        </LazyBoundary>
      )}
    </>
  )
})

function AppContent({ onLogout }: { onLogout: () => void }) {
  const { dispatch, showToast } = useAppActions()
  const { state } = useAppContext()
  const [activeSection, setActiveSection] = useState('home')
  const editorRef = useRef<CodeMirrorRef | null>(null)
  const configActionsRef = useRef<{ switchTab: (index: number) => void; getActiveIndex: () => number }>({
    switchTab: () => { },
    getActiveIndex: () => 0,
  })

  const checkStatus = useCallback(async () => {
    const data = await apiCall<any>('GET', 'control')
    if (!data.success) return null
    const currentCore = data.currentCore || 'mihomo'
    dispatch({
      type: 'SET_CORE_INFO',
      currentCore,
      coreVersions: getAppState().coreVersions,
      availableCores: data.cores || [],
    })
    dispatch({ type: 'SET_SERVICE_STATUS', status: data.running ? 'running' : 'stopped' })
    return currentCore
  }, [dispatch])

  const loadConfigs = useCallback(
    async (core?: string, skipProxies = false, silent = false): Promise<Config[]> => {
      if (!silent) dispatch({ type: 'SET_CONFIGS_LOADING', loading: true })
      try {
        const result = await apiCall<any>('GET', core ? `configs?core=${core}` : 'configs')
        if (result.success && result.configs) {
          const configs: Config[] = result.configs.map((c: any) => ({ ...c, savedContent: c.content, isDirty: false }))
          dispatch({ type: 'SET_CONFIGS', configs })
          const yamlConfig = configs.find((c: any) => c.file.endsWith('/config.yaml') || c.file === 'config.yaml')
          const { port, secret, unix } = yamlConfig
            ? parseClashApiCredentials(yamlConfig.content)
            : { port: null, secret: null, unix: null }
          dispatch({ type: 'SET_DASHBOARD_PORT', port, secret, unix } as any)
          const appState = getAppState()
          const activeCores = core ?? appState.currentCore
          if ((port || unix) && activeCores === 'mihomo' && !skipProxies && appState.serviceStatus === 'running') {
            fetchClashProxies(port ?? '', secret, false, unix)
          }
          return configs
        } else {
          if (!silent) dispatch({ type: 'SET_CONFIGS_LOADING', loading: false })
          showToast('Не удалось загрузить конфигурации', 'error')
        }
      } catch (e: any) {
        if (!silent) dispatch({ type: 'SET_CONFIGS_LOADING', loading: false })
        showToast(`${e.message}`, 'error')
      }
      return []
    },
    [dispatch, showToast]
  )

  const checkVersion = useCallback(
    async (showUpdateToast = false) => {
      try {
        const data = await apiCall<any>('GET', 'version')
        if (!data.success) return

        const ui = data['xkeen-ui']
        if (!ui) return

        let isOutdatedCore = false
        const coreVersions: Record<string, string> = {}
        if (data.mihomo?.version) {
          coreVersions.mihomo = data.mihomo.version
          if (data.mihomo.outdated) isOutdatedCore = true
        }

        dispatch({
          type: 'SET_VERSION',
          version: ui.version,
          isOutdatedUI: !!ui.outdated,
          isOutdatedCore,
        })

        if (Object.keys(coreVersions).length > 0) {
          const appState = getAppState()
          dispatch({
            type: 'SET_CORE_INFO',
            currentCore: appState.currentCore,
            coreVersions: { ...appState.coreVersions, ...coreVersions },
            availableCores: appState.availableCores,
          })
        }

        if (!showUpdateToast) return
        if (ui.show_toast) showToast({ title: 'Доступно обновление', body: 'Доступна новая версия DHQClash Router', persistent: true, id: 'update-ui', ...(ui.link && { action: { url: ui.link } }) })

        const entry = data.mihomo
        if (entry?.show_toast) {
          showToast({
            title: 'Доступно обновление',
            body: 'Доступна новая версия Mihomo',
            persistent: true,
            id: 'update-mihomo',
            ...(entry.link ? { action: { url: entry.link } } : {}),
          })
        }
      } catch {
        /* ignore */
      }
    },
    [dispatch, showToast]
  )

  useEffect(() => {
    const loadSettings = async () => {
      const data = await apiCall<any>('GET', 'settings')
      if (data.success)
        dispatch({
          type: 'SET_SETTINGS',
          settings: {
            autoCheckUI: data.updater.auto_check_ui ?? true,
            autoCheckCore: data.updater.auto_check_core ?? true,
            backupCore: data.updater.backup_core,
            githubProxies: data.updater.github_proxy || [],
            pingTestUrl: data.clash_api?.ping_url ?? DEFAULT_PING_TEST_URL,
            pingTestTimeout: data.clash_api?.ping_timeout ?? DEFAULT_PING_TEST_TIMEOUT,
            showSourceName: data.clash_api?.show_source_name ?? false,
            hideUnavailableProxies: data.clash_api?.hide_unavailable_proxies ?? false,
            hideUnavailableProxiesCounter: data.clash_api?.hide_unavailable_proxies_counter ?? 3,
            proxySortOrder: data.clash_api?.proxy_sort_order ?? 'default',
            timezone: data.log.timezone,
            authEnabled: !!data.auth?.enabled,
          },
        })
    }

    const init = async () => {
      try {
        await loadSettings()
        const currentCore = await checkStatus()
        if (currentCore) loadConfigs(currentCore)
        checkVersion(true)
      } catch {
        showToast('Ошибка инициализации', 'error')
      }
    }

    init()
  }, [checkStatus, loadConfigs, dispatch, showToast, checkVersion])

  const generateConfig = useCallback((uri: string) => {
    if (typeof (window as any).generateMihomoConfig === 'function')
      return (window as any).generateMihomoConfig(uri, editorRef.current?.getValue() ?? '')
    throw new Error('Parser not loaded')
  }, [])

  const importTemplate = useCallback(
    async (url: string) => {
      const activeIndex = configActionsRef.current.getActiveIndex()
      const active = getAppState().configs[activeIndex]
      if (active?.isDirty && !confirm('Несохраненные изменения будут потеряны. Продолжить?')) throw new Error('Отменено')
      const res = await fetch(url)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const content = await res.text()
      if (editorRef.current) {
        editorRef.current.setValue(content)
        dispatch({ type: 'UPDATE_CONFIG_DIRTY', index: activeIndex, isDirty: true, content })
      }
      showToast('Шаблон импортирован')
    },
    [dispatch, showToast]
  )

  const addToConfig = useCallback(
    (generated: string, type: string, position: 'start' | 'end') => {
      const appState = getAppState()
      let targetIndex = configActionsRef.current.getActiveIndex()

      targetIndex = appState.configs.findIndex((c) => c.file.endsWith('/config.yaml') || c.file === 'config.yaml')
      if (targetIndex === -1) {
        showToast('Файл config.yaml не найден', 'error')
        return
      }

      if (targetIndex !== configActionsRef.current.getActiveIndex()) configActionsRef.current.switchTab(targetIndex)

      setTimeout(() => {
        const editorWrapper = editorRef.current
        if (!editorWrapper) return
        let current = editorWrapper.getValue()
        const lineAtOffset = (text: string, offset: number) => text.slice(0, Math.min(offset, text.length)).split('\n').length
        const scrollToLine = (line: number) => setTimeout(() => editorWrapper.revealLine(Math.max(1, line)), 0)
        const insertAtOffset = (offset: number, text: string, scrollLine?: number) => {
          editorWrapper.replaceRange(offset, offset, text)
          scrollToLine(scrollLine ?? editorWrapper.offsetToLineColumn(offset).lineNumber)
        }

        const marker = type === 'proxy' ? 'proxies:' : 'proxy-providers:'
        const markerRegex = new RegExp(`^${marker}(.*)$`, 'm')
        const markerMatch = current.match(markerRegex)

        if (!markerMatch) {
          const eofStr = current.endsWith('\n') ? '' : '\n'
          const insertPos = current.length
          const targetLine = lineAtOffset(current, insertPos) + (eofStr ? 2 : 1)
          insertAtOffset(insertPos, `${eofStr}${marker}\n${generated}\n`, targetLine)
          return
        }

        const markerIdx = markerMatch.index!
        let markerLineEnd = current.indexOf('\n', markerIdx)
        if (markerLineEnd === -1) markerLineEnd = current.length
        else markerLineEnd += 1

        const lineContent = markerMatch[1].trim()
        if (lineContent === '[]' || lineContent === 'null') {
          const pre = current.slice(0, markerIdx)
          const post = current.slice(markerLineEnd)
          current = pre + marker + '\n' + post
          editorWrapper.replaceAll(current)
          markerLineEnd = markerIdx + marker.length + 1
        }

        if (position === 'start') {
          const line = lineAtOffset(current, markerLineEnd)
          insertAtOffset(markerLineEnd, generated + '\n', line)
        } else {
          const afterMarker = markerLineEnd
          const nextKeyMatch = current.slice(afterMarker).search(/^[a-zA-Z0-9_-]+:/m)
          const insertOffset = nextKeyMatch === -1 ? current.length : afterMarker + nextKeyMatch
          let textToInsert = generated + '\n'

          let targetLine = lineAtOffset(current, insertOffset)

          if (nextKeyMatch === -1 && !current.endsWith('\n')) {
            textToInsert = '\n' + textToInsert
            targetLine += 1
          }

          insertAtOffset(insertOffset, textToInsert, targetLine)
        }
      }, 150)
    },
    [showToast]
  )

  const logout = onLogout
  const onInstalled = useCallback(() => void checkVersion(), [checkVersion])
  const openModal = useCallback(
    (modal: string) => setTimeout(() => dispatch({ type: 'SHOW_MODAL', modal: modal as any, show: true }), 0),
    [dispatch]
  )

  useConnectionsSync(
    activeSection !== 'configuration' && state.currentCore === 'mihomo' ? state.clashApiPort : null,
    state.clashApiSecret,
    state.serviceStatus,
    state.clashApiUnix
  )

  const navigation: ShellNavigationItem[] = [
    { id: 'home', label: 'Главная', description: 'Состояние защиты' },
    { id: 'routing', label: 'Маршрутизация', description: 'Текущий маршрут' },
    { id: 'devices', label: 'Устройства', description: 'Активные соединения' },
    { id: 'configuration', label: 'Конфигурация', description: 'Настройки Mihomo' },
    { id: 'diagnostics', label: 'Диагностика', description: 'Проверки и журнал' },
  ]

  const sectionTitles: Record<string, string> = {
    home: 'Главная',
    routing: 'Маршрутизация',
    devices: 'Устройства',
    configuration: 'Конфигурация',
    diagnostics: 'Диагностика',
  }

  const renderSection = () => {
    if (activeSection === 'routing') return <RoutingView />
    if (activeSection === 'devices') return <DevicesView />
    if (activeSection === 'diagnostics') {
      return (
        <DiagnosticsView
          onOpenUpdate={(target) => {
            dispatch({ type: 'SET_UPDATE_MODAL_CORE', core: target })
            openModal('showUpdateModal')
          }}
        />
      )
    }
    if (activeSection === 'configuration') {
      return (
        <div className="flex min-h-[calc(100dvh-10rem)] flex-col">
          <ConfigPanel
            editorRef={editorRef}
            configActionsRef={configActionsRef}
            onOpenImport={() => openModal('showImportModal')}
            onOpenTemplate={() => openModal('showTemplateModal')}
            onOpenBackups={() => openModal('showBackupsModal')}
            onRefreshConfigs={() => loadConfigs(undefined, false, true)}
          />
        </div>
      )
    }
    return <RouterDashboard onOpenRouting={() => setActiveSection('routing')} />
  }

  return (
    <>
      <RouterShell
        activeItem={activeSection}
        onNavigate={setActiveSection}
        navigation={navigation}
        title={sectionTitles[activeSection] ?? 'DHQClash Router'}
        status={state.serviceStatus === 'running' ? 'running' : state.serviceStatus === 'pending' || state.serviceStatus === 'loading' ? 'pending' : 'stopped'}
        onOpenSettings={() => openModal('showSettingsModal')}
        onLogout={logout}
      >
        {renderSection()}
      </RouterShell>
      <Toast />
      <ModalManager
        onInstalled={onInstalled}
        onGenerate={generateConfig}
        onAddToConfig={addToConfig}
        onImportTemplate={importTemplate}
      />
    </>
  )
}

type AuthState = 'loading' | 'login' | 'setup' | 'authenticated'

export default function App() {
  const [authState, setAuthState] = useState<AuthState>('loading')
  const theme = useSettings((s) => s.theme)

  useThemeMode(theme)

  useEffect(() => {
    fetch('/api/auth/login')
      .then((r) => r.json())
      .then((data) => {
        if (!data.enabled || data.authenticated) setAuthState('authenticated')
        else if (!data.has_password) setAuthState('setup')
        else setAuthState('login')
      })
      .catch(() => setAuthState('login'))
  }, [])

  const handleLogout = useCallback(async () => {
    await fetch('/api/auth/logout', { method: 'POST' })
    setAuthState('login')
  }, [])

  if (authState === 'loading') return null

  return (
    <AnimatePresence mode="wait">
      {authState === 'login' || authState === 'setup' ? (
        <motion.div key="login" initial={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.25 }}>
          <LoginForm mode={authState} onAuth={() => setAuthState('authenticated')} />
          <Toast />
        </motion.div>
      ) : (
        <motion.div key="app" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.25 }}>
          <AppContent onLogout={handleLogout} />
        </motion.div>
      )}
    </AnimatePresence>
  )
}
