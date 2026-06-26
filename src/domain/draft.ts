export type TeamSide = 'BLUE' | 'RED'

export type Role = 'TOP' | 'JUNGLE' | 'MID' | 'BOT' | 'SUPPORT'

export interface Champion {
  id: number
  name: string
  key: string
  roleHints: Role[]
}

export interface DraftSlot {
  championId: number | null
  championName?: string | null
  role: Role | null
  playerName?: string
}

export interface TeamDraft {
  side: TeamSide
  slots: DraftSlot[]
  bans: number[]
}

export interface DraftState {
  phase: 'NONE' | 'CHAMP_SELECT'
  blue: TeamDraft
  red: TeamDraft
}

export interface PickSuggestion {
  championId: number
  championName: string
  winrate: number
  games: number
  reason: string
}

export interface BuildItem {
  id: number
  name: string
}

export interface BuildRecommendation {
  championId: number
  championName: string
  bestWinrateBuild: BuildItem[]
  mostPopularBuild: BuildItem[]
  vsEnemyBuild: BuildItem[]
}

export interface DraftAnalytics {
  /** null = реальных данных пока нет (появится после сбора статистики, Этап 2). */
  blueWinProbability: number | null
  redWinProbability: number | null
  blueSynergyScore: number | null
  redSynergyScore: number | null
  /** Оценка по Data Dragon (рейтинги attack/magic чемпионов). */
  blueDamageProfile: { ad: number; ap: number }
  redDamageProfile: { ad: number; ap: number }
  blueWeaknesses: string[]
  redWeaknesses: string[]
}

export interface DraftAnalysisResult {
  draft: DraftState
  /** Баны твоей команды (явно из ответа API, для отображения). */
  blueBans?: number[]
  /** Баны команды противника (явно из ответа API). */
  redBans?: number[]
  bestPicks: PickSuggestion[]
  worstPicks: PickSuggestion[]
  build: BuildRecommendation | null
  analytics: DraftAnalytics
}

export interface DraftSimulationEntry {
  championId: number
  championName: string
  winProbability: number | null
}

export interface DraftSimulationResult {
  baseWinProbability: number | null
  entries: DraftSimulationEntry[]
}


