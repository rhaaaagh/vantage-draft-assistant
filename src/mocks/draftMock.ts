import type { DraftAnalysisResult, DraftState } from '../domain/draft'

export const defaultMockDraft: DraftState = {
  phase: 'CHAMP_SELECT',
  blue: {
    side: 'BLUE',
    slots: [
      { championId: 103, role: 'MID', playerName: 'You' },
      { championId: 64, role: 'JUNGLE' },
      { championId: 51, role: 'BOT' },
      { championId: null, role: 'TOP' },
      { championId: null, role: 'SUPPORT' },
    ],
    bans: [157, 238, 777],
  },
  red: {
    side: 'RED',
    slots: [
      { championId: 238, role: 'MID' },
      { championId: 157, role: 'TOP' },
      { championId: 222, role: 'BOT' },
      { championId: 412, role: 'SUPPORT' },
      { championId: null, role: 'JUNGLE' },
    ],
    bans: [103, 64, 523],
  },
}

export const defaultMockAnalysis: DraftAnalysisResult = {
  draft: defaultMockDraft,
  bestPicks: [
    {
      championId: 127,
      championName: 'Lissandra',
      winrate: 0.61,
      games: 1243,
      reason: 'Сильный контроль и engage против ассасинов Zed/Yasuo',
    },
    {
      championId: 3,
      championName: 'Galio',
      winrate: 0.59,
      games: 932,
      reason: 'Отличная защита керри и глобальное присутствие',
    },
  ],
  worstPicks: [
    {
      championId: 157,
      championName: 'Yasuo',
      winrate: 0.46,
      games: 843,
      reason: 'Слабый винрейт против текущей вражеской композиции',
    },
  ],
  build: {
    championId: 103,
    championName: 'Ahri',
    bestWinrateBuild: [
      { id: 6655, name: "Luden's Companion" },
      { id: 3157, name: "Zhonya's Hourglass" },
      { id: 3089, name: "Rabadon's Deathcap" },
    ],
    mostPopularBuild: [
      { id: 6655, name: "Luden's Companion" },
      { id: 3165, name: 'Morellonomicon' },
      { id: 3089, name: "Rabadon's Deathcap" },
    ],
    vsEnemyBuild: [
      { id: 3157, name: "Zhonya's Hourglass" },
      { id: 3020, name: "Sorcerer's Shoes" },
      { id: 3102, name: "Banshee's Veil" },
    ],
  },
  analytics: {
    blueWinProbability: null,
    redWinProbability: null,
    blueSynergyScore: null,
    redSynergyScore: null,
    blueDamageProfile: { ad: 0.35, ap: 0.65 },
    redDamageProfile: { ad: 0.8, ap: 0.2 },
    blueWeaknesses: ['Мало фронтлайна'],
    redWeaknesses: ['Слишком много AD урона', 'Мало магического урона'],
  },
}


