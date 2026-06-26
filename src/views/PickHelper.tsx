import React, { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchPickRecommendations } from '../api/recommendApi'
import type { DraftPick, PickRec } from '../api/recommendApi'
import { getAllChampions, getChampionIconUrl, useChampionCatalog } from '../api/championCatalog'
import { Card } from '../components/ui'
import { usePatch } from '../components/patchContext'
import './PickHelper.css'

const ROLES = ['TOP', 'JUNGLE', 'MID', 'BOT', 'SUPPORT'] as const

/** Добавление пика: поиск чемпиона + роль. */
function PickAdder({ onAdd }: { onAdd: (p: DraftPick) => void }) {
  const { t } = useTranslation()
  const [query, setQuery] = useState('')
  const [role, setRole] = useState<string>('MID')
  const q = query.trim().toLowerCase()
  const matches = q
    ? getAllChampions().filter((c) => c.name.toLowerCase().includes(q)).slice(0, 6)
    : []
  return (
    <div className="ph-adder">
      <select value={role} onChange={(e) => setRole(e.target.value)} className="ph-role-sel">
        {ROLES.map((r) => (
          <option key={r} value={r}>{t(`roles.${r}`)}</option>
        ))}
      </select>
      <div className="ph-adder-search">
        <input
          type="text"
          value={query}
          placeholder={t('draft.pickHelper.championPlaceholder')}
          onChange={(e) => setQuery(e.target.value)}
        />
        {matches.length > 0 && (
          <div className="ph-suggest">
            {matches.map((c) => {
              const icon = getChampionIconUrl(c.id)
              return (
                <button
                  type="button"
                  key={c.id}
                  className="ph-suggest-item"
                  onClick={() => {
                    onAdd({ championId: c.id, role })
                    setQuery('')
                  }}
                >
                  {icon && <img src={icon} alt="" width={22} height={22} />}
                  <span>{c.name}</span>
                </button>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}

function PickChips({ picks, onRemove }: { picks: DraftPick[]; onRemove: (i: number) => void }) {
  const { t } = useTranslation()
  if (picks.length === 0) return <span className="ui-dim">{t('draft.pickHelper.empty')}</span>
  return (
    <div className="ph-chips">
      {picks.map((p, i) => {
        const icon = getChampionIconUrl(p.championId)
        const champ = getAllChampions().find((c) => c.id === p.championId)
        return (
          <button type="button" className="ph-chip" key={i} onClick={() => onRemove(i)} title={t('draft.pickHelper.remove')}>
            {icon && <img src={icon} alt="" width={20} height={20} />}
            <span>{champ?.name ?? p.championId}</span>
            <span className="ph-chip-role">{t(`roles.${p.role}`, p.role)}</span>
            <span className="ph-chip-x">×</span>
          </button>
        )
      })}
    </div>
  )
}

export const PickHelper: React.FC = () => {
  const { t } = useTranslation()
  useChampionCatalog()
  const [myRole, setMyRole] = useState<string>('MID')
  const [enemies, setEnemies] = useState<DraftPick[]>([])
  const [allies, setAllies] = useState<DraftPick[]>([])
  const [recs, setRecs] = useState<PickRec[] | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { patch } = usePatch()

  const run = async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await fetchPickRecommendations(myRole, enemies, allies, patch || undefined)
      setRecs(res)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  // Если рекомендации уже показаны — пересчитываем при смене выбранного патча.
  useEffect(() => {
    if (recs !== null) void run()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [patch])

  return (
    <Card title={t('draft.pickHelper.title')}>
      <div className="ph-row">
        <span className="ph-label">{t('draft.pickHelper.myRole')}</span>
        <div className="ph-roles">
          {ROLES.map((r) => (
            <button
              key={r}
              type="button"
              className={`ph-role-tab${myRole === r ? ' is-active' : ''}`}
              onClick={() => setMyRole(r)}
            >
              {t(`roles.${r}`)}
            </button>
          ))}
        </div>
      </div>

      <div className="ph-row">
        <span className="ph-label">{t('draft.pickHelper.enemies')}</span>
        <div className="ph-col">
          <PickChips picks={enemies} onRemove={(i) => setEnemies((p) => p.filter((_, j) => j !== i))} />
          <PickAdder onAdd={(p) => setEnemies((prev) => [...prev, p])} />
        </div>
      </div>

      <div className="ph-row">
        <span className="ph-label">{t('draft.pickHelper.allies')}</span>
        <div className="ph-col">
          <PickChips picks={allies} onRemove={(i) => setAllies((p) => p.filter((_, j) => j !== i))} />
          <PickAdder onAdd={(p) => setAllies((prev) => [...prev, p])} />
        </div>
      </div>

      <button type="button" className="ph-go" onClick={() => void run()} disabled={loading}>
        {loading ? t('draft.pickHelper.compute') : t('draft.pickHelper.showRecs')}
      </button>

      {error && <p className="ui-error">{error}</p>}

      {recs && recs.length === 0 && !loading && (
        <p className="ui-muted">
          {t('draft.pickHelper.noRecs')}
        </p>
      )}

      {recs && recs.length > 0 && (
        <div className="ph-recs">
          {recs.map((r, i) => {
            const icon = getChampionIconUrl(r.championId)
            return (
              <div className="ph-rec" key={r.championId}>
                <span className="ph-rec-rank num">{i + 1}</span>
                {icon && <img src={icon} alt="" width={36} height={36} className="ph-rec-icon" />}
                <div className="ph-rec-main">
                  <span className="ph-rec-name">{r.championName}</span>
                  <span className="ph-rec-reason">{r.reason}</span>
                </div>
                <div className="ph-rec-score">
                  <div className="ph-rec-bar">
                    <div className="ph-rec-bar-fill" style={{ width: `${Math.round(r.score)}%` }} />
                  </div>
                  <span className="num">{Math.round(r.score)}</span>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </Card>
  )
}

export default PickHelper
