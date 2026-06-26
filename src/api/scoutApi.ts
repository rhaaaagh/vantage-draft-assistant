declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface ScoutPlayer {
  puuid: string
  riotId: string
  summonerName: string
  championId: number
  championName: string
  teamId: number
  spell1Id: number
  spell2Id: number
  championTags: string[]
  tier: string
  rank: string
  leaguePoints: number
  wins: number
  losses: number
}

export interface ScoutResponse {
  hasGame: boolean
  errorMessage?: string | null
  players: ScoutPlayer[]
}

export interface PlayerGame {
  championId: number
  championName: string
  win: boolean
  queueId: number
}

export interface PlayerHistoryResponse {
  games: PlayerGame[]
}

export const scoutActiveGame = async (
  apiKey: string,
  region: string,
  gameName: string,
  tagLine: string,
): Promise<ScoutResponse> => {
  if (!isTauri) {
    return { hasGame: false, players: [], errorMessage: 'Скаут доступен только в десктопном приложении.' }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<ScoutResponse>('scout_active_game', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    gameName: gameName.trim(),
    tagLine: tagLine.trim(),
  })
}

/** Авто-скаут собственной активной игры — PUUID берётся из LCU. */
export const scoutMyActiveGame = async (
  apiKey: string,
  region: string,
): Promise<ScoutResponse> => {
  if (!isTauri) {
    return { hasGame: false, players: [], errorMessage: 'Скаут доступен только в десктопном приложении.' }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<ScoutResponse>('scout_my_active_game', {
    apiKey: apiKey.trim(),
    region: region.trim(),
  })
}

/** Мета-матчап чемпиона против врага из агрегата краулера (мгновенно, без сети). */
export interface MetaMatchup {
  games: number
  wins: number
}

export const fetchMetaMatchup = async (
  role: string,
  championId: number,
  enemyChampionId: number,
  patch: string,
): Promise<MetaMatchup> => {
  if (!isTauri) return { games: 0, wins: 0 }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<MetaMatchup>('scout_meta_matchup', {
    role,
    championId,
    enemyChampionId,
    patch,
  })
}

export interface PersonalMatchup {
  /** Игр на этом чемпионе против этого врага найдено. */
  games: number
  /** Из них выиграно. */
  wins: number
  /** Сколько недавних матчей просмотрено. */
  scanned: number
}

/** Личный винрейт игрока на его чемпионе против конкретного вражеского чемпиона. */
export const fetchPersonalMatchup = async (
  apiKey: string,
  region: string,
  puuid: string,
  championId: number,
  enemyChampionId: number,
  count: number,
): Promise<PersonalMatchup> => {
  if (!isTauri) return { games: 0, wins: 0, scanned: 0 }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<PersonalMatchup>('scout_personal_matchup', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    puuid,
    championId,
    enemyChampionId,
    count,
  })
}

// ---------- паттерны игрока (предсказание поведения по истории) ----------

export interface MapPoint {
  /** Доля по X (0..1), 0 = низ-лево синей базы. */
  x: number
  /** Доля по Y (0..1), 0 = низ карты. */
  y: number
  /** Игровая минута события (для биннинга/слайдера). */
  minute: number
  /** Точное время события в секундах (для таймингов смертей M:SS). */
  atSeconds?: number
}

export interface PatternArchetype {
  label: string
  /** Доля игр 0..1. */
  value: number
  explanation: string
}

export interface HeatmapBin {
  fromMinute: number
  points: MapPoint[]
}

export interface JungleRoute {
  /** "OWN" | "ENEMY" — сторона старта фарма. */
  startSide: string
  path: MapPoint[]
}

export interface PlayerPatterns {
  puuid: string
  gamesAnalyzed: number
  mainRole: string
  mainChampionId: number
  mainChampionName: string
  archetypes: PatternArchetype[]
  deathPoints: MapPoint[]
  heatmap: HeatmapBin[]
  /** Per-game маршруты (сохранены для совместимости; UI рисует avgJungleRoute). */
  jungleRoutes: JungleRoute[]
  /** Один усреднённый «типичный» ранний лесной маршрут (пусто, если не лесник). */
  avgJungleRoute: MapPoint[]
  /** Доля лесных игр со стартом в своём лесу (0..1). */
  ownStartFraction: number
  /** Полные треки позиций по каждой игре (для линий маршрута перемещений). */
  positionRoutes?: MapPoint[][]
}

/**
 * Анализ паттернов поведения игрока по его истории. Дорого (~2 запроса/игру),
 * результат кэшируется на бэкенде по puuid — вызывать по явному действию.
 */
export const fetchPlayerPatterns = async (
  apiKey: string,
  region: string,
  puuid: string,
  count: number,
  force = false,
  /** Если задан — анализировать только игры на этом чемпионе (id). */
  championId?: number,
  /** Если задана — только игры в этой роли (TOP/JUNGLE/MID/BOT/SUPPORT). */
  role?: string,
): Promise<PlayerPatterns> => {
  if (!isTauri) {
    return {
      puuid,
      gamesAnalyzed: 0,
      mainRole: '',
      mainChampionId: 0,
      mainChampionName: '',
      archetypes: [],
      deathPoints: [],
      heatmap: [],
      jungleRoutes: [],
      avgJungleRoute: [],
      ownStartFraction: 0,
    }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<PlayerPatterns>('scout_player_patterns', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    puuid,
    count,
    force,
    championId,
    role,
  })
}

export const scoutPlayerHistory = async (
  apiKey: string,
  region: string,
  puuid: string,
  count: number,
): Promise<PlayerHistoryResponse> => {
  if (!isTauri) return { games: [] }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<PlayerHistoryResponse>('scout_player_history', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    puuid,
    count,
  })
}
