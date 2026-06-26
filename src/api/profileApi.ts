declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface ProfilePerChampion {
  championId: number
  championName: string
  games: number
  wins: number
}

export interface MatchPlayer {
  puuid: string
  riotId: string
  championId: number
  championName: string
  teamId: number
  role: string
  kills: number
  deaths: number
  assists: number
  cs: number
  win: boolean
  items: number[]
  isTarget: boolean
}

export interface ProfileGame {
  matchId: string
  championId: number
  championName: string
  win: boolean
  queueId: number
  kills: number
  deaths: number
  assists: number
  cs: number
  /** Длительность игры в секундах. */
  gameDuration: number
  /** Предметы (id, нули отфильтрованы). */
  items: number[]
  /** Все 10 участников матча. */
  participants: MatchPlayer[]
}

export interface ProfileResponse {
  found: boolean
  errorMessage?: string | null
  puuid: string
  riotId: string
  summonerLevel: number
  tier: string
  rank: string
  leaguePoints: number
  wins: number
  losses: number
  perChampion: ProfilePerChampion[]
  history: ProfileGame[]
}

export const fetchProfile = async (
  apiKey: string,
  region: string,
  gameName: string,
  tagLine: string,
): Promise<ProfileResponse> => {
  if (!isTauri) {
    return {
      found: false,
      puuid: '',
      riotId: '',
      summonerLevel: 0,
      tier: '',
      rank: '',
      leaguePoints: 0,
      wins: 0,
      losses: 0,
      perChampion: [],
      history: [],
      errorMessage: 'Профиль доступен только в десктопном приложении.',
    }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<ProfileResponse>('fetch_profile', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    gameName: gameName.trim(),
    tagLine: tagLine.trim(),
  })
}
