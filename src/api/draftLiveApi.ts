import type { PickRec } from './recommendApi'
import type { ChampItem, BuildSlot } from './championApi'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** Рекомендованная сборка для зафиксированного чемпиона локального игрока. */
export interface LiveBuildRec {
  championId: number
  championName: string
  role: string
  firstItems: ChampItem[]
  boots: ChampItem[]
  buildPath: BuildSlot[]
}

export interface LiveDraftRecommendations {
  /** Находимся ли мы сейчас в чемпион-селекте (LCU отдал сессию). */
  inChampSelect: boolean
  /** Авто-определённая (или переопределённая) роль; "" если неизвестна. */
  myRole: string
  recommendations: PickRec[]
  /** Сборка для зафиксированного чемпиона; null, если он не выбран или нет данных. */
  build: LiveBuildRec | null
}

const EMPTY: LiveDraftRecommendations = {
  inChampSelect: false,
  myRole: '',
  recommendations: [],
  build: null,
}

/**
 * Живые рекомендации пиков по данным чемпион-селекта из LCU.
 * `roleOverride` — ручной выбор роли, если авто-детект не сработал.
 */
export const fetchLiveDraftRecommendations = async (
  patch?: string,
  roleOverride?: string,
): Promise<LiveDraftRecommendations> => {
  if (!isTauri) return EMPTY
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<LiveDraftRecommendations>('get_live_draft_recommendations', { patch, roleOverride })
}
