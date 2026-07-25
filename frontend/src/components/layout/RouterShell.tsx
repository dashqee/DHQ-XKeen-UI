import {
  IconActivity,
  IconAdjustments,
  IconChevronRight,
  IconDeviceDesktopAnalytics,
  IconDevices,
  IconHome,
  IconListTree,
  IconLogout,
  IconRoute,
  IconSettings,
} from '@tabler/icons-react'
import type { ComponentType, ReactNode } from 'react'
import { BrandMark } from '../brand/BrandMark'
import { Button } from '../ui/button'
import { cn } from '../../lib/utils'

export interface ShellNavigationItem {
  id: string
  label: string
  description?: string
  icon?: ComponentType<{ className?: string; stroke?: number }>
}

const DEFAULT_ICONS: Record<string, ShellNavigationItem['icon']> = {
  home: IconHome,
  routing: IconRoute,
  devices: IconDevices,
  diagnostics: IconActivity,
  overview: IconDeviceDesktopAnalytics,
  connections: IconListTree,
}

interface RouterShellProps {
  activeItem: string
  onNavigate: (item: string) => void
  navigation: ShellNavigationItem[]
  secondaryNavigation?: ShellNavigationItem[]
  title: string
  eyebrow?: string
  status?: 'running' | 'stopped' | 'pending'
  statusLabel?: string
  onOpenSettings?: () => void
  onLogout?: () => void
  children: ReactNode
}

function NavigationButton({
  item,
  active,
  onClick,
  compact = false,
}: {
  item: ShellNavigationItem
  active: boolean
  onClick: () => void
  compact?: boolean
}) {
  const NavigationIcon = item.icon ?? DEFAULT_ICONS[item.id] ?? IconAdjustments

  return (
    <button
      type="button"
      className={cn('dhq-nav-item', active && 'dhq-nav-item--active', compact && 'dhq-nav-item--compact')}
      onClick={onClick}
    >
      <NavigationIcon className="size-5 shrink-0" stroke={1.8} />
      <span className="min-w-0 flex-1 text-left">
        <strong>{item.label}</strong>
        {!compact && item.description && <small>{item.description}</small>}
      </span>
      {!compact && <IconChevronRight className="size-4 opacity-40" />}
    </button>
  )
}

export function RouterShell({
  activeItem,
  onNavigate,
  navigation,
  secondaryNavigation = [],
  title,
  eyebrow = 'Управление роутером',
  status = 'stopped',
  statusLabel,
  onOpenSettings,
  onLogout,
  children,
}: RouterShellProps) {
  const resolvedStatus = statusLabel ?? (status === 'running' ? 'Подключено' : status === 'pending' ? 'Подключение…' : 'Отключено')

  return (
    <div className="dhq-shell">
      <aside className="dhq-sidebar">
        <div className="dhq-brand">
          <BrandMark className="size-11 shrink-0" />
          <div>
            <strong>DHQClash</strong>
            <span>Router</span>
          </div>
        </div>

        <div className={cn('dhq-router-state', status === 'running' && 'dhq-router-state--running')}>
          <span className="dhq-status-dot" />
          <div>
            <small>DHQClash Router</small>
            <strong>{resolvedStatus}</strong>
          </div>
        </div>

        <nav className="dhq-sidebar-nav" aria-label="Основная навигация">
          {navigation.map((item) => (
            <NavigationButton key={item.id} item={item} active={activeItem === item.id} onClick={() => onNavigate(item.id)} />
          ))}
        </nav>

        {secondaryNavigation.length > 0 && (
          <div className="dhq-sidebar-secondary">
            <span>Для специалиста</span>
            {secondaryNavigation.map((item) => (
              <NavigationButton key={item.id} item={item} active={activeItem === item.id} onClick={() => onNavigate(item.id)} />
            ))}
          </div>
        )}

        <div className="mt-auto grid gap-2">
          {onOpenSettings && (
            <Button variant="ghost" className="justify-start text-[var(--dhq-muted)]" onClick={onOpenSettings}>
              <IconSettings data-icon="inline-start" />
              Настройки
            </Button>
          )}
          {onLogout && (
            <Button variant="ghost" className="justify-start text-[var(--dhq-muted)]" onClick={onLogout}>
              <IconLogout data-icon="inline-start" />
              Выйти
            </Button>
          )}
        </div>
      </aside>

      <div className="dhq-content">
        <header className="dhq-topbar">
          <div className="dhq-mobile-brand">
            <BrandMark className="size-9" />
            <strong>DHQClash Router</strong>
          </div>
          <div className="dhq-page-title">
            <span>{eyebrow}</span>
            <h1>{title}</h1>
          </div>
          <div className={cn('dhq-topbar-status', status === 'running' && 'dhq-topbar-status--running')}>
            <span className="dhq-status-dot" />
            {resolvedStatus}
          </div>
          {onOpenSettings && (
            <Button variant="outline" size="icon" className="dhq-topbar-settings" onClick={onOpenSettings} aria-label="Настройки">
              <IconSettings />
            </Button>
          )}
        </header>

        <main className="dhq-main">{children}</main>
      </div>

      <nav className="dhq-bottom-nav" aria-label="Мобильная навигация">
        {navigation.slice(0, 4).map((item) => (
          <NavigationButton key={item.id} item={item} compact active={activeItem === item.id} onClick={() => onNavigate(item.id)} />
        ))}
      </nav>
    </div>
  )
}
