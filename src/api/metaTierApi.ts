declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface MetaTierRow {
  championId: number
  championName: string
  role: string
  patch: string
  games: number
  wins: number
  winRate: number
}

export interface MetaTierResponse {
  patches: string[]
  roles: string[]
  rows: MetaTierRow[]
}

export const getMetaTierList = async (
  patch?: string,
  role?: string,
): Promise<MetaTierResponse> => {
  if (!isTauri) return { patches: [], roles: [], rows: [] }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<MetaTierResponse>('get_meta_tier_list', { patch, role })
}
