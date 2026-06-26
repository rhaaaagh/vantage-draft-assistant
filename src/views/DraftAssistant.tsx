import React, { useEffect, useRef, useState } from 'react'
import { Trans, useTranslation } from 'react-i18next'
import type { DraftAnalysisResult, DraftState } from '../domain/draft'
import { DraftView } from '../components/DraftView'
import { RecommendationsPanel } from '../components/RecommendationsPanel'
import { AnalyticsSummary } from '../components/AnalyticsSummary'
import { fetchDraftAnalysis, fetchDraftBans, simulatePicks } from '../api/draftApi'
import type { DraftSimulationResult } from '../domain/draft'

const POLL_MS = 4_000

const emptyDraftState: DraftState = {
  phase: 'NONE',
  blue: { side: 'BLUE', slots: [], bans: [] },
  red: { side: 'RED', slots: [], bans: [] },
}

interface DraftAssistantProps {
  draftResult: DraftAnalysisResult | null
  setDraftResult: React.Dispatch<React.SetStateAction<DraftAnalysisResult | null>>
}

export const DraftAssistant: React.FC<DraftAssistantProps> = ({ draftResult, setDraftResult }) => {
  const { t } = useTranslation()
  const [loading, setLoading] = useState(false)
  const [autoRefresh, setAutoRefresh] = useState(true)
  const busyRef = useRef(false)

  const refresh = async (quiet = false) => {
    if (busyRef.current) return
    busyRef.current = true
    if (!quiet) setLoading(true)
    try {
      const data = await fetchDraftAnalysis()
      if (data && typeof data === 'object' && data.draft) {
        const d = data as unknown as Record<string, unknown>
        const draftRaw = ((d.draft as Record<string, unknown>) ?? {}) as Record<string, unknown>
        const blue = (draftRaw.blue as Record<string, unknown>) ?? {}
        const red = (draftRaw.red as Record<string, unknown>) ?? {}
        const getBans = (team: unknown): number[] => {
          if (!team || typeof team !== 'object') return []
          const t = team as Record<string, unknown>
          const b = t.bans ?? t.Bans
          return Array.isArray(b) ? (b as number[]) : []
        }
        const arr = (x: unknown): number[] => (Array.isArray(x) ? (x as number[]) : [])
        const firstNonEmpty = (...sources: number[][]): number[] => sources.find((s) => s.length > 0) ?? []
        const blueBans = firstNonEmpty(arr(d.blueBans), arr(d.blue_bans), getBans(draftRaw.blue))
        const redBans = firstNonEmpty(arr(d.redBans), arr(d.red_bans), getBans(draftRaw.red))
        const draftWithBans = {
          ...draftRaw,
          blue: { ...blue, side: blue.side, slots: blue.slots ?? [], bans: blueBans },
          red: { ...red, side: red.side, slots: red.slots ?? [], bans: redBans },
        }
        const normalized = {
          draft: draftWithBans as DraftState,
          blueBans,
          redBans,
          bestPicks: Array.isArray(data.bestPicks) ? data.bestPicks : [],
          worstPicks: Array.isArray(data.worstPicks) ? data.worstPicks : [],
          build: data.build ?? null,
          analytics: data.analytics ?? {
            blueWinProbability: null,
            redWinProbability: null,
            blueSynergyScore: null,
            redSynergyScore: null,
            blueDamageProfile: { ad: 0.5, ap: 0.5 },
            redDamageProfile: { ad: 0.5, ap: 0.5 },
            blueWeaknesses: [],
            redWeaknesses: [],
          },
        }
        setDraftResult(normalized)
        // Баны подгружаем отдельно; мержим только если пришли непустые — иначе не перезатираем
        try {
          const bansRes = await fetchDraftBans()
          const blueBans = bansRes.blueBans ?? []
          const redBans = bansRes.redBans ?? []
          const hasBans = blueBans.length > 0 || redBans.length > 0
          if (hasBans) {
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
          }
        } catch (e) {
          console.warn('fetchDraftBans failed:', e)
        }
      } else {
        setDraftResult(null)
      }
    } catch (e) {
      console.error('DraftAssistant load error:', e)
      if (!quiet) setDraftResult(null)
    } finally {
      busyRef.current = false
      if (!quiet) setLoading(false)
    }
  }

  // Авто-поллинг драфта: LCU локальный, лимитов нет. Команды бэкенда теперь
  // асинхронные, так что UI не зависает.
  useEffect(() => {
    if (!autoRefresh) return
    void refresh(true)
    const t = setInterval(() => void refresh(true), POLL_MS)
    return () => clearInterval(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoRefresh])

  const result = draftResult
  const draft = result?.draft ?? emptyDraftState
  const picks = {
    best: Array.isArray(result?.bestPicks) ? result.bestPicks : [],
    worst: Array.isArray(result?.worstPicks) ? result.worstPicks : [],
  }
  const builds = result?.build ?? null
  const analytics = result?.analytics ?? null
  const [simInput, setSimInput] = useState('127, 3, 61')
  const [simResult, setSimResult] = useState<DraftSimulationResult | null>(null)
  const [simLoading, setSimLoading] = useState(false)
  const [simError, setSimError] = useState<string | null>(null)

  return (
    <div className="draft-assistant">
      <section className="welcome-block">
        <h2 className="welcome-title">{t('draft.assistant.welcomeTitle')}</h2>
        <p>{t('draft.assistant.welcomeText')}</p>
      </section>
      <section className="panel panel-draft">
        <div className="panel-draft-header">
          <h2>{t('draft.assistant.currentDraft')}</h2>
          <div style={{ display: 'flex', alignItems: 'center', gap: '1rem' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={(e) => setAutoRefresh(e.target.checked)}
              />
              <span>{t('draft.assistant.autoRefresh')}</span>
            </label>
            <button
              type="button"
              className="btn-primary"
              disabled={loading}
              onClick={() => void refresh()}
            >
              {loading ? t('draft.assistant.loading') : t('draft.assistant.refreshDraft')}
            </button>
          </div>
        </div>
        <DraftView
          draft={draft}
          blueBans={result?.blueBans}
          redBans={result?.redBans}
        />
      </section>

      <section className="panel panel-recommendations">
        <h2>{t('draft.assistant.recommendations')}</h2>
        <RecommendationsPanel best={picks.best} worst={picks.worst} builds={builds} />
      </section>

      {analytics && (
        <section className="panel panel-analytics">
          <h2>{t('draft.assistant.analytics')}</h2>
          <AnalyticsSummary analytics={analytics} />
        </section>
      )}

      <section className="panel">
        <h2>{t('draft.assistant.simulator')}</h2>
        <p className="field-help">
          {t('draft.assistant.simulatorHelp')}
        </p>
        <div className="form-field">
          <label htmlFor="sim-champs">{t('draft.assistant.championIds')}</label>
          <input
            id="sim-champs"
            type="text"
            value={simInput}
            onChange={(e) => setSimInput(e.target.value)}
          />
        </div>
        <button
          type="button"
          className="btn-secondary"
          disabled={simLoading}
          onClick={async () => {
            setSimError(null)
            const ids = simInput
              .split(/[,;\s]+/)
              .map((s) => Number.parseInt(s, 10))
              .filter((n) => Number.isFinite(n) && n > 0)
            if (ids.length === 0) {
              setSimError(t('draft.assistant.invalidIds'))
              return
            }
            try {
              setSimLoading(true)
              const result = await simulatePicks(ids)
              setSimResult(result)
            } catch (e) {
              setSimError(String(e))
            } finally {
              setSimLoading(false)
            }
          }}
        >
          {simLoading ? t('draft.assistant.simulating') : t('draft.assistant.simulate')}
        </button>
        {simError && <p className="field-help">{simError}</p>}

        {simResult && (
          <div className="sim-result-block">
            {simResult.baseWinProbability === null ? (
              <p className="field-help">
                {t('draft.assistant.noWinStats')}
              </p>
            ) : (
              <>
                <p>
                  <Trans
                    i18nKey="draft.assistant.currentDraftWin"
                    values={{ percent: (simResult.baseWinProbability * 100).toFixed(1) }}
                    components={{ strong: <strong /> }}
                  />
                </p>
                <ul className="recommendation-list">
                  {simResult.entries.map((entry) => (
                    <li key={entry.championId} className="recommendation-item">
                      <div className="recommendation-title">
                        {t('draft.assistant.pickEntry', { champion: entry.championName, id: entry.championId })}{' '}
                        <span className="pill">
                          {entry.winProbability === null
                            ? t('draft.assistant.noData')
                            : `${(entry.winProbability * 100).toFixed(1)}%`}
                        </span>
                      </div>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        )}
      </section>
    </div>
  )
}
