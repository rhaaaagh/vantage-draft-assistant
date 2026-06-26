declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface MatchDetailPlayer {
  puuid: string
  riotId: string
  championId: number
  championName: string
  teamId: number
  role: string
  champLevel: number
  kills: number
  deaths: number
  assists: number
  cs: number
  gold: number
  damageToChampions: number
  damageTaken: number
  visionScore: number
  wardsPlaced: number
  wardsKilled: number
  controlWards: number
  items: number[]
  keystoneId: number
  primaryStyleId: number
  subStyleId: number
  /** Полные руны: основное древо (4), вторичное (2), осколки статов (3). */
  primaryPerks: number[]
  subPerks: number[]
  statPerks: number[]
  soloKills: number
  killsUnderTurret: number
  /** 0..1 */
  killParticipation: number
  /** 0..1 */
  teamDamagePercentage: number
  isTarget: boolean
  /** Последовательность покупок (Этап 2; пусто если таймлайн недоступен). */
  purchases: PurchaseItem[]
}

export interface PurchaseItem {
  itemId: number
  /** Минута игры, на которой куплен. */
  minute: number
}

export interface MatchTeamSummary {
  teamId: number
  win: boolean
  kills: number
  deaths: number
  assists: number
  gold: number
  baron: number
  dragon: number
  herald: number
  tower: number
  inhibitor: number
}

export interface FrameSnapshot {
  minute: number
  /** Метрики по индексу = порядок players. */
  gold: number[]
  xp: number[]
  level: number[]
  cs: number[]
}

export interface GameEvent {
  minute: number
  second: number
  /** 'kill' | 'monster' | 'building' */
  kind: string
  /** participantId 1..10 (0 — нет). */
  killerId: number
  victimId: number
  teamId: number
  detail: string
  x: number
  y: number
}

export interface MatchDetailResponse {
  matchId: string
  queueId: number
  patch: string
  gameDuration: number
  teams: MatchTeamSummary[]
  players: MatchDetailPlayer[]
  /** Преимущество команды искомого игрока по золоту по минутам (может быть < 0). */
  goldAdvantage: number[]
  /** Метрики по минутам (для графиков и ползунка). */
  frames: FrameSnapshot[]
  /** Лента событий матча. */
  events: GameEvent[]
}

export const fetchMatchDetail = async (
  apiKey: string,
  region: string,
  matchId: string,
  targetPuuid: string | null,
): Promise<MatchDetailResponse> => {
  if (!isTauri) {
    throw new Error('Разбор матча доступен только в десктопном приложении.')
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<MatchDetailResponse>('get_match_detail', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    matchId,
    targetPuuid: targetPuuid ?? null,
  })
}
