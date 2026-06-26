import React, { useEffect, useMemo, useState } from 'react'
import { Trans, useTranslation } from 'react-i18next'
import {
  scoutActiveGame,
  scoutMyActiveGame,
  scoutPlayerHistory,
  fetchPersonalMatchup,
  fetchMetaMatchup,
  fetchPlayerPatterns,
} from '../api/scoutApi'
import type { PlayerGame, ScoutPlayer, PersonalMatchup, MetaMatchup, PlayerPatterns, MapPoint } from '../api/scoutApi'
import { fetchChampionMastery } from '../api/masteryApi'
import type { ChampionMasteryEntry } from '../api/masteryApi'
import { getChampionIconUrl, useChampionCatalog } from '../api/championCatalog'
import { assignRoles, formatRank, ROLE_ORDER } from '../utils/roles'
import type { Role } from '../utils/roles'
import { parseRiotId } from '../utils/riotId'
import { usePatch } from '../components/patchContext'
import { Minimap } from '../components/Minimap'
import type { MinimapPoint, MinimapRoute } from '../components/Minimap'

/** Глубина истории на игрока. Достаточно, чтобы агрегировать топ-чемпионов. */
const HISTORY_COUNT = 10
/** Сколько последних матчей сканировать для личного матчапа (дорого: 1 запрос/матч). */
const MATCHUP_COUNT = 30
/** Сколько топ-чемпионов показывать в мини-сводке. */
const TOP_CHAMPS = 3
/** Сколько последних игр анализировать для паттернов (дорого: ~2 запроса/игру). */
const PATTERNS_COUNT = 6

/** Развёрнутый блок паттернов игрока: архетипы + миникарта (смерти/хитмап/лес). */
/** Палитра для лесных маршрутов по матчам (цвет = матч). */
const ROUTE_COLORS = ['#56b6ff', '#e0507a', '#f5c542', '#5fd38a', '#b083f0', '#ff8a5c', '#3fd0d6', '#e87fd0']

/** Игровое время точки в формате M:SS (atSeconds приоритетнее минут). */
function fmtGameTime(p: MapPoint): string {
  const s = p.atSeconds ?? p.minute * 60
  const m = Math.floor(s / 60)
  const r = Math.round(s % 60)
  return `${m}:${r.toString().padStart(2, '0')}`
}

/** Грубая зона карты SR по долям (0..1, y вверх): мид / река / верх / низ. Возвращает i18n-ключ. */
function deathZoneKey(x: number, y: number): 'scout.zoneMid' | 'scout.zoneRiver' | 'scout.zoneTop' | 'scout.zoneBot' {
  if (Math.abs(y - x) < 0.1) return 'scout.zoneMid'
  if (Math.abs(x + y - 1) < 0.1) return 'scout.zoneRiver'
  return y > x ? 'scout.zoneTop' : 'scout.zoneBot'
}

function PatternsPanel({ patterns }: { patterns: PlayerPatterns }) {
  const { t } = useTranslation()
  // Выбранный лесной маршрут: 'all' — все сразу, либо индекс матча (по одному).
  const [selectedRoute, setSelectedRoute] = useState<number | 'all'>('all')
  // То же для карты перемещений (полные треки позиций по матчам).
  const [selectedMove, setSelectedMove] = useState<number | 'all'>('all')
  if (patterns.gamesAnalyzed === 0) {
    return <p className="scout-history-loading">{t('scout.notEnoughMatches')}</p>
  }
  // Смерти по возрастанию времени → нумеруем 1..N (точка + строка таблицы).
  const sortedDeaths = [...patterns.deathPoints].sort(
    (a, b) => (a.atSeconds ?? a.minute * 60) - (b.atSeconds ?? b.minute * 60),
  )
  const deaths: MinimapPoint[] = sortedDeaths.map((p, i) => ({
    x: p.x,
    y: p.y,
    minute: p.minute,
    kind: 'death',
    label: String(i + 1),
    title: t('scout.deathTitle', { num: i + 1, time: fmtGameTime(p), zone: t(deathZoneKey(p.x, p.y)) }),
  }))
  const isJungle = patterns.mainRole === 'JUNGLE'
  // Точные лесные маршруты ПО МАТЧАМ — каждая игра своим цветом (усреднение давало
  // бессмысленную «кашу»: пути разных игр не совпадают). Со стрелками направления.
  const routeItems: { route: MinimapRoute; color: string; label: string }[] = isJungle
    ? patterns.jungleRoutes
        .filter((r) => r.path.length >= 2)
        .map((r, i) => {
          const color = ROUTE_COLORS[i % ROUTE_COLORS.length]
          const sideTxt =
            r.startSide === 'OWN'
              ? t('scout.matchSideOwn')
              : r.startSide === 'ENEMY'
                ? t('scout.matchSideInvade')
                : ''
          const label = `${t('scout.match', { num: i + 1 })}${sideTxt ? ` · ${sideTxt}` : ''}`
          return {
            color,
            label,
            route: {
              points: r.path.map((p) => ({ x: p.x, y: p.y, minute: p.minute })),
              color,
              title: label,
              directed: true,
            },
          }
        })
    : []
  const ownPct = Math.round(patterns.ownStartFraction * 100)
  // Карта перемещений: ПОЛНЫЙ трек позиций по каждой игре — линиями со стрелками,
  // цвет = матч (точные, не усреднённые). Ползунок времени раскрывает путь по минутам.
  const moveItems: { route: MinimapRoute; color: string; label: string }[] = (
    patterns.positionRoutes ?? []
  )
    .filter((path) => path.length >= 2)
    .map((path, i) => {
      const color = ROUTE_COLORS[i % ROUTE_COLORS.length]
      const label = t('scout.match', { num: i + 1 })
      return {
        color,
        label,
        route: {
          points: path.map((p) => ({ x: p.x, y: p.y, minute: p.minute })),
          color,
          title: label,
          directed: true,
        },
      }
    })
  return (
    <div className="scout-patterns">
      <div className="scout-patterns-head">
        {t('scout.gamesAnalyzed', { count: patterns.gamesAnalyzed })}
        {patterns.mainRole &&
          t('scout.rolePart', { role: t(`roles.${patterns.mainRole}`, { defaultValue: patterns.mainRole }) })}
        {patterns.mainChampionName && t('scout.mostPlayedPart', { champion: patterns.mainChampionName })}
      </div>

      {patterns.archetypes.length > 0 ? (
        <div className="scout-archetypes">
          {patterns.archetypes.map((a, i) => (
            <div className="scout-archetype" key={i} title={a.explanation}>
              <span className="scout-archetype-label">{a.label}</span>
              <span className="scout-archetype-val num">{Math.round(a.value * 100)}%</span>
              <span className="scout-archetype-exp">{a.explanation}</span>
            </div>
          ))}
        </div>
      ) : (
        <p className="scout-history-loading">{t('scout.noBehaviorLabels')}</p>
      )}

      <div className="scout-maps">
        {deaths.length > 0 && (
          <div className="scout-deaths">
            <Minimap
              points={deaths}
              size={240}
              pointRadius={9}
              showTimeSlider
              caption={t('scout.deathsCaption')}
            />
            <table className="scout-deaths-table">
              <thead>
                <tr>
                  <th>{t('scout.deathNum')}</th>
                  <th>{t('scout.deathTime')}</th>
                  <th>{t('scout.deathWhere')}</th>
                </tr>
              </thead>
              <tbody>
                {sortedDeaths.map((p, i) => (
                  <tr key={i}>
                    <td className="num">{i + 1}</td>
                    <td className="num">{fmtGameTime(p)}</td>
                    <td>{t(deathZoneKey(p.x, p.y))}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {moveItems.length > 0 && (
          <div className="scout-jungle-route">
            <Minimap
              routes={(selectedMove === 'all'
                ? moveItems
                : moveItems.filter((_, i) => i === selectedMove)
              ).map((r) => r.route)}
              size={240}
              pointRadius={3}
              showTimeSlider
              caption={t('scout.movementCaption')}
            />
            <div className="scout-route-legend">
              <button
                type="button"
                className={`scout-route-tab${selectedMove === 'all' ? ' is-active' : ''}`}
                onClick={() => setSelectedMove('all')}
              >
                {t('scout.all')}
              </button>
              {moveItems.map((r, i) => (
                <button
                  type="button"
                  key={i}
                  className={`scout-route-tab${selectedMove === i ? ' is-active' : ''}`}
                  onClick={() => setSelectedMove((s) => (s === i ? 'all' : i))}
                >
                  <span className="scout-route-swatch" style={{ background: r.color }} />
                  {r.label}
                </button>
              ))}
            </div>
          </div>
        )}
        {isJungle && (
          <div className="scout-jungle-route">
            <Minimap
              routes={(selectedRoute === 'all'
                ? routeItems
                : routeItems.filter((_, i) => i === selectedRoute)
              ).map((r) => r.route)}
              size={240}
              pointRadius={3}
              caption={t('scout.jungleRoutesCaption')}
            />
            {routeItems.length > 0 ? (
              <>
                <div className="scout-route-legend">
                  <button
                    type="button"
                    className={`scout-route-tab${selectedRoute === 'all' ? ' is-active' : ''}`}
                    onClick={() => setSelectedRoute('all')}
                  >
                    {t('scout.all')}
                  </button>
                  {routeItems.map((r, i) => (
                    <button
                      type="button"
                      key={i}
                      className={`scout-route-tab${selectedRoute === i ? ' is-active' : ''}`}
                      onClick={() => setSelectedRoute((s) => (s === i ? 'all' : i))}
                    >
                      <span className="scout-route-swatch" style={{ background: r.color }} />
                      {r.label}
                    </button>
                  ))}
                </div>
                <p className="scout-history-wr">
                  {t('scout.ownStart', { own: ownPct, enemy: 100 - ownPct })}
                </p>
              </>
            ) : (
              <p className="scout-history-loading">{t('scout.notEnoughRoute')}</p>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

type Assignment = Record<Role, string | null>
type DragInfo = { teamId: number; role: Role }

/**
 * Кэш последнего скаута на уровне модуля — переживает размонтирование при
 * переключении вкладок, поэтому возврат на Скаут не теряет текущую игру.
 * Хранится до следующего поиска. `expanded` сериализуем как массив (Set не
 * переживает копирование), `loading` не храним — он транзиентный.
 */
interface ScoutCache {
  riotId: string
  players: ScoutPlayer[] | null
  assignments: Record<number, Assignment>
  expanded: string[]
  histories: Record<string, PlayerGame[]>
  error: string | null
}
let scoutCache: ScoutCache | null = null

function getCreds(): { apiKey: string; region: string } {
  if (typeof window === 'undefined') return { apiKey: '', region: 'ru' }
  return {
    apiKey: window.localStorage.getItem('lolda_riot_api_key') ?? '',
    region: window.localStorage.getItem('lolda_region') ?? 'ru',
  }
}

function winrate(games: PlayerGame[]): number | null {
  if (games.length === 0) return null
  const w = games.filter((g) => g.win).length
  return Math.round((w / games.length) * 100)
}

/** Очки мастерства кратко: 153400 → «153k». */
function fmtMasteryPoints(n: number): string {
  return n >= 1000 ? `${Math.round(n / 1000)}k` : String(n)
}

/**
 * Состояние мастерства игрока на чемпионе текущей игры:
 * - undefined — ещё не запрашивали
 * - 'loading' — грузим
 * - 'error' — не удалось
 * - null — мастерства на этом чемпионе у игрока нет (играет «не на мейне»)
 * - ChampionMasteryEntry — найдено
 */
type MasteryState = ChampionMasteryEntry | 'loading' | 'error' | null | undefined

interface ChampSummary {
  championId: number
  championName: string
  games: number
  wins: number
}

/** Агрегирует уже загруженную историю по championId — без новых запросов. */
function topChampions(games: PlayerGame[]): ChampSummary[] {
  const m = new Map<number, ChampSummary>()
  for (const g of games) {
    const cur = m.get(g.championId) ?? {
      championId: g.championId,
      championName: g.championName,
      games: 0,
      wins: 0,
    }
    cur.games += 1
    if (g.win) cur.wins += 1
    m.set(g.championId, cur)
  }
  return Array.from(m.values())
    .sort((a, b) => b.games - a.games || b.wins - a.wins)
    .slice(0, TOP_CHAMPS)
}

function PlayerCard({
  player,
  onToggleHistory,
  expanded,
  history,
  opponent,
  matchup,
  metaMatchup,
  onLoadMatchup,
  onOpenProfile,
  patterns,
  onLoadPatterns,
  mastery,
  onLoadMastery,
}: {
  player: ScoutPlayer
  onToggleHistory: () => void
  expanded: boolean
  history: PlayerGame[] | 'loading' | undefined
  /** Оппонент по линии (для личного матчапа), или null если линия пуста. */
  opponent: ScoutPlayer | null
  matchup: PersonalMatchup | 'loading' | undefined
  /** Мета-матчап (агрегат краулера) или undefined, пока не загружен. */
  metaMatchup: MetaMatchup | undefined
  onLoadMatchup: () => void
  /** Открыть полный профиль игрока (по его Riot ID). */
  onOpenProfile: () => void
  /** Паттерны игрока: undefined — не запрошены, 'loading' — считаются. */
  patterns: PlayerPatterns | 'loading' | 'error' | undefined
  onLoadPatterns: () => void
  /** Мастерство на чемпионе текущей игры («мейн ли он на этом чемпе?»). */
  mastery: MasteryState
  onLoadMastery: () => void
}) {
  const { t } = useTranslation()
  const icon = getChampionIconUrl(player.championId)
  const name = player.riotId || player.summonerName || '—'
  const champSummary = Array.isArray(history) ? topChampions(history) : []
  return (
    <div className="scout-card">
      {/* role=button вместо <button>: нативная кнопка в Chromium/WebView2 перехватывает
          mousedown и не даёт стартовать drag родителя. div этого не делает. */}
      <div
        className="scout-card-main"
        role="button"
        tabIndex={0}
        onClick={onToggleHistory}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            onToggleHistory()
          }
        }}
      >
        {icon && <img src={icon} alt="" className="scout-card-icon" width={40} height={40} />}
        <div className="scout-card-info">
          <span className="scout-card-champ">{player.championName}</span>
          <span className="scout-card-name">{name}</span>
          <span className="scout-card-rank">
            {formatRank(player.tier, player.rank, player.leaguePoints, player.wins, player.losses)}
          </span>
        </div>
      </div>
      {expanded && (
        <div className="scout-history">
          {history === 'loading' && <span className="scout-history-loading">{t('scout.loadingHistory')}</span>}
          {Array.isArray(history) && history.length === 0 && (
            <span className="scout-history-loading">{t('scout.noRecentGames')}</span>
          )}
          {Array.isArray(history) && history.length > 0 && (
            <>
              <div className="scout-history-games">
                {history.map((g, i) => {
                  const gi = getChampionIconUrl(g.championId)
                  return (
                    <div
                      key={`${g.championId}-${i}`}
                      className={`scout-history-game ${g.win ? 'win' : 'loss'}`}
                      title={`${g.championName} — ${g.win ? t('scout.win') : t('scout.loss')}`}
                    >
                      {gi && <img src={gi} alt="" width={28} height={28} />}
                    </div>
                  )
                })}
              </div>
              <span className="scout-history-wr">
                {t('scout.winrateOver', { count: history.length, percent: winrate(history) })}
              </span>
            </>
          )}

          <div className="scout-mastery">
            {mastery === undefined && (
              <button type="button" className="btn-secondary" onClick={onLoadMastery}>
                {t('scout.masteryButton', { champion: player.championName })}
              </button>
            )}
            {mastery === 'loading' && (
              <span className="scout-history-loading">{t('scout.loadingMastery')}</span>
            )}
            {mastery === 'error' && (
              <span className="scout-history-loading">{t('scout.masteryLoadFailed')}</span>
            )}
            {mastery === null && (
              <span className="scout-history-wr">
                {t('scout.noMastery', { champion: player.championName })}
              </span>
            )}
            {mastery && mastery !== 'loading' && mastery !== 'error' && (
              <span className="scout-history-wr scout-mastery-val">
                {t('scout.masteryValue', {
                  champion: player.championName,
                  level: mastery.level,
                  points: fmtMasteryPoints(mastery.points),
                })}
              </span>
            )}
          </div>

          {champSummary.length > 0 && (
            <div className="scout-champs">
              <span className="scout-champs-title">{t('scout.topChampions')}</span>
              <div className="scout-champs-list">
                {champSummary.map((c) => {
                  const ci = getChampionIconUrl(c.championId)
                  return (
                    <div
                      key={c.championId}
                      className="scout-champ"
                      title={t('scout.champTooltip', { champion: c.championName, wins: c.wins, games: c.games })}
                    >
                      {ci && <img src={ci} alt="" width={24} height={24} />}
                      <span className="scout-champ-stat">
                        {Math.round((c.wins / c.games) * 100)}% · {c.games}
                      </span>
                    </div>
                  )
                })}
              </div>
            </div>
          )}

          {opponent && (
            <div className="scout-matchup">
              <div className="scout-matchup-row">
                {matchup === undefined && (
                  <button type="button" className="btn-secondary" onClick={onLoadMatchup}>
                    {t('scout.matchupButton', { champion: player.championName, opponent: opponent.championName })}
                  </button>
                )}
                {matchup === 'loading' && (
                  <span className="scout-history-loading">{t('scout.matchupLoading')}</span>
                )}
                {matchup && matchup !== 'loading' && (
                  <span className="scout-history-wr">
                    {matchup.games > 0
                      ? t('scout.matchupResult', {
                          champion: player.championName,
                          opponent: opponent.championName,
                          percent: Math.round((matchup.wins / matchup.games) * 100),
                          wins: matchup.wins,
                          games: matchup.games,
                          scanned: matchup.scanned,
                        })
                      : t('scout.matchupNone', {
                          champion: player.championName,
                          opponent: opponent.championName,
                          scanned: matchup.scanned,
                        })}
                  </span>
                )}
              </div>
              {/* Мета-матчап (агрегат краулера) — мгновенно, рядом с личным. */}
              <span className="scout-history-wr scout-meta-wr">
                {metaMatchup && metaMatchup.games > 0
                  ? t('scout.metaResult', {
                      percent: Math.round((metaMatchup.wins / metaMatchup.games) * 100),
                      games: metaMatchup.games,
                    })
                  : t('scout.metaNoData')}
              </span>
            </div>
          )}

          <button type="button" className="btn-secondary scout-open-profile" onClick={onOpenProfile}>
            {t('scout.openProfile', { name })}
          </button>

          <div className="scout-patterns-wrap">
            {patterns === undefined && (
              <button type="button" className="btn-secondary" onClick={onLoadPatterns}>
                {t('scout.patternsButton', { champion: player.championName })}
              </button>
            )}
            {patterns === 'loading' && (
              <span className="scout-history-loading">{t('scout.patternsLoading')}</span>
            )}
            {patterns === 'error' && (
              <span className="scout-history-loading">{t('scout.patternsFailed')}</span>
            )}
            {patterns && patterns !== 'loading' && patterns !== 'error' && (
              <PatternsPanel patterns={patterns} />
            )}
          </div>
        </div>
      )}
    </div>
  )
}

interface ScoutProps {
  /** Открыть полный профиль игрока по Riot ID (переключает на вкладку Профиль). */
  onOpenProfile: (query: string) => void
}

export const Scout: React.FC<ScoutProps> = ({ onOpenProfile }) => {
  const { t } = useTranslation()
  useChampionCatalog()
  const { patch } = usePatch()
  const [riotId, setRiotId] = useState(scoutCache?.riotId ?? '')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(scoutCache?.error ?? null)
  const [players, setPlayers] = useState<ScoutPlayer[] | null>(scoutCache?.players ?? null)
  const [assignments, setAssignments] = useState<Record<number, Assignment>>(scoutCache?.assignments ?? {})
  const [expanded, setExpanded] = useState<Set<string>>(new Set(scoutCache?.expanded ?? []))
  const [histories, setHistories] = useState<Record<string, PlayerGame[] | 'loading'>>(
    scoutCache?.histories ?? {},
  )
  // Личные матчапы по ключу `${puuid}:${enemyChampionId}` (транзиентно, не кэшируем между вкладками).
  const [matchups, setMatchups] = useState<Record<string, PersonalMatchup | 'loading'>>({})
  // Мета-матчапы (агрегат краулера) по ключу `${role}:${championId}:${enemyChampionId}:${patch}`.
  const [metaMatchups, setMetaMatchups] = useState<Record<string, MetaMatchup>>({})
  // Паттерны игрока по puuid. Бэкенд кэширует по puuid; тут — транзиентно.
  const [patterns, setPatterns] = useState<Record<string, PlayerPatterns | 'loading' | 'error'>>({})
  // Мастерство на чемпионе текущей игры по ключу `${puuid}:${championId}`.
  // null = у игрока нет мастерства на этом чемпионе.
  const [masteries, setMasteries] = useState<
    Record<string, ChampionMasteryEntry | 'loading' | 'error' | null>
  >({})
  const dragRef = React.useRef<DragInfo | null>(null)
  // Где была нажата мышь перед стартом drag — чтобы не таскать карточку, если
  // жест начался на ползунке/мини-карте (в WebView2 у dragstart e.target = сама
  // карточка, поэтому проверяем именно точку нажатия).
  const pointerDownRef = React.useRef<HTMLElement | null>(null)

  // Зеркалим состояние в модульный кэш (без 'loading'-историй, чтобы зависшую
  // загрузку можно было повторить после возврата на вкладку).
  useEffect(() => {
    const persistedHistories: Record<string, PlayerGame[]> = {}
    for (const [puuid, h] of Object.entries(histories)) {
      if (Array.isArray(h)) persistedHistories[puuid] = h
    }
    scoutCache = {
      riotId,
      players,
      assignments,
      expanded: Array.from(expanded),
      histories: persistedHistories,
      error,
    }
  }, [riotId, players, assignments, expanded, histories, error])

  const byPuuid = useMemo(() => {
    const m = new Map<string, ScoutPlayer>()
    players?.forEach((p) => m.set(p.puuid, p))
    return m
  }, [players])

  const teamIds = useMemo(() => {
    const ids = Array.from(new Set((players ?? []).map((p) => p.teamId)))
    ids.sort((a, b) => a - b)
    return ids
  }, [players])

  // Мета-матчапы бесплатны (агрегат краулера) — грузим автоматически для каждого
  // раскрытого слота, у которого есть оппонент по линии. Зависит от patch.
  useEffect(() => {
    if (!players) return
    for (const teamId of teamIds) {
      const otherTeam = teamIds.find((t) => t !== teamId)
      for (const role of ROLE_ORDER) {
        const puuid = assignments[teamId]?.[role] ?? null
        const player = puuid ? byPuuid.get(puuid) : undefined
        if (!player || !expanded.has(player.puuid)) continue
        const oppPuuid = otherTeam !== undefined ? (assignments[otherTeam]?.[role] ?? null) : null
        const opponent = oppPuuid ? byPuuid.get(oppPuuid) : undefined
        if (!opponent) continue
        const key = `${role}:${player.championId}:${opponent.championId}:${patch}`
        if (metaMatchups[key] !== undefined) continue
        void fetchMetaMatchup(role, player.championId, opponent.championId, patch)
          .then((res) => setMetaMatchups((prev) => ({ ...prev, [key]: res })))
          .catch(() => setMetaMatchups((prev) => ({ ...prev, [key]: { games: 0, wins: 0 } })))
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [players, assignments, expanded, teamIds, byPuuid, patch])

  /** Применяет результат скаута к состоянию (общая часть ручного и авто-скаута). */
  const applyScoutResult = (players: ScoutPlayer[]) => {
    setPlayers(players)
    const next: Record<number, Assignment> = {}
    const ids = Array.from(new Set(players.map((p) => p.teamId)))
    for (const tid of ids) {
      next[tid] = assignRoles(players.filter((p) => p.teamId === tid))
    }
    setAssignments(next)
  }

  const beginScout = () => {
    setError(null)
    setLoading(true)
    setPlayers(null)
    setExpanded(new Set())
    setHistories({})
    setMatchups({})
    setMetaMatchups({})
    setPatterns({})
    setMasteries({})
  }

  const runScout = async () => {
    const parsed = parseRiotId(riotId)
    if (!parsed) {
      setError(t('scout.invalidRiotId'))
      return
    }
    const { apiKey, region } = getCreds()
    if (!apiKey.trim()) {
      setError(t('scout.noApiKey'))
      return
    }
    beginScout()
    try {
      const res = await scoutActiveGame(apiKey, region, parsed[0], parsed[1])
      if (!res.hasGame) {
        setError(res.errorMessage ?? t('scout.noActiveGame'))
        setLoading(false)
        return
      }
      applyScoutResult(res.players)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  /** Авто-скаут своей текущей игры: PUUID берётся из LCU (без ручного ввода). */
  const runScoutMyGame = async () => {
    const { apiKey, region } = getCreds()
    if (!apiKey.trim()) {
      setError(t('scout.noApiKey'))
      return
    }
    beginScout()
    try {
      const res = await scoutMyActiveGame(apiKey, region)
      if (!res.hasGame) {
        setError(res.errorMessage ?? t('scout.noActiveGame'))
        setLoading(false)
        return
      }
      applyScoutResult(res.players)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  const toggleHistory = async (puuid: string) => {
    setExpanded((prev) => {
      const n = new Set(prev)
      if (n.has(puuid)) n.delete(puuid)
      else n.add(puuid)
      return n
    })
    if (histories[puuid] !== undefined) return
    setHistories((prev) => ({ ...prev, [puuid]: 'loading' }))
    const { apiKey, region } = getCreds()
    try {
      const res = await scoutPlayerHistory(apiKey, region, puuid, HISTORY_COUNT)
      setHistories((prev) => ({ ...prev, [puuid]: res.games }))
    } catch {
      setHistories((prev) => ({ ...prev, [puuid]: [] }))
    }
  }

  const loadMatchup = async (puuid: string, championId: number, enemyChampionId: number) => {
    const key = `${puuid}:${enemyChampionId}`
    if (matchups[key] !== undefined) return
    setMatchups((prev) => ({ ...prev, [key]: 'loading' }))
    const { apiKey, region } = getCreds()
    try {
      const res = await fetchPersonalMatchup(apiKey, region, puuid, championId, enemyChampionId, MATCHUP_COUNT)
      setMatchups((prev) => ({ ...prev, [key]: res }))
    } catch {
      setMatchups((prev) => ({ ...prev, [key]: { games: 0, wins: 0, scanned: 0 } }))
    }
  }

  // Паттерны считаются по текущему чемпиону и роли слота — только игры на этом
  // же чемпионе в этой же роли, чтобы поведение было релевантно текущей игре.
  const loadPatterns = async (puuid: string, championId: number, role: Role) => {
    if (patterns[puuid] !== undefined && patterns[puuid] !== 'error') return
    setPatterns((prev) => ({ ...prev, [puuid]: 'loading' }))
    const { apiKey, region } = getCreds()
    try {
      const res = await fetchPlayerPatterns(apiKey, region, puuid, PATTERNS_COUNT, false, championId, role)
      setPatterns((prev) => ({ ...prev, [puuid]: res }))
    } catch {
      setPatterns((prev) => ({ ...prev, [puuid]: 'error' }))
    }
  }

  /** Грузит мастерство игрока и достаёт запись по чемпиону текущей игры. */
  const loadMastery = async (puuid: string, championId: number) => {
    const key = `${puuid}:${championId}`
    if (masteries[key] !== undefined && masteries[key] !== 'error') return
    setMasteries((prev) => ({ ...prev, [key]: 'loading' }))
    const { apiKey, region } = getCreds()
    try {
      const list = await fetchChampionMastery(apiKey, region, puuid)
      const entry = list.find((m) => m.championId === championId) ?? null
      setMasteries((prev) => ({ ...prev, [key]: entry }))
    } catch {
      setMasteries((prev) => ({ ...prev, [key]: 'error' }))
    }
  }

  const onDrop = (teamId: number, targetRole: Role) => {
    const from = dragRef.current
    dragRef.current = null
    if (!from || from.teamId !== teamId || from.role === targetRole) return
    setAssignments((prev) => {
      const team = { ...prev[teamId] }
      const tmp = team[targetRole]
      team[targetRole] = team[from.role]
      team[from.role] = tmp
      return { ...prev, [teamId]: team }
    })
  }

  const renderSlot = (teamId: number, role: Role) => {
    const puuid = assignments[teamId]?.[role] ?? null
    const player = puuid ? byPuuid.get(puuid) : undefined
    // Оппонент по линии = игрок той же роли в другой команде.
    const otherTeam = teamIds.find((t) => t !== teamId)
    const oppPuuid = otherTeam !== undefined ? (assignments[otherTeam]?.[role] ?? null) : null
    const opponent = oppPuuid ? (byPuuid.get(oppPuuid) ?? null) : null
    const mkey = player && opponent ? `${player.puuid}:${opponent.championId}` : ''
    const metaKey =
      player && opponent ? `${role}:${player.championId}:${opponent.championId}:${patch}` : ''
    return (
      <div
        className="scout-slot"
        onDragOver={(e) => {
          e.preventDefault()
          e.dataTransfer.dropEffect = 'move'
        }}
        onDrop={(e) => {
          e.preventDefault()
          onDrop(teamId, role)
        }}
      >
        {player ? (
          <div
            draggable
            onMouseDownCapture={(e) => {
              pointerDownRef.current = e.target as HTMLElement
            }}
            onDragStart={(e) => {
              // Не начинать перетаскивание роли, если жест начался на интерактивной
              // зоне карточки (ползунок/мини-карта/кнопка/таблица). Проверяем ТОЧКУ
              // НАЖАТИЯ: в WebView2 у dragstart e.target — сама карточка, не ползунок.
              const t = pointerDownRef.current
              if (t?.closest('input, button, select, textarea, a, .minimap, .scout-patterns, .scout-history, table')) {
                e.preventDefault()
                return
              }
              dragRef.current = { teamId, role }
              e.dataTransfer.effectAllowed = 'move'
              // Без setData drag не стартует в части браузеров; значение не используем.
              e.dataTransfer.setData('text/plain', player.puuid)
            }}
          >
            <PlayerCard
              player={player}
              expanded={expanded.has(player.puuid)}
              history={histories[player.puuid]}
              onToggleHistory={() => void toggleHistory(player.puuid)}
              opponent={opponent}
              matchup={mkey ? matchups[mkey] : undefined}
              metaMatchup={metaKey ? metaMatchups[metaKey] : undefined}
              onLoadMatchup={() => {
                if (player && opponent) void loadMatchup(player.puuid, player.championId, opponent.championId)
              }}
              onOpenProfile={() => {
                if (player.riotId) onOpenProfile(player.riotId)
              }}
              patterns={patterns[player.puuid]}
              onLoadPatterns={() => void loadPatterns(player.puuid, player.championId, role)}
              mastery={masteries[`${player.puuid}:${player.championId}`]}
              onLoadMastery={() => void loadMastery(player.puuid, player.championId)}
            />
          </div>
        ) : (
          <div className="scout-slot-empty">—</div>
        )}
      </div>
    )
  }

  const blue = teamIds[0]
  const red = teamIds[1]

  return (
    <div className="scout-view">
      <section className="panel">
        <h2>{t('scout.title')}</h2>
        <p className="field-help">
          <Trans i18nKey="scout.intro" components={{ 1: <strong /> }} />
        </p>
        <div className="scout-search">
          <input
            type="text"
            value={riotId}
            placeholder={t('scout.searchPlaceholder')}
            onChange={(e) => setRiotId(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void runScout()
            }}
          />
          <button type="button" className="btn-primary" disabled={loading} onClick={() => void runScout()}>
            {loading ? t('scout.searching') : t('scout.findGame')}
          </button>
          <button
            type="button"
            className="btn-secondary"
            disabled={loading}
            onClick={() => void runScoutMyGame()}
            title={t('scout.scoutMyGameTitle')}
          >
            {t('scout.scoutMyGame')}
          </button>
        </div>
        {error && <p className="field-help text-danger">{error}</p>}
      </section>

      {players && blue !== undefined && red !== undefined && (
        <section className="panel">
          <div className="scout-board">
            <div className="scout-board-head">
              <span className="scout-team-label scout-team-blue">{t('scout.team1')}</span>
              <span className="scout-role-head">{t('scout.lane')}</span>
              <span className="scout-team-label scout-team-red">{t('scout.team2')}</span>
            </div>
            {ROLE_ORDER.map((role) => (
              <div key={role} className="scout-row">
                {renderSlot(blue, role)}
                <div className="scout-role-cell">{t(`roles.${role}`)}</div>
                {renderSlot(red, role)}
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}
