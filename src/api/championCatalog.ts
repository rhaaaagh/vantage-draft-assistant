import { useEffect, useState } from 'react'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface ChampionMeta {
  id: number
  /** Имя файла иконки Data Dragon (например "MonkeyKing" для Вуконга). */
  iconName: string
  name: string
  tags: string[]
  attack: number
  magic: number
  defense: number
}

interface Catalog {
  version: string
  byId: Map<number, ChampionMeta>
}

/** Используется в веб-режиме и до загрузки каталога (только для URL предметов). */
const FALLBACK_VERSION = '16.12.1'

let cached: Catalog | null = null
let pending: Promise<Catalog> | null = null

export async function loadCatalog(): Promise<Catalog> {
  if (cached) return cached
  if (!pending) {
    pending = (async (): Promise<Catalog> => {
      if (!isTauri) {
        cached = { version: FALLBACK_VERSION, byId: new Map() }
        return cached
      }
      const { invoke } = await import('@tauri-apps/api/core')
      const res = await invoke<{ version: string; champions: ChampionMeta[] }>(
        'get_champion_catalog',
      )
      cached = {
        version: res.version,
        byId: new Map(res.champions.map((c) => [c.id, c])),
      }
      return cached
    })().catch(() => {
      cached = { version: FALLBACK_VERSION, byId: new Map() }
      return cached
    })
  }
  return pending
}

/** Имя чемпиона (после загрузки каталога; до — "#id"). */
export function getChampionName(id: number): string {
  return cached?.byId.get(id)?.name ?? `#${id}`
}

export function getChampionIconUrl(id: number): string | null {
  const meta = cached?.byId.get(id)
  if (!meta || !cached) return null
  return `https://ddragon.leagueoflegends.com/cdn/${cached.version}/img/champion/${meta.iconName}.png`
}

export function getItemIconUrl(id: number): string {
  return `https://ddragon.leagueoflegends.com/cdn/${cached?.version ?? FALLBACK_VERSION}/img/item/${id}.png`
}

export function getAllChampions(): ChampionMeta[] {
  return cached ? Array.from(cached.byId.values()) : []
}

/** Версия Data Dragon (= текущий патч), которую использует приложение. */
export function getCatalogVersion(): string | null {
  return cached?.version ?? null
}

/** Короткий патч "16.12" из полной версии "16.12.1". */
export function patchOf(version: string | null | undefined): string | null {
  if (!version) return null
  const parts = version.split('.')
  return parts.length >= 2 ? `${parts[0]}.${parts[1]}` : version
}

/** Хук: грузит каталог один раз и возвращает true когда готов (компонент перерендерится). */
export function useChampionCatalog(): boolean {
  const [loaded, setLoaded] = useState(cached !== null)
  useEffect(() => {
    if (cached) return
    let active = true
    void loadCatalog().then(() => {
      if (active) setLoaded(true)
    })
    return () => {
      active = false
    }
  }, [])
  return loaded
}
