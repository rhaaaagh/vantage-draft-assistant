declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface PatchOptions {
  /** Текущий патч из каталога Data Dragon ("16.12"). */
  current: string
  /** Предыдущий патч в базе (или null, если хранится только один). */
  previous: string | null
}

export const getPatchOptions = async (): Promise<PatchOptions> => {
  if (!isTauri) return { current: '', previous: null }
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<PatchOptions>('get_patch_options')
}
