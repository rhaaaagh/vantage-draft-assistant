import './App.css'
import './theme/tokens.css' // импортируется ПОСЛЕ App.css — новые токены перебивают легаси
import { useEffect, useState } from 'react'
import type { DraftAnalysisResult } from './domain/draft'
import { applyWindowOverlay, setupWindowBoundsSaving } from './utils/windowOverlay'
import { AppShell } from './components/AppShell'
import type { TabId } from './components/AppShell'
import { RailProvider } from './components/rail'
import { PatchProvider } from './components/patch'
import { DraftScreen } from './views/DraftScreen'
import { Settings } from './views/Settings'
import { MetaTierList } from './views/MetaTierList'
import { ChampionPage } from './views/ChampionPage'
import type { ChampionRequest } from './views/ChampionPage'
import { Profile } from './views/Profile'
import { Scout } from './views/Scout'
import { Crawler } from './views/Crawler'

function App() {
  const [activeTab, setActiveTab] = useState<TabId>('profile')
  const [draftResult, setDraftResult] = useState<DraftAnalysisResult | null>(null)
  const [rail, setRail] = useState<React.ReactNode>(null)
  // Запрос открыть страницу чемпиона (напр. кликом из тир-листа). seq → повторный клик.
  const [championRequest, setChampionRequest] = useState<ChampionRequest | null>(null)

  const onOpenChampion = (id: number, name: string) => {
    setChampionRequest((prev) => ({ id, name, seq: (prev?.seq ?? 0) + 1 }))
    setActiveTab('champion')
  }

  // «Текущий аккаунт» — кого мы смотрим. Задаётся ТОЛЬКО сабмитом поиска.
  // Профиль/Скаут показывают его; смена вкладок данные не перезапрашивает.
  // seq инкрементится на КАЖДЫЙ сабмит, чтобы повторный поиск того же Riot ID
  // (напр. после 401) всё равно перезапускал загрузку.
  const [account, setAccount] = useState<{ query: string; seq: number } | null>(null)

  useEffect(() => {
    const id = setTimeout(() => {
      applyWindowOverlay().catch(() => {})
      setupWindowBoundsSaving().catch(() => {})
    }, 100)
    return () => clearTimeout(id)
  }, [])

  // Поиск живёт внутри вкладки Профиль. seq инкрементится на каждый сабмит,
  // чтобы повторный поиск того же Riot ID перезапускал загрузку. account хранится
  // здесь (в App), чтобы переживать смену вкладок.
  const onSearch = (query: string) => {
    const q = query.trim()
    if (!q) return
    setAccount((prev) => ({ query: q, seq: (prev?.seq ?? 0) + 1 }))
  }

  return (
    <PatchProvider>
    <RailProvider value={setRail}>
      <AppShell
        active={activeTab}
        onNavigate={setActiveTab}
        rail={rail}
      >
        {activeTab === 'draft' && (
          <DraftScreen draftResult={draftResult} setDraftResult={setDraftResult} />
        )}
        {activeTab === 'scout' && (
          <Scout
            onOpenProfile={(query) => {
              onSearch(query)
              setActiveTab('profile')
            }}
          />
        )}
        {activeTab === 'profile' && <Profile account={account} onSearch={onSearch} />}
        {activeTab === 'tier' && <MetaTierList onOpenChampion={onOpenChampion} />}
        {activeTab === 'champion' && <ChampionPage request={championRequest} />}
        {activeTab === 'crawler' && <Crawler />}
        {activeTab === 'settings' && (
          <Settings setDraftResult={setDraftResult} onSwitchToDraft={() => setActiveTab('draft')} />
        )}
      </AppShell>
    </RailProvider>
    </PatchProvider>
  )
}

export default App
