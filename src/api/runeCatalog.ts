import { useEffect, useState } from 'react'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface RuneInfo {
  url: string
  name: string
  desc: string
}

let cached: Map<number, RuneInfo> | null = null
let pending: Promise<Map<number, RuneInfo>> | null = null

/** Грузит каталог рун один раз (ленивая загрузка на бэке): id → иконка/имя/описание. */
export async function loadRuneIcons(): Promise<Map<number, RuneInfo>> {
  if (cached) return cached
  if (!pending) {
    pending = (async (): Promise<Map<number, RuneInfo>> => {
      if (!isTauri) {
        cached = new Map()
        return cached
      }
      const { invoke } = await import('@tauri-apps/api/core')
      const res = await invoke<{ id: number; url: string; name: string; desc: string }[]>(
        'get_rune_icons',
      )
      cached = new Map(res.map((r) => [r.id, { url: r.url, name: r.name, desc: r.desc }]))
      return cached
    })().catch(() => {
      cached = new Map()
      return cached
    })
  }
  return pending
}

/** URL иконки руны/древа по id (null — пока не загружено или неизвестно). */
export function getRuneIconUrl(id: number): string | null {
  return cached?.get(id)?.url ?? null
}

/** Имя + описание руны по id (для подсказок). */
export function getRuneInfo(id: number): RuneInfo | null {
  return cached?.get(id) ?? null
}

/** Хук: грузит каталог рун один раз, возвращает true когда готов. */
export function useRuneCatalog(): boolean {
  const [loaded, setLoaded] = useState(cached !== null)
  useEffect(() => {
    if (cached) return
    let active = true
    void loadRuneIcons().then(() => {
      if (active) setLoaded(true)
    })
    return () => {
      active = false
    }
  }, [])
  return loaded
}
