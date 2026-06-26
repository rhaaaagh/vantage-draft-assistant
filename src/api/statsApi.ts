declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

interface SyncParams {
  apiKey: string
  region: string
  /** Riot ID: имя (до #). */
  gameName: string
  /** Riot ID: тег (после #), например "RU1". */
  tagLine: string
  count: number
}

export const syncMatches = async (params: SyncParams): Promise<number> => {
  if (!isTauri) {
    throw new Error('Синхронизация Riot API доступна только в десктопном приложении.')
  }

  const { invoke } = await import('@tauri-apps/api/core')
  const processed = await invoke<number>('sync_matches', params as unknown as Record<string, unknown>)
  return processed
}

