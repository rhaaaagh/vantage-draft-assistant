import React, { useEffect, useId, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { fetchMatchDetail } from '../api/matchDetailApi'
import type { MatchDetailPlayer, MatchDetailResponse } from '../api/matchDetailApi'
import { getCatalogVersion, getChampionIconUrl, getItemIconUrl, useChampionCatalog } from '../api/championCatalog'
import { getRuneIconUrl, getRuneInfo, useRuneCatalog } from '../api/runeCatalog'
import { Card } from '../components/ui'
import { IconBack } from '../components/icons'
import './MatchBreakdown.css'

/** Короткая подпись роли через общий словарь match.roleShort. */
function roleShort(t: TFunction, role: string): string {
  const key = role.toUpperCase()
  return t(`match.roleShort.${key}`, { defaultValue: '' })
}

/** Подпись очереди: известные id из словаря, остальное — fallback. */
function queueLabel(t: TFunction, id: number): string {
  const known = [400, 420, 430, 440, 450, 700]
  return known.includes(id) ? t(`match.queue.${id}`) : t('match.queue.fallback', { id })
}

type Metric = 'gold' | 'xp' | 'cs'

function getCreds(): { apiKey: string; region: string } {
  if (typeof window === 'undefined') return { apiKey: '', region: 'ru' }
  return {
    apiKey: window.localStorage.getItem('lolda_riot_api_key') ?? '',
    region: window.localStorage.getItem('lolda_region') ?? 'ru',
  }
}

function fmtK(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

function fmtDuration(sec: number): string {
  const m = Math.floor(sec / 60)
  const s = sec % 60
  return `${m}:${String(s).padStart(2, '0')}`
}

function fmtClock(minute: number, second: number): string {
  return `${minute}:${String(second).padStart(2, '0')}`
}

function monsterLabel(t: TFunction, detail: string): string {
  const d = detail.toUpperCase()
  if (d.includes('BARON')) return t('match.monster.baron')
  if (d.includes('HERALD')) return t('match.monster.herald')
  if (d.includes('HORDE') || d.includes('GRUB')) return t('match.monster.grubs')
  if (d.includes('ELDER')) return t('match.monster.elder')
  if (d.includes('DRAGON')) {
    const elKey = d.includes('FIRE')
      ? 'fire'
      : d.includes('WATER')
        ? 'water'
        : d.includes('EARTH')
          ? 'earth'
          : d.includes('AIR')
            ? 'air'
            : d.includes('HEXTECH')
              ? 'hextech'
              : d.includes('CHEMTECH')
                ? 'chemtech'
                : ''
    return elKey
      ? t('match.monster.dragonOf', { element: t(`match.monster.element.${elKey}`) })
      : t('match.monster.dragon')
  }
  return t('match.monster.generic')
}

function monsterIcon(detail: string): string {
  const d = detail.toUpperCase()
  if (d.includes('BARON')) return '🟪'
  if (d.includes('HERALD')) return '🦅'
  if (d.includes('HORDE') || d.includes('GRUB')) return '🐛'
  if (d.includes('ELDER')) return '✨'
  if (d.includes('DRAGON')) return '🐉'
  return '👾'
}

function buildingLabel(t: TFunction, detail: string): string {
  const d = detail.toUpperCase()
  if (d.includes('INHIBITOR')) return t('match.building.inhibitor')
  if (d.includes('NEXUS')) return t('match.building.nexusTower')
  if (d.includes('BASE')) return t('match.building.baseTower')
  if (d.includes('INNER')) return t('match.building.innerTower')
  if (d.includes('OUTER')) return t('match.building.outerTower')
  return t('match.building.tower')
}

/** Кэш разборов по matchId — повторное открытие мгновенно. */
const detailCache = new Map<string, MatchDetailResponse>()

function PlayerRow({
  p,
  durationSec,
  open,
  onClick,
  t,
}: {
  p: MatchDetailPlayer
  durationSec: number
  open: boolean
  onClick: () => void
  t: TFunction
}) {
  const champIcon = getChampionIconUrl(p.championId)
  const keystone = getRuneIconUrl(p.keystoneId)
  const subStyle = getRuneIconUrl(p.subStyleId)
  const kp = Math.round(p.killParticipation * 100)
  const dmgPct = Math.round(p.teamDamagePercentage * 100)
  const csPerMin = durationSec > 0 ? (p.cs / (durationSec / 60)).toFixed(1) : '0'

  return (
    <div
      className={`mb-row${p.isTarget ? ' is-target' : ''}${open ? ' is-open' : ''}`}
      onClick={onClick}
      role="button"
      tabIndex={0}
      aria-expanded={open}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onClick()
        }
      }}
    >
      <div className="mb-runes">
        {keystone && <img src={keystone} alt="" width={24} height={24} className="mb-rune-key" />}
        {subStyle && <img src={subStyle} alt="" width={16} height={16} className="mb-rune-sub" />}
      </div>

      <div className="mb-champ">
        {champIcon && <img src={champIcon} alt="" width={36} height={36} className="mb-champ-icon" />}
        <span className="mb-champ-lvl num">{p.champLevel}</span>
      </div>

      <div className="mb-player">
        <span className="mb-player-name" title={p.riotId}>{p.riotId}</span>
        <span className="mb-player-role">{roleShort(t, p.role)}</span>
      </div>

      <div className="mb-kda">
        <span className="num">
          {p.kills}<span className="mb-sep">/</span>
          <span className="mb-deaths">{p.deaths}</span><span className="mb-sep">/</span>
          {p.assists}
        </span>
        <span className="mb-sub num">{t('match.kp', { pct: kp })}</span>
      </div>

      <div className="mb-cs">
        <span className="num">{p.cs}</span>
        <span className="mb-sub num">{t('match.csPerMin', { value: csPerMin })}</span>
      </div>

      <div className="mb-gold num">{fmtK(p.gold)}</div>

      <div className="mb-dmg">
        <span className="num">{fmtK(p.damageToChampions)}</span>
        <span className="mb-sub num">{t('match.dmgPct', { pct: dmgPct })}</span>
      </div>

      <div className="mb-items">
        {p.items.map((id, i) => (
          <img key={`${id}-${i}`} src={getItemIconUrl(id)} alt="" width={20} height={20} />
        ))}
      </div>
    </div>
  )
}

/**
 * График метрики-преимущества: нулевая база по центру, расходящаяся заливка
 * (зелёным вверх / красным вниз, как в клиенте LoL). Рисуется в пиксельных
 * координатах, поэтому маркеры — ровные кружки. scrubIndex рисует вертикаль.
 */
function MetricChart({ values, scrubIndex, t }: { values: number[]; scrubIndex: number; t: TFunction }) {
  const ref = useRef<HTMLDivElement>(null)
  const [w, setW] = useState(640)
  const clipId = useId()
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const ro = new ResizeObserver((entries) => {
      const cw = entries[0]?.contentRect.width
      if (cw && cw > 0) setW(cw)
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  if (values.length < 2) {
    return <div ref={ref} className="mb-gold" />
  }

  const H = 150
  const padX = 8
  const padY = 12
  const maxAbs = Math.max(1, ...values.map((v) => Math.abs(v)))
  const innerW = Math.max(1, w - padX * 2)
  const stepX = innerW / (values.length - 1)
  const zeroY = H / 2
  const yOf = (v: number) => zeroY - (v / maxAbs) * (H / 2 - padY)
  const pts = values.map((v, i) => [padX + i * stepX, yOf(v)] as const)
  const line = pts.map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)},${y.toFixed(1)}`).join(' ')
  const area = `${line} L${pts[pts.length - 1][0].toFixed(1)},${zeroY} L${pts[0][0].toFixed(1)},${zeroY} Z`
  const last = pts[pts.length - 1]
  const lastUp = values[values.length - 1] >= 0
  const si = Math.max(0, Math.min(scrubIndex, pts.length - 1))
  const scrubPt = pts[si]
  const scrubUp = values[si] >= 0

  return (
    <div ref={ref} className="mb-gold">
      <svg width={w} height={H} role="img" aria-label={t('match.advantageChartAria')}>
        <defs>
          <clipPath id={`up-${clipId}`}>
            <rect x="0" y="0" width={w} height={zeroY} />
          </clipPath>
          <clipPath id={`dn-${clipId}`}>
            <rect x="0" y={zeroY} width={w} height={H - zeroY} />
          </clipPath>
        </defs>
        <path d={area} className="mb-gold-up" clipPath={`url(#up-${clipId})`} />
        <path d={area} className="mb-gold-dn" clipPath={`url(#dn-${clipId})`} />
        <line x1={padX} y1={zeroY} x2={w - padX} y2={zeroY} className="mb-gold-zero" />
        <path d={line} className="mb-gold-line" />
        <line x1={scrubPt[0]} y1={0} x2={scrubPt[0]} y2={H} className="mb-gold-scrub" />
        <circle cx={scrubPt[0]} cy={scrubPt[1]} r={4} className={`mb-gold-dot ${scrubUp ? 'is-up' : 'is-down'}`} />
        <circle cx={last[0]} cy={last[1]} r={3} className={`mb-gold-dot ${lastUp ? 'is-up' : 'is-down'}`} />
      </svg>
    </div>
  )
}

/** Полные руны игрока с подсказками (имя + что делает). */
function RunePage({ p, t }: { p: MatchDetailPlayer; t: TFunction }) {
  const rune = (id: number, key = false) => {
    const url = getRuneIconUrl(id)
    if (!url) return null
    const info = getRuneInfo(id)
    const title = info ? (info.desc ? `${info.name} — ${info.desc}` : info.name) : ''
    return (
      <img
        key={id}
        src={url}
        alt={info?.name ?? ''}
        title={title}
        className={key ? 'mb-rune-big' : 'mb-rune-small'}
        width={key ? 32 : 24}
        height={key ? 32 : 24}
      />
    )
  }
  const primaryTree = getRuneIconUrl(p.primaryStyleId)
  const subTree = getRuneIconUrl(p.subStyleId)
  return (
    <div className="mb-runes-full">
      <div className="mb-rune-col">
        {primaryTree && <img src={primaryTree} alt="" className="mb-rune-tree" width={18} height={18} />}
        <div className="mb-rune-line">{p.primaryPerks.map((id, i) => rune(id, i === 0))}</div>
      </div>
      <div className="mb-rune-col">
        {subTree && <img src={subTree} alt="" className="mb-rune-tree" width={18} height={18} />}
        <div className="mb-rune-line">{p.subPerks.map((id) => rune(id))}</div>
      </div>
      <div className="mb-rune-col">
        <span className="mb-rune-tree-label">{t('match.runes.shards')}</span>
        <div className="mb-rune-line">{p.statPerks.map((id) => rune(id))}</div>
      </div>
    </div>
  )
}

/** Раскрытая панель игрока: полные руны + порядок покупок. */
function PlayerPanel({ p, t }: { p: MatchDetailPlayer; t: TFunction }) {
  return (
    <div className="mb-panel">
      <div className="mb-panel-stats">
        <span>{t('match.panel.damageTaken')}: <b className="num">{fmtK(p.damageTaken)}</b></span>
        <span>{t('match.panel.visionScore')}: <b className="num">{p.visionScore}</b></span>
        <span>{t('match.panel.wards')}: <b className="num">{p.wardsPlaced}</b> / {t('match.panel.wardsKilledWord')} <b className="num">{p.wardsKilled}</b></span>
        <span>{t('match.panel.controlWards')}: <b className="num">{p.controlWards}</b></span>
      </div>
      <div className="mb-panel-sec">
        <div className="mb-panel-title">{t('match.panel.runesTitle')}</div>
        <RunePage p={p} t={t} />
      </div>
      {p.purchases.length > 0 && (
        <div className="mb-panel-sec">
          <div className="mb-panel-title">{t('match.panel.buildOrder')}</div>
          <div className="mb-buys">
            {p.purchases.map((b, i) => (
              <div className="mb-buy" key={`${b.itemId}-${i}`}>
                <img src={getItemIconUrl(b.itemId)} alt="" width={28} height={28} />
                <span className="mb-buy-min num">{b.minute}′</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

interface KillPoint {
  x: number
  y: number
  /** Команда погибшего: true = синие (100). */
  blue: boolean
  minute: number
  second: number
  killer: string
  victim: string
}

/**
 * Карта смертей: миникарта Summoner's Rift как подложка, точки в местах гибели
 * (цвет по команде погибшего), легенда, фильтр по минуте и подсказка
 * «кто кого убил и на какой минуте».
 */
function DeathMap({ kills, maxMinute, t }: { kills: KillPoint[]; maxMinute: number; t: TFunction }) {
  const S = 320
  const MAP = 15000
  const version = getCatalogVersion()
  // Актуальный миникап SR (после реворка) из CommunityDragon; ddragon map11 —
  // старый арт, оставляем как fallback при ошибке загрузки CDragon.
  const CURRENT_MAP =
    'https://raw.communitydragon.org/latest/game/assets/maps/info/map11/2dlevelminimap_base_baron1.png'
  const legacyMap = version
    ? `https://ddragon.leagueoflegends.com/cdn/${version}/img/map/map11.png`
    : null
  const [mapUrl, setMapUrl] = useState<string | null>(CURRENT_MAP)
  // Фильтр по времени: показываем убийства до выбранной минуты включительно.
  const [upto, setUpto] = useState(maxMinute)
  const [hover, setHover] = useState<number | null>(null)
  const max = Math.max(1, maxMinute)
  const limit = Math.min(upto, max)

  if (kills.length === 0) return <p className="ui-dim">{t('match.noDeathPositions')}</p>

  const shown = kills.filter((k) => k.minute <= limit)

  return (
    <div className="mb-deathmap">
      <div className="mb-map-wrap">
        <svg
          className="mb-map"
          width={S}
          height={S}
          viewBox={`0 0 ${S} ${S}`}
          role="img"
          aria-label={t('match.deathMapAria')}
        >
          {mapUrl ? (
            <image
              href={mapUrl}
              x={0}
              y={0}
              width={S}
              height={S}
              preserveAspectRatio="none"
              onError={() => setMapUrl((s) => (s === CURRENT_MAP ? legacyMap : s))}
            />
          ) : (
            <>
              <rect x={0} y={0} width={S} height={S} className="mb-map-bg" rx={6} />
              <line x1={0} y1={S} x2={S} y2={0} className="mb-map-river" />
            </>
          )}
          <rect x={0} y={0} width={S} height={S} className="mb-map-frame" rx={6} />
          {shown.map((k, i) => {
            const cx = (k.x / MAP) * S
            const cy = S - (k.y / MAP) * S
            const active = hover === i
            return (
              <g key={i} onMouseEnter={() => setHover(i)} onMouseLeave={() => setHover(null)}>
                <circle
                  cx={cx}
                  cy={cy}
                  r={active ? 7 : 5}
                  className={k.blue ? 'mb-map-dot is-blue' : 'mb-map-dot is-red'}
                />
                <title>
                  {fmtClock(k.minute, k.second)} — {k.killer} ✕ {k.victim}
                </title>
              </g>
            )
          })}
        </svg>
        {hover != null && shown[hover] && (
          <div className="mb-map-tip">
            <span className="num">{fmtClock(shown[hover].minute, shown[hover].second)}</span>{' '}
            {shown[hover].killer} → {shown[hover].victim}
          </div>
        )}
      </div>

      <div className="mb-map-controls">
        <div className="mb-legend">
          <span className="mb-legend-item">
            <span className="mb-legend-dot is-blue" /> {t('match.legend.blueDied')}
          </span>
          <span className="mb-legend-item">
            <span className="mb-legend-dot is-red" /> {t('match.legend.redDied')}
          </span>
        </div>
        <div className="mb-map-filter">
          <input
            type="range"
            min={1}
            max={max}
            value={limit}
            onChange={(e) => setUpto(Number(e.target.value))}
            aria-label={t('match.deathFilterAria')}
          />
          <span className="mb-map-filter-time num">{t('match.deathUpTo', { minute: limit })}</span>
        </div>
        <span className="mb-map-count num">{t('match.deathCount', { count: shown.length })}</span>
      </div>
    </div>
  )
}

/** Горизонтальная гистограмма из двух значений (урон по чемпионам / полученный). */
function DamageBars({ players, t }: { players: MatchDetailPlayer[]; t: TFunction }) {
  const maxDealt = Math.max(1, ...players.map((p) => p.damageToChampions))
  const maxTaken = Math.max(1, ...players.map((p) => p.damageTaken))
  return (
    <div className="mb-dmg-chart">
      <div className="mb-legend">
        <span className="mb-legend-item">
          <span className="mb-legend-swatch is-dealt" /> {t('match.legend.damageDealt')}
        </span>
        <span className="mb-legend-item">
          <span className="mb-legend-swatch is-taken" /> {t('match.legend.damageTaken')}
        </span>
      </div>
      {players.map((p, i) => {
        const icon = getChampionIconUrl(p.championId)
        return (
          <div className="mb-dmg-row" key={p.puuid || i}>
            <div className="mb-dmg-champ">
              {icon && <img src={icon} alt="" width={22} height={22} />}
              <span className="mb-dmg-name" title={p.riotId}>
                {p.championName}
              </span>
            </div>
            <div className="mb-dmg-bars">
              <div className="mb-dmg-bar-line" title={t('match.damageChart.dealtTitle', { value: p.damageToChampions.toLocaleString('ru') })}>
                <div
                  className="mb-dmg-bar is-dealt"
                  style={{ width: `${(p.damageToChampions / maxDealt) * 100}%` }}
                />
                <span className="mb-dmg-val num">{fmtK(p.damageToChampions)}</span>
              </div>
              <div className="mb-dmg-bar-line" title={t('match.damageChart.takenTitle', { value: p.damageTaken.toLocaleString('ru') })}>
                <div
                  className="mb-dmg-bar is-taken"
                  style={{ width: `${(p.damageTaken / maxTaken) * 100}%` }}
                />
                <span className="mb-dmg-val num">{fmtK(p.damageTaken)}</span>
              </div>
            </div>
          </div>
        )
      })}
    </div>
  )
}

/** Сравнение игроков по ролям: синий слева, красный справа, расходящиеся полосы. */
function PlayerCompare({ blue, red, t }: { blue: MatchDetailPlayer[]; red: MatchDetailPlayer[]; t: TFunction }) {
  const COMPARE_METRICS: { id: keyof MatchDetailPlayer; label: string; fmt: (n: number) => string }[] = [
    { id: 'damageToChampions', label: t('match.metric.damage'), fmt: fmtK },
    { id: 'gold', label: t('match.metric.gold'), fmt: fmtK },
    { id: 'cs', label: t('match.metric.cs'), fmt: (n) => String(n) },
    { id: 'visionScore', label: t('match.metric.vision'), fmt: (n) => String(n) },
  ]
  const ROLE_ORDER = ['TOP', 'JUNGLE', 'MIDDLE', 'MID', 'BOTTOM', 'BOT', 'UTILITY', 'SUPPORT']
  const roleRank = (r: string) => {
    const idx = ROLE_ORDER.indexOf(r.toUpperCase())
    return idx < 0 ? 99 : idx
  }
  const sortByRole = (arr: MatchDetailPlayer[]) => [...arr].sort((a, b) => roleRank(a.role) - roleRank(b.role))
  const b = sortByRole(blue)
  const r = sortByRole(red)
  const rows = Math.min(b.length, r.length)

  return (
    <div className="mb-compare">
      {Array.from({ length: rows }, (_, i) => {
        const bp = b[i]
        const rp = r[i]
        const bIcon = getChampionIconUrl(bp.championId)
        const rIcon = getChampionIconUrl(rp.championId)
        return (
          <div className="mb-cmp-block" key={i}>
            <div className="mb-cmp-head">
              <span className="mb-cmp-champ is-blue">
                {bIcon && <img src={bIcon} alt="" width={20} height={20} />}
                {bp.championName}
              </span>
              <span className="mb-cmp-role">{roleShort(t, bp.role) || bp.role}</span>
              <span className="mb-cmp-champ is-red">
                {rp.championName}
                {rIcon && <img src={rIcon} alt="" width={20} height={20} />}
              </span>
            </div>
            {COMPARE_METRICS.map((m) => {
              const bv = Number(bp[m.id]) || 0
              const rv = Number(rp[m.id]) || 0
              const total = bv + rv || 1
              const bPct = (bv / total) * 100
              return (
                <div className="mb-cmp-row" key={String(m.id)}>
                  <span className="mb-cmp-num num">{m.fmt(bv)}</span>
                  <div className="mb-cmp-bar" title={m.label}>
                    <div className="mb-cmp-fill is-blue" style={{ width: `${bPct}%` }} />
                    <div className="mb-cmp-fill is-red" style={{ width: `${100 - bPct}%` }} />
                    <span className="mb-cmp-label">{m.label}</span>
                  </div>
                  <span className="mb-cmp-num is-right num">{m.fmt(rv)}</span>
                </div>
              )
            })}
          </div>
        )
      })}
    </div>
  )
}

interface ObjectiveItem {
  minute: number
  second: number
  /** Команда, забравшая объект (для зданий — команда-разрушитель). */
  teamId: number
  label: string
  icon: string
}

/** Таймлайн объектов: драконы/барон/вестник/грибы/башни — кто и когда взял. */
function ObjectiveTimeline({ items, maxMinute, t }: { items: ObjectiveItem[]; maxMinute: number; t: TFunction }) {
  if (items.length === 0) return <p className="ui-dim">{t('match.noObjectives')}</p>
  const max = Math.max(1, maxMinute)
  return (
    <div className="mb-obj">
      <div className="mb-legend">
        <span className="mb-legend-item">
          <span className="mb-legend-dot is-blue" /> {t('match.legend.blueTeam')}
        </span>
        <span className="mb-legend-item">
          <span className="mb-legend-dot is-red" /> {t('match.legend.redTeam')}
        </span>
      </div>
      <div className="mb-obj-track">
        {/* Минутные деления */}
        {Array.from({ length: Math.floor(max / 5) + 1 }, (_, i) => i * 5).map((mn) => (
          <div className="mb-obj-tick" key={mn} style={{ left: `${(mn / max) * 100}%` }}>
            <span className="mb-obj-tick-label num">{mn}</span>
          </div>
        ))}
        {items.map((o, i) => (
          <div
            className={`mb-obj-mark ${o.teamId === 100 ? 'is-blue' : 'is-red'}`}
            key={i}
            style={{ left: `${(o.minute / max) * 100}%` }}
            title={t('match.objectiveTitle', {
              time: fmtClock(o.minute, o.second),
              label: o.label,
              side: o.teamId === 100 ? t('match.side.blue') : t('match.side.red'),
            })}
          >
            <span className="mb-obj-icon">{o.icon}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

interface MatchBreakdownProps {
  matchId: string
  targetPuuid: string | null
  onBack: () => void
}

export const MatchBreakdown: React.FC<MatchBreakdownProps> = ({ matchId, targetPuuid, onBack }) => {
  const { t } = useTranslation()
  useChampionCatalog()
  useRuneCatalog()

  const METRICS: { id: Metric; label: string }[] = [
    { id: 'gold', label: t('match.metric.gold') },
    { id: 'xp', label: t('match.metric.xp') },
    { id: 'cs', label: t('match.metric.cs') },
  ]

  const [data, setData] = useState<MatchDetailResponse | null>(detailCache.get(matchId) ?? null)
  const [error, setError] = useState<string | null>(null)
  const [openPlayers, setOpenPlayers] = useState<Set<string>>(new Set())
  const [metric, setMetric] = useState<Metric>('gold')
  const [scrub, setScrub] = useState<number | null>(null)
  const loading = !data && !error

  const togglePlayer = (puuid: string) =>
    setOpenPlayers((prev) => {
      const next = new Set(prev)
      if (next.has(puuid)) next.delete(puuid)
      else next.add(puuid)
      return next
    })

  useEffect(() => {
    if (detailCache.has(matchId)) return
    let active = true
    const { apiKey, region } = getCreds()
    fetchMatchDetail(apiKey, region, matchId, targetPuuid)
      .then((res) => {
        if (!active) return
        detailCache.set(matchId, res)
        setData(res)
      })
      .catch((e) => {
        if (active) setError(String(e))
      })
    return () => {
      active = false
    }
  }, [matchId, targetPuuid])

  const target = data?.players.find((p) => p.isTarget)
  const targetTeam = (target?.teamId ?? data?.teams[0]?.teamId) ?? 100
  const headerTeam = data?.teams.find((t) => t.teamId === targetTeam)
  const outcome = headerTeam ? (headerTeam.win ? t('match.outcome.win') : t('match.outcome.loss')) : ''

  const frames = data?.frames ?? []
  const players = data?.players ?? []

  // Преимущество выбранной команды по выбранной метрике по минутам.
  const advantage = useMemo(() => {
    const fr = data?.frames ?? []
    const pl = data?.players ?? []
    return fr.map((f) => {
      const arr = metric === 'gold' ? f.gold : metric === 'xp' ? f.xp : f.cs
      let mine = 0
      let theirs = 0
      pl.forEach((p, i) => {
        const v = arr[i] ?? 0
        if (p.teamId === targetTeam) mine += v
        else theirs += v
      })
      return mine - theirs
    })
  }, [data, metric, targetTeam])

  const maxIdx = frames.length - 1
  const scrubIdx = maxIdx < 0 ? 0 : Math.min(scrub ?? maxIdx, maxIdx)
  const snap = frames[scrubIdx]

  const playerByPid = (pid: number): MatchDetailPlayer | undefined => players[pid - 1]

  const renderTeam = (teamId: number) => {
    if (!data) return null
    const team = data.teams.find((t) => t.teamId === teamId)
    const teamPlayers = data.players.filter((p) => p.teamId === teamId)
    if (!team || teamPlayers.length === 0) return null
    return (
      <div className="mb-team">
        <div className={`mb-team-head ${team.win ? 'is-win' : 'is-loss'}`}>
          <span className="mb-team-result">{team.win ? t('match.outcome.win') : t('match.outcome.loss')}</span>
          <span className="mb-team-side">{teamId === 100 ? t('match.team.side.blue') : t('match.team.side.red')}</span>
          <span className="mb-team-totals num">
            {t('match.team.totals', {
              kills: team.kills,
              deaths: team.deaths,
              assists: team.assists,
              gold: fmtK(team.gold),
              dragon: team.dragon,
              herald: team.herald,
              baron: team.baron,
              tower: team.tower,
            })}
          </span>
        </div>
        <div className="mb-rows">
          {teamPlayers.map((p) => {
            const key = p.puuid || String(p.championId)
            const isOpen = openPlayers.has(key)
            return (
              <React.Fragment key={key}>
                <PlayerRow
                  p={p}
                  durationSec={data.gameDuration}
                  open={isOpen}
                  onClick={() => togglePlayer(key)}
                  t={t}
                />
                {isOpen && <PlayerPanel p={p} t={t} />}
              </React.Fragment>
            )
          })}
        </div>
      </div>
    )
  }

  // Срез метрик на выбранной минуте (ползунок) для одной команды.
  const renderSnapTeam = (teamId: number) => {
    if (!snap) return null
    const idxs = players
      .map((p, i) => ({ p, i }))
      .filter((x) => x.p.teamId === teamId)
    const metricArr = metric === 'gold' ? snap.gold : metric === 'xp' ? snap.xp : snap.cs
    const total = idxs.reduce((s, x) => s + (metricArr[x.i] ?? 0), 0)
    return (
      <div className="mb-snap-team">
        <div className={`mb-snap-head ${teamId === targetTeam ? 'is-mine' : ''}`}>
          {t('match.teamSnap', { side: teamId === 100 ? t('match.side.blue') : t('match.side.red'), total: fmtK(total) })}
        </div>
        {idxs.map(({ p, i }) => {
          const icon = getChampionIconUrl(p.championId)
          return (
            <div className="mb-snap-row" key={p.puuid || i}>
              {icon && <img src={icon} alt="" width={20} height={20} className="mb-snap-icon" />}
              <span className="mb-snap-lvl num">{snap.level[i] ?? 0}</span>
              <span className="mb-snap-val num">{fmtK(metricArr[i] ?? 0)}</span>
            </div>
          )
        })}
      </div>
    )
  }

  return (
    <div className="mb">
      <div className="mb-topbar">
        <button type="button" className="mb-back" onClick={onBack}>
          <IconBack />
          <span>{t('match.back')}</span>
        </button>
        <div className="mb-title">
          <span className="mb-title-main">{t('match.breakdownTitle')}</span>
          {data && (
            <span className={`mb-title-sub ${headerTeam?.win ? 'is-win' : 'is-loss'}`}>
              {t('match.header', {
                outcome,
                queue: queueLabel(t, data.queueId),
                duration: fmtDuration(data.gameDuration),
                patch: data.patch,
              })}
            </span>
          )}
        </div>
      </div>

      {loading && (
        <Card>
          <p className="ui-muted">{t('match.loading')}</p>
        </Card>
      )}
      {error && (
        <Card>
          <p className="ui-error">{error}</p>
        </Card>
      )}

      {data && frames.length >= 2 && (
        <Card title={t('match.timingsTitle')}>
          <div className="mb-metric-tabs">
            {METRICS.map((m) => (
              <button
                key={m.id}
                type="button"
                className={`mb-metric-tab${metric === m.id ? ' is-active' : ''}`}
                onClick={() => setMetric(m.id)}
              >
                {m.label}
              </button>
            ))}
          </div>

          <MetricChart values={advantage} scrubIndex={scrubIdx} t={t} />

          <div className="mb-scrub">
            <input
              type="range"
              min={0}
              max={maxIdx}
              value={scrubIdx}
              onChange={(e) => setScrub(Number(e.target.value))}
            />
            <span className="mb-scrub-time num">{snap ? t('match.scrubMinute', { minute: snap.minute }) : ''}</span>
          </div>

          {snap && (
            <div className="mb-snap">
              {renderSnapTeam(100)}
              {renderSnapTeam(200)}
            </div>
          )}
        </Card>
      )}

      {data && (
        <Card>
          <p className="mb-hint">{t('match.playerHint')}</p>
          {renderTeam(100)}
          <div className="mb-team-gap" />
          {renderTeam(200)}
        </Card>
      )}

      {data && players.length > 0 && (
        <Card title={t('match.damageCardTitle')}>
          <DamageBars players={players} t={t} />
        </Card>
      )}

      {data && (
        <Card title={t('match.compareCardTitle')}>
          <PlayerCompare
            blue={players.filter((p) => p.teamId === 100)}
            red={players.filter((p) => p.teamId === 200)}
            t={t}
          />
        </Card>
      )}

      {data &&
        (() => {
          const kills = data.events
            .filter((e) => e.kind === 'kill' && e.x > 0 && e.y > 0)
            .map((e) => {
              const killer = playerByPid(e.killerId)
              const victim = playerByPid(e.victimId)
              return {
                x: e.x,
                y: e.y,
                blue: (victim?.teamId ?? 100) === 100,
                minute: e.minute,
                second: e.second,
                killer: killer?.championName ?? '?',
                victim: victim?.championName ?? '?',
              }
            })
          if (kills.length === 0) return null
          const maxMinute = Math.max(...kills.map((k) => k.minute), 1)
          return (
            <Card title={t('match.deathMapTitle')}>
              <DeathMap kills={kills} maxMinute={maxMinute} t={t} />
            </Card>
          )
        })()}

      {data &&
        (() => {
          const objs: ObjectiveItem[] = data.events
            .filter((e) => e.kind === 'monster' || e.kind === 'building')
            .map((e) => {
              if (e.kind === 'monster') {
                // teamId — команда-убийца.
                return {
                  minute: e.minute,
                  second: e.second,
                  teamId: e.teamId,
                  label: monsterLabel(t, e.detail),
                  icon: monsterIcon(e.detail),
                }
              }
              // building: e.teamId — команда, потерявшая строение → объект взяла другая.
              const taker = e.teamId === 100 ? 200 : 100
              return {
                minute: e.minute,
                second: e.second,
                teamId: taker,
                label: buildingLabel(t, e.detail),
                icon: e.detail.toUpperCase().includes('INHIBITOR') ? '⬛' : '🏰',
              }
            })
          if (objs.length === 0) return null
          const maxMinute = Math.max(...objs.map((o) => o.minute), Math.ceil(data.gameDuration / 60))
          return (
            <Card title={t('match.objectiveTimelineTitle')}>
              <ObjectiveTimeline items={objs} maxMinute={maxMinute} t={t} />
            </Card>
          )
        })()}

      {data && data.events.length > 0 && (
        <Card title={t('match.eventsTitle')}>
          <div className="mb-events">
            {data.events.map((ev, i) => {
              const time = fmtClock(ev.minute, ev.second)
              if (ev.kind === 'kill') {
                const killer = playerByPid(ev.killerId)
                const victim = playerByPid(ev.victimId)
                const side = killer?.teamId === 100 ? 'is-blue' : 'is-red'
                return (
                  <div className="mb-event" key={i}>
                    <span className="mb-event-time num">{time}</span>
                    <span className={`mb-event-dot ${side}`} />
                    <span className="mb-event-text">
                      {t('match.event.kill', {
                        killer: killer?.championName ?? '?',
                        victim: victim?.championName ?? '?',
                      })}
                    </span>
                  </div>
                )
              }
              if (ev.kind === 'monster') {
                const side = ev.teamId === 100 ? 'is-blue' : 'is-red'
                return (
                  <div className="mb-event" key={i}>
                    <span className="mb-event-time num">{time}</span>
                    <span className={`mb-event-dot ${side}`} />
                    <span className="mb-event-text">
                      {t('match.event.monster', {
                        monster: monsterLabel(t, ev.detail),
                        side: ev.teamId === 100 ? t('match.side.blue') : t('match.side.red'),
                      })}
                    </span>
                  </div>
                )
              }
              // building: teamId — команда, потерявшая строение.
              const side = ev.teamId === 100 ? 'is-red' : 'is-blue'
              return (
                <div className="mb-event" key={i}>
                  <span className="mb-event-time num">{time}</span>
                  <span className={`mb-event-dot ${side}`} />
                  <span className="mb-event-text">
                    {t('match.event.building', {
                      building: buildingLabel(t, ev.detail),
                      side: ev.teamId === 100 ? t('match.sideGen.blue') : t('match.sideGen.red'),
                    })}
                  </span>
                </div>
              )
            })}
          </div>
        </Card>
      )}
    </div>
  )
}

export default MatchBreakdown
