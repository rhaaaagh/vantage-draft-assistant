import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { getMetaTierList } from '../api/metaTierApi'
import type { MetaTierResponse, MetaTierRow } from '../api/metaTierApi'
import { getChampionIconUrl, useChampionCatalog } from '../api/championCatalog'
import './MetaTierList.css'

type SortKey = 'championName' | 'role' | 'patch' | 'games' | 'winRate'
type SortDir = 'asc' | 'desc'

interface MetaTierListProps {
  /** Открыть подробную страницу чемпиона (клик по имени в таблице). */
  onOpenChampion?: (championId: number, championName: string) => void
}

export function MetaTierList({ onOpenChampion }: MetaTierListProps = {}) {
  const { t } = useTranslation()
  const roleLabel = (role: string): string => t(`roles.${role}`, { defaultValue: role })
  useChampionCatalog()
  const [data, setData] = useState<MetaTierResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [patch, setPatch] = useState<string | null>(null) // null = ещё не выбран (берём новейший)
  const [role, setRole] = useState('')
  const [sortKey, setSortKey] = useState<SortKey>('winRate')
  const [sortDir, setSortDir] = useState<SortDir>('desc')

  const load = async (patchArg: string) => {
    setLoading(true)
    try {
      const res = await getMetaTierList(patchArg || undefined, role || undefined)
      setData(res)
      // Первый раз: если патч не выбран — выставляем новейший из доступных.
      if (patch === null && res.patches.length > 0) {
        setPatch(res.patches[0])
      } else if (patch === null) {
        setPatch('')
      }
    } catch (e) {
      console.error('Meta tier list load error:', e)
      setData({ patches: [], roles: [], rows: [] })
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void load(patch ?? '')
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [patch, role])

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((d) => (d === 'desc' ? 'asc' : 'desc'))
    } else {
      setSortKey(key)
      setSortDir(key === 'championName' || key === 'role' || key === 'patch' ? 'asc' : 'desc')
    }
  }

  const arrow = (key: SortKey) => (sortKey === key ? (sortDir === 'desc' ? ' ▼' : ' ▲') : '')

  const patches = data?.patches ?? []
  const roles = data?.roles ?? []
  const rows = useMemo(() => {
    const base = [...(data?.rows ?? [])]
    base.sort((a, b) => {
      let cmp = 0
      switch (sortKey) {
        case 'championName': cmp = a.championName.localeCompare(b.championName); break
        case 'role': cmp = a.role.localeCompare(b.role); break
        case 'patch': cmp = a.patch.localeCompare(b.patch); break
        case 'games': cmp = a.games - b.games; break
        case 'winRate': cmp = a.winRate - b.winRate; break
      }
      return sortDir === 'asc' ? cmp : -cmp
    })
    return base
  }, [data, sortKey, sortDir])

  return (
    <div className="meta-tier-view">
      <h2 className="meta-tier-title">{t('tier.title')}</h2>
      <p className="meta-tier-subtitle">{t('tier.subtitle')}</p>

      <div className="meta-tier-filters">
        <label className="meta-tier-filter-label">
          {t('tier.patch')}
          <select
            value={patch ?? ''}
            onChange={(e) => setPatch(e.target.value)}
            className="meta-tier-select"
          >
            <option value="">{t('tier.allPatches')}</option>
            {patches.map((p) => (
              <option key={p} value={p}>{p}</option>
            ))}
          </select>
        </label>
        <label className="meta-tier-filter-label">
          {t('tier.role')}
          <select
            value={role}
            onChange={(e) => setRole(e.target.value)}
            className="meta-tier-select"
          >
            <option value="">{t('tier.allRoles')}</option>
            {roles.map((r) => (
              <option key={r} value={r}>{roleLabel(r)}</option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="btn-secondary"
          onClick={() => void load(patch ?? '')}
          disabled={loading}
        >
          {loading ? t('tier.loading') : t('tier.refresh')}
        </button>
      </div>

      {loading && <p className="meta-tier-loading">{t('tier.loading')}</p>}

      {!loading && rows.length === 0 && (
        <div className="meta-tier-empty">
          <p>{t('tier.empty')}</p>
        </div>
      )}

      {!loading && rows.length > 0 && (
        <div className="meta-tier-table-wrap">
          <table className="meta-tier-table">
            <thead>
              <tr>
                <th>#</th>
                <th className="meta-tier-sortable" onClick={() => toggleSort('championName')}>{t('tier.col.champion')}{arrow('championName')}</th>
                <th className="meta-tier-sortable" onClick={() => toggleSort('role')}>{t('tier.col.role')}{arrow('role')}</th>
                <th className="meta-tier-sortable" onClick={() => toggleSort('patch')}>{t('tier.col.patch')}{arrow('patch')}</th>
                <th className="meta-tier-sortable" onClick={() => toggleSort('games')}>{t('tier.col.games')}{arrow('games')}</th>
                <th className="meta-tier-sortable" onClick={() => toggleSort('winRate')}>{t('tier.col.winrate')}{arrow('winRate')}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row: MetaTierRow, idx: number) => {
                const iconUrl = getChampionIconUrl(row.championId)
                return (
                  <tr key={`${row.championId}-${row.role}-${row.patch}-${idx}`}>
                    <td className="meta-tier-rank">{idx + 1}</td>
                    <td className="meta-tier-champ">
                      {onOpenChampion ? (
                        <button
                          type="button"
                          className="meta-tier-champ-btn"
                          title={t('tier.openPage', { name: row.championName })}
                          onClick={() => onOpenChampion(row.championId, row.championName)}
                        >
                          {iconUrl && (
                            <img
                              src={iconUrl}
                              alt=""
                              className="meta-tier-champ-icon"
                              width={24}
                              height={24}
                            />
                          )}
                          <span>{row.championName}</span>
                        </button>
                      ) : (
                        <>
                          {iconUrl && (
                            <img
                              src={iconUrl}
                              alt=""
                              className="meta-tier-champ-icon"
                              width={24}
                              height={24}
                            />
                          )}
                          <span>{row.championName}</span>
                        </>
                      )}
                    </td>
                    <td>{roleLabel(row.role)}</td>
                    <td>{row.patch}</td>
                    <td>{row.games}</td>
                    <td className="meta-tier-winrate">{(row.winRate * 100).toFixed(1)}%</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
