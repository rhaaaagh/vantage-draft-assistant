import React, { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchCurrentGameInfo } from '../api/draftApi'
import type { CurrentGamePlayer } from '../api/draftApi'
import { getChampionIconUrl, useChampionCatalog } from '../api/championCatalog'

const AUTO_REFRESH_MS = 60_000

function PlayerRow({ p }: { p: CurrentGamePlayer }) {
  const iconUrl = getChampionIconUrl(p.championId)
  const displayName = p.riotId || p.summonerName || '—'
  return (
    <div className="current-match-player">
      {iconUrl && (
        <img src={iconUrl} alt="" className="current-match-champ-icon" width={28} height={28} />
      )}
      <div className="current-match-player-info">
        <span className="current-match-champ-name">{p.championName}</span>
        <span className="current-match-summoner">{displayName}</span>
        <span className="current-match-rank">{p.rank || '—'}</span>
      </div>
    </div>
  )
}

export const CurrentMatch: React.FC = () => {
  const { t } = useTranslation()
  useChampionCatalog()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [autoRefresh, setAutoRefresh] = useState(false)
  const [data, setData] = useState<{
    hasGame: boolean
    myTeam: CurrentGamePlayer[]
    enemyTeam: CurrentGamePlayer[]
  } | null>(null)
  const loadingRef = useRef(false)

  const refresh = async () => {
    if (loadingRef.current) return
    setError(null)
    const apiKey = typeof window !== 'undefined' ? window.localStorage.getItem('lolda_riot_api_key') ?? '' : ''
    const region = typeof window !== 'undefined' ? window.localStorage.getItem('lolda_region') ?? 'ru' : 'ru'
    if (!apiKey.trim()) {
      setError(t('draft.currentMatch.noApiKey'))
      return
    }
    loadingRef.current = true
    setLoading(true)
    try {
      const res = await fetchCurrentGameInfo(apiKey, region)
      setData({
        hasGame: res.hasGame,
        myTeam: res.myTeam ?? [],
        enemyTeam: res.enemyTeam ?? [],
      })
      if (res.errorMessage) setError(res.errorMessage)
    } catch (e) {
      setError(String(e))
      setData(null)
    } finally {
      loadingRef.current = false
      setLoading(false)
    }
  }

  // Авто-обновление раз в минуту, пока вкладка открыта и включён чекбокс.
  useEffect(() => {
    if (!autoRefresh) return
    void refresh()
    const timer = setInterval(() => void refresh(), AUTO_REFRESH_MS)
    return () => clearInterval(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoRefresh])

  return (
    <div className="current-match-view">
      <section className="panel">
        <h2>{t('draft.currentMatch.title')}</h2>
        <p className="field-help">
          {t('draft.currentMatch.help')}
        </p>
        <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', flexWrap: 'wrap' }}>
          <button
            type="button"
            className="btn-primary"
            disabled={loading}
            onClick={() => void refresh()}
          >
            {loading ? t('draft.currentMatch.loading') : t('common.refresh')}
          </button>
          <label style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
            />
            <span>{t('draft.currentMatch.autoRefresh')}</span>
          </label>
        </div>

        {error && <p className="field-help text-danger">{error}</p>}

        {data && !data.hasGame && !error && (
          <p className="field-help">{t('draft.currentMatch.noGame')}</p>
        )}

        {data?.hasGame && (
          <div className="current-match-teams">
            <div className="current-match-team">
              <h3>{t('draft.currentMatch.myTeam')}</h3>
              {data.myTeam.length === 0 ? (
                <p className="field-help">{t('draft.currentMatch.noData')}</p>
              ) : (
                <ul className="current-match-list">
                  {data.myTeam.map((p, i) => (
                    <li key={`my-${i}-${p.riotId || p.summonerName}`}>
                      <PlayerRow p={p} />
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <div className="current-match-team current-match-team-enemy">
              <h3>{t('draft.currentMatch.enemyTeam')}</h3>
              {data.enemyTeam.length === 0 ? (
                <p className="field-help">{t('draft.currentMatch.noData')}</p>
              ) : (
                <ul className="current-match-list">
                  {data.enemyTeam.map((p, i) => (
                    <li key={`enemy-${i}-${p.riotId || p.summonerName}`}>
                      <PlayerRow p={p} />
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        )}
      </section>
    </div>
  )
}
