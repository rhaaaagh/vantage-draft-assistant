declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface CrawlStatus {
  running: boolean
  seeding: boolean
  puuidsTotal: number
  puuidsDone: number
  matchesTotal: number
  matchesDone: number
  target: number
  lastError?: string | null
  message: string
}

export const startCrawl = async (
  apiKey: string,
  region: string,
  includeDiamond: boolean,
  maxMatches: number,
  reset: boolean,
): Promise<void> => {
  if (!isTauri) throw new Error('Краулер доступен только в десктопном приложении.')
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('start_crawl', {
    apiKey: apiKey.trim(),
    region: region.trim(),
    includeDiamond,
    maxMatches,
    reset,
  })
}

export const stopCrawl = async (): Promise<void> => {
  if (!isTauri) return
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('stop_crawl')
}

export const getCrawlStatus = async (): Promise<CrawlStatus> => {
  if (!isTauri) {
    return {
      running: false,
      seeding: false,
      puuidsTotal: 0,
      puuidsDone: 0,
      matchesTotal: 0,
      matchesDone: 0,
      target: 0,
      message: 'Краулер доступен только в десктопном приложении.',
    }
  }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<CrawlStatus>('get_crawl_status')
}
