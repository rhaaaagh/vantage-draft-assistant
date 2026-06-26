import React from 'react'
import { useTranslation } from 'react-i18next'
import './AppShell.css'
import {
  IconDraft,
  IconScout,
  IconProfile,
  IconTier,
  IconChampion,
  IconCrawler,
  IconSettings,
} from './icons'
import { PatchSelector } from './patch'

export type TabId = 'draft' | 'scout' | 'profile' | 'tier' | 'champion' | 'crawler' | 'settings'

interface NavItem {
  id: TabId
  /** Ключ перевода (i18n), напр. "nav.scout". */
  labelKey: string
  icon: React.FC
}

const NAV: NavItem[] = [
  { id: 'draft', labelKey: 'nav.draft', icon: IconDraft },
  { id: 'scout', labelKey: 'nav.scout', icon: IconScout },
  { id: 'profile', labelKey: 'nav.profile', icon: IconProfile },
  { id: 'tier', labelKey: 'nav.tier', icon: IconTier },
  { id: 'champion', labelKey: 'nav.champion', icon: IconChampion },
  { id: 'crawler', labelKey: 'nav.crawler', icon: IconCrawler },
  { id: 'settings', labelKey: 'nav.settings', icon: IconSettings },
]

interface AppShellProps {
  active: TabId
  onNavigate: (id: TabId) => void
  children: React.ReactNode
  rail?: React.ReactNode
}

export const AppShell: React.FC<AppShellProps> = ({
  active,
  onNavigate,
  children,
  rail,
}) => {
  const { t } = useTranslation()
  return (
    <div className="shell">
      <div className="shell__brand">
        <span className="shell__brand-mark" />
        <span className="shell__brand-name">{t('app.name')}</span>
      </div>

      <header className="shell__header">
        <PatchSelector />
      </header>

      <nav className="shell__nav">
        {NAV.map((it) => {
          const Icon = it.icon
          const label = t(it.labelKey)
          return (
            <button
              key={it.id}
              type="button"
              title={label}
              className={`shell__nav-item${it.id === active ? ' is-active' : ''}`}
              onClick={() => onNavigate(it.id)}
            >
              <Icon />
              <span>{label}</span>
            </button>
          )
        })}
      </nav>

      <div className="shell__body">
        <div className={`shell__layout${rail ? ' shell__layout--rail' : ''}`}>
          <div className="shell__content">{children}</div>
          {rail && <aside className="shell__rail">{rail}</aside>}
        </div>
      </div>
    </div>
  )
}
