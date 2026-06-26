import React, { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { syncMatches } from '../api/statsApi'
import { checkLcu, debugGameInfoForRiotId, fetchDraftAnalysis, fetchDraftBans, getLeaguePath, setLeaguePath } from '../api/draftApi'
import type { DraftAnalysisResult } from '../domain/draft'
import {
  getStoredAlwaysOnTop,
  isTauriWindow,
  saveWindowBounds,
  setAlwaysOnTop,
} from '../utils/windowOverlay'
import { parseRiotId } from '../utils/riotId'
import { LANGUAGES, setLanguage } from '../i18n'

function LanguageSection() {
  const { t, i18n } = useTranslation()
  return (
    <div className="form-field">
      <label htmlFor="lang-select">{t('settings.language.label')}</label>
      <select
        id="lang-select"
        value={i18n.resolvedLanguage ?? i18n.language}
        onChange={(e) => setLanguage(e.target.value)}
      >
        {LANGUAGES.map((l) => (
          <option key={l.code} value={l.code}>
            {l.label}
          </option>
        ))}
      </select>
      <p className="field-help">{t('settings.language.help')}</p>
    </div>
  )
}

function WindowOverlaySection() {
  const { t } = useTranslation()
  const [alwaysOnTop, setAlwaysOnTopState] = useState(getStoredAlwaysOnTop)
  const [saving, setSaving] = useState(false)
  const isTauri = isTauriWindow()

  useEffect(() => {
    if (!isTauri) return
    setAlwaysOnTopState(getStoredAlwaysOnTop())
  }, [isTauri])

  const handleAlwaysOnTopChange = async (checked: boolean) => {
    setAlwaysOnTopState(checked)
    await setAlwaysOnTop(checked)
  }

  const handleSaveBounds = async () => {
    if (!isTauri) return
    setSaving(true)
    try {
      await saveWindowBounds()
    } finally {
      setSaving(false)
    }
  }

  if (!isTauri) {
    return <p className="field-help">{t('settings.window.desktopOnly')}</p>
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
      <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
        <input
          type="checkbox"
          checked={alwaysOnTop}
          onChange={(e) => void handleAlwaysOnTopChange(e.target.checked)}
        />
        <span>{t('settings.window.alwaysOnTop')}</span>
      </label>
      <p className="field-help" style={{ margin: 0 }}>
        {t('settings.window.boundsHelp')}
      </p>
      <button type="button" className="btn-secondary" disabled={saving} onClick={() => void handleSaveBounds()}>
        {saving ? t('common.saving') : t('settings.window.saveBounds')}
      </button>
    </div>
  )
}

function LeaguePathField() {
  const { t } = useTranslation()
  const [leaguePath, setLeaguePathState] = useState('')
  const [saved, setSaved] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

  useEffect(() => {
    if (!isTauri) return
    getLeaguePath().then((p) => {
      setLeaguePathState(p)
      setLoaded(true)
    })
  }, [isTauri])

  const handleSave = async () => {
    if (!isTauri) return
    try {
      await setLeaguePath(leaguePath)
      setSaved(true)
      setTimeout(() => setSaved(false), 2000)
    } catch (e) {
      console.error('setLeaguePath failed:', e)
    }
  }

  if (!isTauri) return null

  return (
    <div className="form-field" style={{ marginBottom: '1rem' }}>
      <label htmlFor="league-path">{t('settings.lcu.leaguePathLabel')}</label>
      <input
        id="league-path"
        type="text"
        value={leaguePath}
        placeholder="C:\Riot Games\League of Legends"
        onChange={(e) => setLeaguePathState(e.target.value)}
        style={{ width: '100%', maxWidth: '400px' }}
      />
      <p className="field-help" style={{ marginTop: '0.25rem' }}>
        {t('settings.lcu.leaguePathHelp')}
      </p>
      <button type="button" className="btn-secondary" onClick={() => void handleSave()} disabled={!loaded}>
        {saved ? t('common.saved') : t('settings.lcu.savePath')}
      </button>
    </div>
  )
}

const defaultAnalytics = {
  blueWinProbability: null,
  redWinProbability: null,
  blueSynergyScore: null,
  redSynergyScore: null,
  blueDamageProfile: { ad: 0.5, ap: 0.5 },
  redDamageProfile: { ad: 0.5, ap: 0.5 },
  blueWeaknesses: [] as string[],
  redWeaknesses: [] as string[],
}

function getBansFromTeam(team: unknown): number[] {
  if (!team || typeof team !== 'object') return []
  const t = team as Record<string, unknown>
  const b = t.bans ?? t.Bans
  return Array.isArray(b) ? (b as number[]) : []
}

function normalizeDraftResult(data: unknown): DraftAnalysisResult | null {
  if (!data || typeof data !== 'object' || !('draft' in data) || !(data as { draft?: unknown }).draft) return null
  const d = data as Record<string, unknown>
  const draftRaw = d.draft as Record<string, unknown>
  const blue = (draftRaw?.blue as Record<string, unknown>) ?? {}
  const red = (draftRaw?.red as Record<string, unknown>) ?? {}
  const blueBans = getBansFromTeam(draftRaw?.blue)
  const redBans = getBansFromTeam(draftRaw?.red)
  const draft = {
    ...draftRaw,
    blue: { ...blue, bans: blueBans },
    red: { ...red, bans: redBans },
  } as DraftAnalysisResult['draft']
  return {
    draft,
    blueBans,
    redBans,
    bestPicks: Array.isArray(d.bestPicks) ? d.bestPicks : [],
    worstPicks: Array.isArray(d.worstPicks) ? d.worstPicks : [],
    build: (d.build as DraftAnalysisResult['build']) ?? null,
    analytics: (d.analytics as DraftAnalysisResult['analytics']) ?? defaultAnalytics,
  }
}

interface SettingsProps {
  setDraftResult: React.Dispatch<React.SetStateAction<DraftAnalysisResult | null>>
  onSwitchToDraft?: () => void
}

export const Settings: React.FC<SettingsProps> = ({ setDraftResult, onSwitchToDraft }) => {
  const { t } = useTranslation()
  const [riotApiKey, setRiotApiKey] = useState('')
  const [region, setRegion] = useState('ru')
  const [riotId, setRiotId] = useState('')
  const [syncStatus, setSyncStatus] = useState<string | null>(null)
  const [syncing, setSyncing] = useState(false)
  const [lcuStatus, setLcuStatus] = useState<string | null>(null)
  const [lcuChecking, setLcuChecking] = useState(false)

  // Загружаем сохранённые настройки при старте
  useEffect(() => {
    if (typeof window === 'undefined') return
    try {
      const storedKey = window.localStorage.getItem('lolda_riot_api_key')
      const storedRegion = window.localStorage.getItem('lolda_region')
      const storedGameName = window.localStorage.getItem('lolda_game_name')
      const storedTagLine = window.localStorage.getItem('lolda_tag_line')
      if (storedKey) setRiotApiKey(storedKey)
      if (storedRegion) setRegion(storedRegion)
      if (storedGameName && storedTagLine) {
        setRiotId(`${storedGameName}#${storedTagLine}`)
      } else {
        const legacy = window.localStorage.getItem('lolda_summoner_name')
        if (legacy) setRiotId(legacy.includes('#') ? legacy : legacy)
      }
    } catch {
      // игнорируем ошибки localStorage
    }
  }, [])

  const parsedRiotId = parseRiotId(riotId)

  const handleSaveSettings = () => {
    if (typeof window === 'undefined') return
    if (riotId.trim() && !parsedRiotId) {
      setSyncStatus(t('settings.riotApi.invalidRiotId'))
      return
    }
    try {
      window.localStorage.setItem('lolda_riot_api_key', riotApiKey)
      window.localStorage.setItem('lolda_region', region)
      if (parsedRiotId) {
        window.localStorage.setItem('lolda_game_name', parsedRiotId[0])
        window.localStorage.setItem('lolda_tag_line', parsedRiotId[1])
      }
      setSyncStatus(t('settings.riotApi.savedOk'))
    } catch (e) {
      setSyncStatus(t('settings.riotApi.saveFailed', { error: String(e) }))
    }
  }

  return (
    <div className="settings">
      <section className="panel">
        <h2>{t('settings.language.title')}</h2>
        <LanguageSection />
      </section>

      <section className="panel">
        <h2>{t('settings.riotApi.title')}</h2>
        <div className="form-field">
          <label htmlFor="riot-api-key">{t('settings.riotApi.keyLabel')}</label>
          <input
            id="riot-api-key"
            type="password"
            value={riotApiKey}
            placeholder="RGAPI-..."
            onChange={(e) => setRiotApiKey(e.target.value)}
          />
          <p className="field-help">{t('settings.riotApi.keyHelp')}</p>
        </div>

        <div className="form-field">
          <label htmlFor="riot-id">{t('settings.riotApi.riotIdLabel')}</label>
          <input
            id="riot-id"
            type="text"
            value={riotId}
            placeholder={t('settings.riotApi.riotIdPlaceholder')}
            onChange={(e) => setRiotId(e.target.value)}
          />
          <p className="field-help">{t('settings.riotApi.riotIdHelp')}</p>
        </div>

        <button
          type="button"
          className="btn-secondary"
          style={{ marginBottom: '0.5rem' }}
          onClick={handleSaveSettings}
        >
          {t('settings.riotApi.save')}
        </button>

        <button
          type="button"
          className="btn-primary"
          disabled={syncing || !riotApiKey || !parsedRiotId}
          onClick={async () => {
            if (!parsedRiotId) return
            try {
              setSyncing(true)
              setSyncStatus(t('settings.riotApi.syncing'))
              const processed = await syncMatches({
                apiKey: riotApiKey,
                region,
                gameName: parsedRiotId[0],
                tagLine: parsedRiotId[1],
                count: 50,
              })
              setSyncStatus(t('settings.riotApi.synced', { count: processed }))
            } catch (e) {
              setSyncStatus(String(e))
            } finally {
              setSyncing(false)
            }
          }}
        >
          {syncing ? t('settings.riotApi.syncing') : t('settings.riotApi.sync')}
        </button>

        {syncStatus && <p className="field-help">{syncStatus}</p>}

        <div className="form-field">
          <label htmlFor="region">{t('settings.riotApi.regionLabel')}</label>
          <select id="region" value={region} onChange={(e) => setRegion(e.target.value)}>
            <option value="euw1">EUW</option>
            <option value="eun1">EUNE</option>
            <option value="na1">NA</option>
            <option value="kr">KR</option>
            <option value="ru">RU</option>
          </select>
        </div>
      </section>

      <section className="panel">
        <h2>{t('settings.lcu.title')}</h2>
        <p className="field-help">{t('settings.lcu.help')}</p>
        <LeaguePathField />
        <button
          type="button"
          className="btn-secondary"
          disabled={lcuChecking}
          onClick={async () => {
            setLcuStatus(null)
            setLcuChecking(true)
            try {
              const result = await checkLcu()
              setLcuStatus(result.message)
              if (result.found && result.sessionSaved) {
                try {
                  const data = await fetchDraftAnalysis()
                  const normalized = normalizeDraftResult(data)
                  if (normalized) {
                    setDraftResult(normalized)
                    setLcuStatus((prev) =>
                      prev ? `${prev} ${t('settings.lcu.draftLoaded')}` : t('settings.lcu.draftLoaded'),
                    )
                    onSwitchToDraft?.()
                    try {
                      const bansRes = await fetchDraftBans()
                      const blueBans = bansRes.blueBans ?? []
                      const redBans = bansRes.redBans ?? []
                      setDraftResult((prev) => {
                        if (!prev) return prev
                        return {
                          ...prev,
                          blueBans,
                          redBans,
                          draft: {
                            ...prev.draft,
                            blue: { ...prev.draft.blue, bans: blueBans },
                            red: { ...prev.draft.red, bans: redBans },
                          },
                        }
                      })
                    } catch {
                      // баны не подгрузились — драфт уже показан
                    }
                  } else {
                    setLcuStatus((prev) =>
                      prev
                        ? `${prev} ${t('settings.lcu.draftParseFailed')}`
                        : t('settings.lcu.draftParseFailed'),
                    )
                  }
                } catch {
                  setLcuStatus((prev) =>
                    prev ? `${prev} ${t('settings.lcu.draftLoadError')}` : t('settings.lcu.draftLoadError'),
                  )
                }
              }
            } catch (e) {
              setLcuStatus(String(e))
            } finally {
              setLcuChecking(false)
            }
          }}
        >
          {lcuChecking ? t('common.checking') : t('settings.lcu.check')}
        </button>
        {lcuStatus && <p className="field-help" style={{ marginTop: '0.5rem' }}>{lcuStatus}</p>}
      </section>

      <section className="panel">
        <h2>{t('settings.window.title')}</h2>
        <WindowOverlaySection />
      </section>

      <details className="panel" style={{ padding: '1rem' }}>
        <summary style={{ cursor: 'pointer', fontWeight: 600 }}>{t('settings.debug.title')}</summary>
        <DebugRiotApiSection apiKey={riotApiKey} region={region} />
      </details>
    </div>
  )
}

/** Проверка Riot API по любому Riot ID (без запущенной своей игры). */
function DebugRiotApiSection({ apiKey, region }: { apiKey: string; region: string }) {
  const { t } = useTranslation()
  const [debugRiotId, setDebugRiotId] = useState('')
  const [status, setStatus] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const run = async () => {
    const parsed = parseRiotId(debugRiotId)
    if (!parsed) {
      setStatus(t('settings.debug.enterRiotId'))
      return
    }
    if (!apiKey.trim()) {
      setStatus(t('settings.debug.enterKey'))
      return
    }
    setBusy(true)
    setStatus(t('settings.debug.checkingChain'))
    try {
      const res = await debugGameInfoForRiotId(apiKey, region, parsed[0], parsed[1])
      if (!res.hasGame) {
        setStatus(res.errorMessage ?? t('settings.debug.noGame'))
      } else {
        const names = [...res.myTeam, ...res.enemyTeam]
          .map((p) => `${p.championName} (${p.riotId || p.summonerName || '—'}, ${p.rank})`)
          .join('; ')
        setStatus(t('settings.debug.gameFound', { names }))
      }
    } catch (e) {
      setStatus(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ marginTop: '0.75rem' }}>
      <p className="field-help">{t('settings.debug.help')}</p>
      <div className="form-field">
        <label htmlFor="debug-riot-id">{t('settings.debug.riotIdLabel')}</label>
        <input
          id="debug-riot-id"
          type="text"
          value={debugRiotId}
          placeholder={t('settings.riotApi.riotIdPlaceholder')}
          onChange={(e) => setDebugRiotId(e.target.value)}
        />
      </div>
      <button type="button" className="btn-secondary" disabled={busy} onClick={() => void run()}>
        {busy ? t('common.checking') : t('settings.debug.run')}
      </button>
      {status && <p className="field-help" style={{ marginTop: '0.5rem' }}>{status}</p>}
    </div>
  )
}
