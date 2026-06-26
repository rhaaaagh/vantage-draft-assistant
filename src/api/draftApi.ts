import type { DraftAnalysisResult, DraftSimulationResult, DraftState } from '../domain/draft'
import { defaultMockAnalysis, defaultMockDraft } from '../mocks/draftMock'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export const fetchDraftState = async (): Promise<DraftState> => {
  if (!isTauri) {
    // В веб-режиме возвращаем мок-данные
    return defaultMockDraft
  }

  // Динамический импорт, чтобы не ломать веб-сборку, если Tauri недоступен
  const { invoke } = await import('@tauri-apps/api/core')
  const state = await invoke<DraftState>('get_draft_state')
  return state
}

export const fetchDraftAnalysis = async (): Promise<DraftAnalysisResult> => {
  if (!isTauri) {
    return defaultMockAnalysis
  }

  const { invoke } = await import('@tauri-apps/api/core')
  const result = await invoke<DraftAnalysisResult>('analyze_current_draft')
  return result
}

export interface DraftBansResponse {
  blueBans: number[]
  redBans: number[]
}

export const fetchDraftBans = async (): Promise<DraftBansResponse> => {
  if (!isTauri) return { blueBans: [], redBans: [] }
  const { invoke } = await import('@tauri-apps/api/core')
  const res = (await invoke<Record<string, unknown>>('get_draft_bans')) ?? {}
  const blueBans = Array.isArray(res.blueBans) ? res.blueBans as number[] : Array.isArray(res.blue_bans) ? res.blue_bans as number[] : []
  const redBans = Array.isArray(res.redBans) ? res.redBans as number[] : Array.isArray(res.red_bans) ? res.red_bans as number[] : []
  return { blueBans, redBans }
}

export const getLeaguePath = async (): Promise<string> => {
  if (!isTauri) return ''
  const { invoke } = await import('@tauri-apps/api/core')
  return (await invoke<string>('get_league_path')) ?? ''
}

export const setLeaguePath = async (path: string): Promise<void> => {
  if (!isTauri) return
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('set_league_path', { path })
}

export const simulatePicks = async (championIds: number[]): Promise<DraftSimulationResult> => {
  if (!isTauri) {
    // В веб-режиме возвращаем пустую заглушку (реальных данных нет).
    return {
      baseWinProbability: null,
      entries: championIds.map((id) => ({
        championId: id,
        championName: `#${id}`,
        winProbability: null,
      })),
    }
  }

  const { invoke } = await import('@tauri-apps/api/core')
  // Tauri 2 ожидает camelCase-ключи аргументов команд.
  const result = await invoke<DraftSimulationResult>('simulate_picks', {
    championIds,
  })
  return result
}

export interface LcuCheckResult {
  found: boolean
  port?: number
  message: string
  sessionSaved: boolean
}

export const checkLcu = async (): Promise<LcuCheckResult> => {
  if (!isTauri) {
    return {
      found: false,
      port: undefined,
      message: 'Проверка LCU доступна только в десктопном приложении.',
      sessionSaved: false,
    }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<LcuCheckResult>('check_lcu')
}

export interface CurrentGamePlayer {
  summonerName: string
  /** Riot ID вида "Имя#TAG" (основное отображаемое имя). */
  riotId: string
  championId: number
  championName: string
  rank: string
}

export interface CurrentGameInfoResponse {
  hasGame: boolean
  errorMessage?: string | null
  myTeam: CurrentGamePlayer[]
  enemyTeam: CurrentGamePlayer[]
}

/** Отладка: проверка пайплайна Riot API по произвольному Riot ID (без LCU). */
export const debugGameInfoForRiotId = async (
  apiKey: string,
  region: string,
  gameName: string,
  tagLine: string,
): Promise<CurrentGameInfoResponse> => {
  if (!isTauri) {
    return { hasGame: false, myTeam: [], enemyTeam: [] }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<CurrentGameInfoResponse>('debug_game_info_for_riot_id', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    gameName: gameName.trim(),
    tagLine: tagLine.trim(),
  })
}

export const fetchCurrentGameInfo = async (
  apiKey: string,
  region: string
): Promise<CurrentGameInfoResponse> => {
  if (!isTauri) {
    return {
      hasGame: false,
      myTeam: [],
      enemyTeam: [],
    }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  // Tauri 2 ожидает camelCase-ключи аргументов (api_key не работал — команда падала молча).
  const raw = await invoke<CurrentGameInfoResponse>('get_current_game_info', {
    apiKey: apiKey.trim(),
    region: region.trim(),
  })
  if (!raw || typeof raw !== 'object') {
    return { hasGame: false, myTeam: [], enemyTeam: [] }
  }
  const r = raw as unknown as Record<string, unknown>
  const errVal = r.errorMessage ?? r.error_message
  const errorMessage = typeof errVal === 'string' ? errVal : null
  return {
    hasGame: Boolean(r.hasGame ?? r.has_game),
    errorMessage,
    myTeam: Array.isArray(r.myTeam) ? (r.myTeam as CurrentGamePlayer[]) : Array.isArray(r.my_team) ? (r.my_team as CurrentGamePlayer[]) : [],
    enemyTeam: Array.isArray(r.enemyTeam) ? (r.enemyTeam as CurrentGamePlayer[]) : Array.isArray(r.enemy_team) ? (r.enemy_team as CurrentGamePlayer[]) : [],
  }
}


