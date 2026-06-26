/** Разбирает Riot ID "Имя#TAG" → [gameName, tagLine]. Тег — после последнего #. */
export function parseRiotId(riotId: string): [string, string] | null {
  const idx = riotId.lastIndexOf('#')
  if (idx <= 0) return null
  const gameName = riotId.slice(0, idx).trim()
  const tagLine = riotId.slice(idx + 1).trim()
  if (!gameName || tagLine.length < 2 || tagLine.length > 5) return null
  return [gameName, tagLine]
}
