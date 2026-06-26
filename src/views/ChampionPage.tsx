import React, { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchChampionPage } from '../api/championApi'
import type {
  ChampItem,
  ChampMatchup,
  ChampionPageResponse,
  ChampRune,
  ChampRunes,
  ChampSynergy,
} from '../api/championApi'
import {
  getAllChampions,
  getChampionIconUrl,
  getItemIconUrl,
  useChampionCatalog,
} from '../api/championCatalog'
import { getRuneIconUrl, getRuneInfo, useRuneCatalog } from '../api/runeCatalog'
import { Card, Stat, StatRow } from '../components/ui'
import { usePatch } from '../components/patchContext'
import { IconSearch } from '../components/icons'
import './ChampionPage.css'

function pct(x: number): string {
  return `${(x * 100).toFixed(1)}%`
}

function wrClass(wr: number): string {
  if (wr >= 0.52) return 'cp-wr--high'
  if (wr >= 0.48) return 'cp-wr--mid'
  return 'cp-wr--low'
}

function MatchupList({ title, items }: { title: string; items: ChampMatchup[] }) {
  const { t } = useTranslation()
  return (
    <Card title={title}>
      {items.length === 0 ? (
        <p className="ui-dim">{t('champion.notEnoughData')}</p>
      ) : (
        <div className="cp-list">
          {items.map((m) => {
            const icon = getChampionIconUrl(m.championId)
            return (
              <div className="cp-row" key={m.championId}>
                {icon && <img src={icon} alt="" width={28} height={28} className="cp-icon" />}
                <span className="cp-name">{m.championName}</span>
                <span className={`cp-wr num ${wrClass(m.winRate)}`}>{pct(m.winRate)}</span>
                <span className="cp-games num">{t('champion.games', { count: m.games })}</span>
              </div>
            )
          })}
        </div>
      )}
    </Card>
  )
}

function SynergyList({ items }: { items: ChampSynergy[] }) {
  const { t } = useTranslation()
  return (
    <Card title={t('champion.synergies')}>
      {items.length === 0 ? (
        <p className="ui-dim">{t('champion.notEnoughData')}</p>
      ) : (
        <div className="cp-list">
          {items.map((m) => {
            const icon = getChampionIconUrl(m.championId)
            return (
              <div className="cp-row" key={`${m.championId}-${m.role}`}>
                {icon && <img src={icon} alt="" width={28} height={28} className="cp-icon" />}
                <span className="cp-name">{m.championName}</span>
                <span className="cp-role">{t(`roles.${m.role}`, { defaultValue: m.role })}</span>
                <span className={`cp-wr num ${wrClass(m.winRate)}`}>{pct(m.winRate)}</span>
                <span className="cp-games num">{t('champion.games', { count: m.games })}</span>
              </div>
            )
          })}
        </div>
      )}
    </Card>
  )
}

function fmtGames(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

function ItemRow({ items }: { items: ChampItem[] }) {
  const { t } = useTranslation()
  if (items.length === 0) return <p className="ui-dim">{t('champion.notEnoughData')}</p>
  return (
    <div className="cp-items">
      {items.map((it) => (
        <div className="cp-item" key={it.itemId}>
          <img src={getItemIconUrl(it.itemId)} alt="" width={32} height={32} />
          <span className={`cp-item-wr num ${wrClass(it.winRate)}`}>{pct(it.winRate)}</span>
          <span className="cp-item-games num">{t('champion.games', { count: fmtGames(it.games) })}</span>
        </div>
      ))}
    </div>
  )
}

function RuneGroup({ title, items }: { title: string; items: ChampRune[] }) {
  const { t } = useTranslation()
  if (items.length === 0) return null
  return (
    <div className="cp-rune-group">
      <span className="cp-rune-group-title">{title}</span>
      <div className="cp-runes">
        {items.map((r) => {
          const url = getRuneIconUrl(r.runeId)
          const info = getRuneInfo(r.runeId)
          return (
            <div className="cp-rune" key={r.runeId} title={info?.name ?? String(r.runeId)}>
              {url ? (
                <img src={url} alt="" width={32} height={32} className="cp-rune-icon" />
              ) : (
                <span className="cp-rune-icon cp-rune-icon--ph" />
              )}
              <span className="cp-rune-name">{info?.name ?? `#${r.runeId}`}</span>
              <span className={`cp-rune-wr num ${wrClass(r.winRate)}`}>{pct(r.winRate)}</span>
              <span className="cp-rune-games num">{t('champion.games', { count: fmtGames(r.games) })}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}

function RunesCard({ runes }: { runes: ChampRunes }) {
  const { t } = useTranslation()
  useRuneCatalog()
  const empty =
    runes.keystones.length === 0 && runes.primary.length === 0 && runes.secondary.length === 0
  return (
    <Card title={t('champion.runes.title')}>
      {empty ? (
        <p className="ui-dim">{t('champion.runes.noData')}</p>
      ) : (
        <div className="cp-rune-groups">
          <RuneGroup title={t('champion.runes.keystone')} items={runes.keystones} />
          <RuneGroup title={t('champion.runes.primaryTree')} items={runes.primary} />
          <RuneGroup title={t('champion.runes.secondaryTree')} items={runes.secondary} />
        </div>
      )}
    </Card>
  )
}

/** Внешний запрос открыть страницу чемпиона (напр. кликом из тир-листа). */
export interface ChampionRequest {
  id: number
  name: string
  /** Инкрементится на каждый клик — чтобы повторный клик по тому же чемпиону сработал. */
  seq: number
}

export const ChampionPage: React.FC<{ request?: ChampionRequest | null }> = ({ request }) => {
  const { t } = useTranslation()
  useChampionCatalog()
  const [championId, setChampionId] = useState<number | null>(null)
  const [championName, setChampionName] = useState('')
  const [roleSel, setRoleSel] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [data, setData] = useState<ChampionPageResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [appliedSeq, setAppliedSeq] = useState(0)
  const { patch } = usePatch()

  // Открытие чемпиона по внешнему запросу (клик из тир-листа и т. п.). Применяем
  // при смене seq прямо в рендере — паттерн React «корректировка состояния при
  // изменении пропа» (без эффекта; повторный клик меняет seq → срабатывает снова).
  if (request && request.seq !== appliedSeq) {
    setAppliedSeq(request.seq)
    setChampionId(request.id)
    setChampionName(request.name)
    setRoleSel(null)
    setData(null)
    setQuery(request.name)
  }

  useEffect(() => {
    if (championId == null) return
    let active = true
    fetchChampionPage(championId, roleSel, patch || undefined)
      .then((res) => {
        if (!active) return
        setData(res)
        setError(null)
      })
      .catch((e) => {
        if (active) setError(String(e))
      })
    return () => {
      active = false
    }
  }, [championId, roleSel, patch])

  const q = query.trim().toLowerCase()
  const suggestions =
    q && q !== championName.toLowerCase()
      ? getAllChampions()
          .filter((c) => c.name.toLowerCase().includes(q))
          .slice(0, 8)
      : []

  const pick = (id: number, name: string) => {
    setChampionId(id)
    setChampionName(name)
    setRoleSel(null)
    setData(null)
    setQuery(name)
  }

  const loading = championId != null && !error && (!data || data.championId !== championId)
  const found = data?.found ? data : null

  return (
    <>
      <div className="cp-search-wrap">
        <div className="cp-search">
          <IconSearch />
          <input
            type="text"
            value={query}
            placeholder={t('champion.searchPlaceholder')}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        {suggestions.length > 0 && (
          <div className="cp-suggest">
            {suggestions.map((c) => {
              const icon = getChampionIconUrl(c.id)
              return (
                <button type="button" className="cp-suggest-item" key={c.id} onClick={() => pick(c.id, c.name)}>
                  {icon && <img src={icon} alt="" width={24} height={24} />}
                  <span>{c.name}</span>
                </button>
              )
            })}
          </div>
        )}
      </div>

      {championId == null && (
        <Card>
          <p className="ui-muted">{t('champion.emptyHint')}</p>
        </Card>
      )}

      {loading && (
        <Card>
          <p className="ui-muted">{t('champion.loading')}</p>
        </Card>
      )}
      {error && (
        <Card>
          <p className="ui-error">{error}</p>
        </Card>
      )}

      {data && !found && !loading && (
        <Card>
          <p className="ui-muted">{t('champion.noCrawlerData', { name: data.championName })}</p>
        </Card>
      )}

      {found && (
        <>
          <Card>
            <div className="cp-head">
              <div className="cp-head-id">
                {getChampionIconUrl(found.championId) && (
                  <img src={getChampionIconUrl(found.championId)!} alt="" width={48} height={48} className="cp-head-icon" />
                )}
                <span className="cp-head-name">{found.championName}</span>
              </div>
              <div className="cp-roles">
                {found.roles.map((r) => (
                  <button
                    key={r.role}
                    type="button"
                    className={`cp-role-tab${r.role === found.role ? ' is-active' : ''}`}
                    onClick={() => setRoleSel(r.role)}
                  >
                    {t(`roles.${r.role}`, { defaultValue: r.role })}
                  </button>
                ))}
              </div>
              <StatRow>
                <Stat label={t('champion.winrate')} value={pct(found.winRate)} tone="accent" sub={t('champion.games', { count: found.games })} />
                <Stat label={t('champion.pickrate')} value={pct(found.pickRate)} />
                <Stat label={t('champion.banrate')} value={pct(found.banRate)} />
              </StatRow>
            </div>
          </Card>

          <div className="cp-grid">
            <MatchupList title={t('champion.strongAgainst')} items={found.strongAgainst} />
            <MatchupList title={t('champion.weakAgainst')} items={found.weakAgainst} />
          </div>

          <SynergyList items={found.synergies} />

          <RunesCard runes={found.runes} />

          <Card title={t('champion.build.title')}>
            {found.boots.length > 0 && (
              <div className="cp-slot">
                <span className="cp-slot-label">{t('champion.build.boots')}</span>
                <ItemRow items={found.boots} />
              </div>
            )}
            {found.buildPath.length === 0 && found.boots.length === 0 ? (
              <p className="ui-dim">{t('champion.build.noData')}</p>
            ) : (
              found.buildPath.map((s) => (
                <div className="cp-slot" key={s.slot}>
                  <span className="cp-slot-label">{t('champion.build.slotItem', { slot: s.slot })}</span>
                  <ItemRow items={s.items} />
                </div>
              ))
            )}
          </Card>
          <Card title={t('champion.finalItems')}>
            <ItemRow items={found.finalItems} />
          </Card>
        </>
      )}
    </>
  )
}

export default ChampionPage
