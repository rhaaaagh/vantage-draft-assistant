declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface ChampionMasteryEntry {
  championId: number
  championName: string
  level: number
  points: number
}

/** Топ чемпионов по мастерству для игрока (по puuid). */
export const fetchChampionMastery = async (
  apiKey: string,
  region: string,
  puuid: string,
): Promise<ChampionMasteryEntry[]> => {
  if (!isTauri) return []
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<ChampionMasteryEntry[]>('get_champion_mastery', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    puuid,
  })
}
