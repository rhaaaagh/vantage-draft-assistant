import React, { useEffect, useState } from 'react'
import { Trans, useTranslation } from 'react-i18next'
import { fetchProfile } from '../api/profileApi'
import type { MatchPlayer, ProfileResponse } from '../api/profileApi'
import { getChampionIconUrl, getItemIconUrl, useChampionCatalog } from '../api/championCatalog'
import { parseRiotId } from '../utils/riotId'
import { fetchChampionMastery } from '../api/masteryApi'
import type { ChampionMasteryEntry } from '../api/masteryApi'
import { Card, Stat, StatRow, MatchRow, ChampList, LineChart, Donut } from '../components/ui'
import type { ChampListItem } from '../components/ui'
import { useRail } from '../components/rail'
import { IconSearch } from '../components/icons'
import { MatchBreakdown } from './MatchBreakdown'
import './Profile.css'

/**
 * Кэш последнего загруженного профиля на уровне модуля — переживает
 * размонтирование при переключении вкладок, поэтому возврат на Профиль
 * не перезапрашивает данные.
 */
let cache: { account: string; seq: number; data: ProfileResponse } | null = null
/** Кэш мастерства по puuid — переживает смену вкладок. */
const masteryCache = new Map<string, ChampionMasteryEntry[]>()

function fmtPoints(n: number): string {
  return n >= 1000 ? `${Math.round(n / 1000)}k` : String(n)
}

function getCreds(): { apiKey: string; region: string } {
  if (typeof window === 'undefined') return { apiKey: '', region: 'ru' }
  return {
    apiKey: window.localStorage.getItem('lolda_riot_api_key') ?? '',
    region: window.localStorage.getItem('lolda_region') ?? 'ru',
  }
}

/** Один игрок в развёрнутой карточке матча. */
function PlayerLine({ p }: { p: MatchPlayer }) {
  const { t } = useTranslation()
  const icon = getChampionIconUrl(p.championId)
  return (
    <div className={`pd-player${p.isTarget ? ' is-target' : ''}`}>
      {icon && <img src={icon} alt="" width={22} height={22} className="pd-player__icon" />}
      <span className="pd-player__role">{p.role ? t(`roles.${p.role}`, { defaultValue: '' }) : ''}</span>
      <span className="pd-player__name" title={p.riotId}>{p.riotId}</span>
      <span className="pd-player__kda num">{p.kills}/{p.deaths}/{p.assists}</span>
      <span className="pd-player__cs num">{p.cs}</span>
      <span className="pd-player__items">
        {p.items.map((id, j) => (
          <img key={`${id}-${j}`} src={getItemIconUrl(id)} alt="" width={18} height={18} />
        ))}
      </span>
    </div>
  )
}

function MatchDetail({ participants }: { participants: MatchPlayer[] }) {
  const { t } = useTranslation()
  const blue = participants.filter((p) => p.teamId === 100)
  const red = participants.filter((p) => p.teamId === 200)
  return (
    <div className="pd">
      <div className="pd-team">
        <div className="pd-team__head pd-team__head--blue">{t('profile.blueTeam')}</div>
        {blue.map((p, i) => <PlayerLine key={`b-${i}`} p={p} />)}
      </div>
      <div className="pd-team">
        <div className="pd-team__head pd-team__head--red">{t('profile.redTeam')}</div>
        {red.map((p, i) => <PlayerLine key={`r-${i}`} p={p} />)}
      </div>
    </div>
  )
}

interface ProfileProps {
  /** Текущий аккаунт. null — поиск ещё не делали.
   *  seq меняется на каждый сабмит — повторный поиск того же Riot ID перезагружает. */
  account: { query: string; seq: number } | null
  /** Сабмит поиска (поле живёт на этом экране). Задаёт «текущий аккаунт» в App. */
  onSearch: (query: string) => void
}

export const Profile: React.FC<ProfileProps> = ({ account, onSearch }) => {
  const { t } = useTranslation()
  useChampionCatalog()

  // Поле поиска — на самом экране Профиля. Предзаполняем текущим аккаунтом,
  // чтобы было видно, кого смотрим, и можно было отредактировать.
  const [searchText, setSearchText] = useState(account?.query ?? '')
  const [expandedGame, setExpandedGame] = useState<number | null>(null)
  // Открытый полноэкранный разбор матча (null — показываем обычный профиль).
  const [breakdown, setBreakdown] = useState<{ matchId: string; targetPuuid: string | null } | null>(
    null,
  )
  // Ошибка сети и тик-перерисовка ставятся только из async-колбэков, не в теле эффекта.
  const [fetchError, setFetchError] = useState<{ seq: number; message: string } | null>(null)
  const [, setTick] = useState(0)

  // ---- Всё состояние экрана выводится из props + модульного кэша (без эффект-setState) ----
  const creds = getCreds()
  const query = account?.query ?? null
  const seq = account?.seq ?? 0
  const parsed = query ? parseRiotId(query) : null
  const validationError = query
    ? !creds.apiKey.trim()
      ? t('profile.errors.noApiKey')
      : !parsed
        ? t('profile.errors.badRiotId')
        : null
    : null

  // Кэш валиден, только если совпадают и аккаунт, и seq — иначе повторный
  // поиск (новый seq) форсит перезагрузку, а смена вкладок (тот же seq) — нет.
  const cacheHit = !!query && cache?.account === query && cache?.seq === seq
  const data = cacheHit ? cache!.data : null
  const error = validationError ?? (fetchError?.seq === seq ? fetchError.message : null)
  const loading = !!query && !validationError && !data && !error

  // Эффект делает ТОЛЬКО сетевой запрос; state меняется в async-колбэках.
  useEffect(() => {
    if (!query || validationError || !parsed) return
    if (cache && cache.account === query && cache.seq === seq) return
    let active = true
    fetchProfile(creds.apiKey, creds.region, parsed[0], parsed[1])
      .then((res) => {
        if (!active) return
        if (!res.found) {
          setFetchError({ seq, message: res.errorMessage ?? t('profile.errors.notFound') })
        } else {
          cache = { account: query, seq, data: res }
          setFetchError(null)
          setTick((t) => t + 1)
        }
      })
      .catch((e) => {
        if (active) setFetchError({ seq, message: String(e) })
      })
    return () => {
      active = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, seq])

  // ---- Производные данные ----
  const found = data?.found ? data : null

  // Мастерство чемпионов — отдельный ленивый запрос по puuid.
  const masteryPuuid = found?.puuid ?? null
  useEffect(() => {
    if (!masteryPuuid || masteryCache.has(masteryPuuid)) return
    let active = true
    fetchChampionMastery(creds.apiKey, creds.region, masteryPuuid)
      .then((res) => {
        if (!active) return
        masteryCache.set(masteryPuuid, res)
        setTick((t) => t + 1)
      })
      .catch(() => {
        if (!active) return
        masteryCache.set(masteryPuuid, [])
        setTick((t) => t + 1)
      })
    return () => {
      active = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [masteryPuuid])
  const masteryList = masteryPuuid ? masteryCache.get(masteryPuuid) ?? [] : []
  const totalRanked = found ? found.wins + found.losses : 0
  const rankedWr = totalRanked > 0 ? (found!.wins / totalRanked) * 100 : null
  const tierLabel = found?.tier
    ? t(`profile.tiers.${found.tier.toUpperCase()}`, { defaultValue: found.tier })
    : null

  const champItems: ChampListItem[] = found
    ? found.perChampion.map((c) => ({
        championId: c.championId,
        name: c.championName,
        games: c.games,
        winRate: c.games > 0 ? (c.wins / c.games) * 100 : 0,
        icon: getChampionIconUrl(c.championId),
      }))
    : []

  const formWins = found ? found.history.filter((g) => g.win).length : 0
  const formGames = found ? found.history.length : 0
  const kdaTrend = found
    ? found.history
        .slice()
        .reverse()
        .map((g) => (g.deaths === 0 ? g.kills + g.assists : (g.kills + g.assists) / g.deaths))
    : []

  // ---- Правый рейл (прячем во время полноэкранного разбора матча) ----
  useRail(
    breakdown || !found ? null : (
      <>
        <Card title={t('profile.form.title')}>
          <StatRow>
            <Stat label={t('profile.form.games')} value={formGames} />
            <Stat label={t('profile.form.wins')} value={formWins} tone="win" />
            <Stat
              label={t('profile.form.winrate')}
              value={`${formGames > 0 ? Math.round((formWins / formGames) * 100) : 0}%`}
              tone="accent"
            />
          </StatRow>
        </Card>
        <Card title={t('profile.kdaTrend')}>
          <LineChart values={kdaTrend} emptyLabel={t('profile.notEnoughGames')} />
        </Card>
      </>
    ),
    [data, breakdown],
  )

  // ---- Полноэкранный разбор матча перекрывает обычный контент ----
  if (breakdown) {
    return (
      <MatchBreakdown
        matchId={breakdown.matchId}
        targetPuuid={breakdown.targetPuuid}
        onBack={() => setBreakdown(null)}
      />
    )
  }

  // ---- Поле поиска (всегда сверху экрана) ----
  const submitSearch = (e: React.FormEvent) => {
    e.preventDefault()
    const q = searchText.trim()
    if (q) onSearch(q)
  }
  const searchBar = (
    <form className="profile-search" onSubmit={submitSearch} role="search">
      <input
        type="text"
        value={searchText}
        placeholder={t('profile.searchPlaceholder')}
        onChange={(e) => setSearchText(e.target.value)}
        autoFocus
      />
      <button type="submit" className="profile-search__btn" aria-label={t('profile.searchAria')}>
        <IconSearch />
      </button>
    </form>
  )

  // ---- Пустое состояние / загрузка / ошибка ----
  if (!found) {
    return (
      <>
        {searchBar}
        <Card>
          <div className="profile-empty">
            {loading ? (
              <p className="ui-muted">{t('profile.loading')}</p>
            ) : error ? (
              <p className="ui-error">{error}</p>
            ) : (
              <>
                <p className="profile-empty__title">{t('profile.emptyTitle')}</p>
                <p className="ui-muted">
                  <Trans i18nKey="profile.emptyHint" components={[<span className="num" />]} />
                </p>
              </>
            )}
          </div>
        </Card>
      </>
    )
  }

  return (
    <>
      {searchBar}
      {/* Шапка профиля */}
      <Card>
        <div className="profile-head">
          <div className="profile-head__main">
            <div className="profile-head__id">
              <span className="profile-head__name">{found.riotId}</span>
              <span className="profile-head__level">{t('profile.level', { level: found.summonerLevel })}</span>
            </div>
            <StatRow>
              <Stat
                label={t('profile.rank')}
                value={tierLabel ? `${tierLabel}${found.rank ? ` ${found.rank}` : ''}` : t('profile.noRank')}
                sub={tierLabel ? `${found.leaguePoints} LP` : undefined}
              />
              {rankedWr != null && (
                <Stat
                  label={t('profile.seasonWinrate')}
                  value={`${rankedWr.toFixed(1)}%`}
                  sub={t('profile.winLoss', { wins: found.wins, losses: found.losses })}
                  tone="accent"
                />
              )}
            </StatRow>
          </div>
          {totalRanked > 0 && (
            <div className="profile-head__wr">
              <Donut wins={found.wins} losses={found.losses} />
              <div className="profile-head__wr-legend">
                <span className="profile-head__wr-w num">{t('profile.winShort', { count: found.wins })}</span>
                <span className="profile-head__wr-sep">/</span>
                <span className="profile-head__wr-l num">{t('profile.lossShort', { count: found.losses })}</span>
              </div>
            </div>
          )}
        </div>
      </Card>

      {/* Чемпионы */}
      {champItems.length > 0 && (
        <Card title={t('profile.championsRecent')}>
          <ChampList items={champItems} />
        </Card>
      )}

      {masteryList.length > 0 && (
        <Card title={t('profile.mastery')}>
          <div className="profile-mastery">
            {masteryList.map((m) => {
              const icon = getChampionIconUrl(m.championId)
              return (
                <div className="profile-mastery__item" key={m.championId}>
                  {icon && <img src={icon} alt="" width={32} height={32} className="profile-mastery__icon" />}
                  <span className="profile-mastery__name" title={m.championName}>{m.championName}</span>
                  <span className="profile-mastery__lvl num">{t('profile.masteryLevel', { level: m.level })}</span>
                  <span className="profile-mastery__pts num">{fmtPoints(m.points)}</span>
                </div>
              )
            })}
          </div>
        </Card>
      )}

      {/* История игр */}
      {found.history.length > 0 && (
        <Card title={t('profile.history')}>
          <div className="profile-history">
            {found.history.map((g, i) => {
              const isOpen = expandedGame === i
              const csPerMin = g.gameDuration > 0 ? (g.cs / (g.gameDuration / 60)).toFixed(1) : '0'
              return (
                <div key={`${g.championId}-${i}`} className="profile-game">
                  <MatchRow
                    win={g.win}
                    championIcon={getChampionIconUrl(g.championId)}
                    championName={g.championName}
                    kills={g.kills}
                    deaths={g.deaths}
                    assists={g.assists}
                    cs={g.cs}
                    csPerMin={csPerMin}
                    items={g.items}
                    itemIconUrl={getItemIconUrl}
                    durationSec={g.gameDuration}
                    open={isOpen}
                    onClick={() => setExpandedGame(isOpen ? null : i)}
                  />
                  {isOpen && (
                    <div className="profile-game__detail">
                      <MatchDetail participants={g.participants} />
                      <button
                        type="button"
                        className="profile-game__more"
                        onClick={() =>
                          setBreakdown({
                            matchId: g.matchId,
                            targetPuuid: g.participants.find((p) => p.isTarget)?.puuid ?? null,
                          })
                        }
                      >
                        {t('profile.more')}
                      </button>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        </Card>
      )}
    </>
  )
}

export default Profile
