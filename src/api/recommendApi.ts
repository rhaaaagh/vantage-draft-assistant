declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface DraftPick {
  championId: number
  role: string
}

export interface PickRec {
  championId: number
  championName: string
  /** Оценка пика 0..100. */
  score: number
  /** Сглаженный базовый винрейт 0..1. */
  baseWinRate: number
  games: number
  reason: string
}

export const fetchPickRecommendations = async (
  myRole: string,
  enemies: DraftPick[],
  allies: DraftPick[],
  patch?: string,
): Promise<PickRec[]> => {
  if (!isTauri) return []
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<PickRec[]>('get_pick_recommendations', { myRole, enemies, allies, patch })
}
