declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface ChampMatchup {
  championId: number
  championName: string
  games: number
  winRate: number
}

export interface ChampSynergy {
  championId: number
  championName: string
  role: string
  games: number
  winRate: number
}

export interface ChampItem {
  itemId: number
  games: number
  winRate: number
}

export interface BuildSlot {
  slot: number
  items: ChampItem[]
}

export interface ChampRoleOpt {
  role: string
  games: number
}

export interface ChampRune {
  runeId: number
  games: number
  winRate: number
}

export interface ChampRunes {
  keystones: ChampRune[]
  primary: ChampRune[]
  secondary: ChampRune[]
}

export interface ChampionPageResponse {
  found: boolean
  championId: number
  championName: string
  role: string
  roles: ChampRoleOpt[]
  games: number
  winRate: number
  pickRate: number
  banRate: number
  strongAgainst: ChampMatchup[]
  weakAgainst: ChampMatchup[]
  synergies: ChampSynergy[]
  firstItems: ChampItem[]
  finalItems: ChampItem[]
  boots: ChampItem[]
  buildPath: BuildSlot[]
  runes: ChampRunes
}

export const fetchChampionPage = async (
  championId: number,
  role: string | null,
  patch?: string,
): Promise<ChampionPageResponse> => {
  if (!isTauri) {
    throw new Error('Страница чемпиона доступна только в десктопном приложении.')
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<ChampionPageResponse>('get_champion_page', {
    championId,
    role: role ?? null,
    patch,
  })
}
