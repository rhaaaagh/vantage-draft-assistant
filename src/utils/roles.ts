export type Role = 'TOP' | 'JUNGLE' | 'MID' | 'BOT' | 'SUPPORT'

export const ROLE_ORDER: Role[] = ['TOP', 'JUNGLE', 'MID', 'BOT', 'SUPPORT']

export const ROLE_LABEL: Record<Role, string> = {
  TOP: 'Топ',
  JUNGLE: 'Лес',
  MID: 'Мид',
  BOT: 'Бот',
  SUPPORT: 'Саппорт',
}

const SMITE_SPELL_ID = 11

interface RoleInput {
  puuid: string
  championTags: string[]
  spell1Id: number
  spell2Id: number
}

/** Баллы пригодности игрока к каждой роли по тегам Data Dragon и заклинаниям. */
function roleScores(p: RoleInput): Record<Role, number> {
  const tags = p.championTags ?? []
  const hasSmite = p.spell1Id === SMITE_SPELL_ID || p.spell2Id === SMITE_SPELL_ID
  const s: Record<Role, number> = { TOP: 1, JUNGLE: 1, MID: 1, BOT: 1, SUPPORT: 1 }
  if (hasSmite) s.JUNGLE += 100 // Смайт — почти 100% признак леса
  if (tags.includes('Marksman')) s.BOT += 50
  if (tags.includes('Support')) s.SUPPORT += 50
  if (tags.includes('Tank')) { s.TOP += 15; s.SUPPORT += 12 }
  if (tags.includes('Fighter')) { s.TOP += 25; s.JUNGLE += 8 }
  if (tags.includes('Assassin')) { s.MID += 22; s.JUNGLE += 6 }
  if (tags.includes('Mage')) { s.MID += 20; s.SUPPORT += 8 }
  return s
}

/**
 * Жадно раскидывает до 5 игроков команды по уникальным ролям.
 * Возвращает Record<Role, puuid | null>. Неточности правятся перетаскиванием.
 */
export function assignRoles(players: RoleInput[]): Record<Role, string | null> {
  const result: Record<Role, string | null> = {
    TOP: null, JUNGLE: null, MID: null, BOT: null, SUPPORT: null,
  }
  const candidates: { puuid: string; role: Role; score: number }[] = []
  for (const p of players.slice(0, 5)) {
    const scores = roleScores(p)
    for (const role of ROLE_ORDER) {
      candidates.push({ puuid: p.puuid, role, score: scores[role] })
    }
  }
  candidates.sort((a, b) => b.score - a.score)
  const usedPlayers = new Set<string>()
  const usedRoles = new Set<Role>()
  for (const c of candidates) {
    if (usedPlayers.has(c.puuid) || usedRoles.has(c.role)) continue
    result[c.role] = c.puuid
    usedPlayers.add(c.puuid)
    usedRoles.add(c.role)
  }
  // Если игроков было меньше 5 или остались дыры — ничего, слоты просто пустые.
  return result
}

export const TIER_LABEL: Record<string, string> = {
  IRON: 'Железо',
  BRONZE: 'Бронза',
  SILVER: 'Серебро',
  GOLD: 'Золото',
  PLATINUM: 'Платина',
  EMERALD: 'Изумруд',
  DIAMOND: 'Алмаз',
  MASTER: 'Мастер',
  GRANDMASTER: 'Грандмастер',
  CHALLENGER: 'Челленджер',
}

export function formatRank(tier: string, rank: string, lp: number, wins: number, losses: number): string {
  if (!tier) return 'Без ранга'
  const total = wins + losses
  const wr = total > 0 ? `${Math.round((wins / total) * 100)}% (${wins}В/${losses}П)` : 'нет игр'
  const tierRu = TIER_LABEL[tier.toUpperCase()] ?? tier
  const div = rank ? ` ${rank}` : ''
  return `${tierRu}${div} · ${lp} LP · ${wr}`
}
