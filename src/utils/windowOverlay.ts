/**
 * Окно: always-on-top, сохранение и восстановление позиции/размера.
 * Работает только в Tauri (десктоп).
 */

const STORAGE_KEY_BOUNDS = 'lolda_window_bounds'
const STORAGE_KEY_ALWAYS_ON_TOP = 'lolda_always_on_top'

export function isTauriWindow(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

export function getStoredAlwaysOnTop(): boolean {
  if (typeof window === 'undefined') return false
  try {
    const v = window.localStorage.getItem(STORAGE_KEY_ALWAYS_ON_TOP)
    return v === 'true'
  } catch {
    return false
  }
}

export function setStoredAlwaysOnTop(value: boolean): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(STORAGE_KEY_ALWAYS_ON_TOP, String(value))
  } catch {
    // ignore
  }
}

export interface WindowBounds {
  x: number
  y: number
  width: number
  height: number
}

const MIN_WIDTH = 400
const MIN_HEIGHT = 300
const MAX_XY = 100000

export function getStoredBounds(): WindowBounds | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY_BOUNDS)
    if (!raw) return null
    const o = JSON.parse(raw) as unknown
    if (typeof o !== 'object' || o === null) return null
    const { x, y, width, height } = o as Record<string, unknown>
    if (
      typeof x !== 'number' ||
      typeof y !== 'number' ||
      typeof width !== 'number' ||
      typeof height !== 'number'
    )
      return null
    if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(width) || !Number.isFinite(height))
      return null
    if (width < MIN_WIDTH || height < MIN_HEIGHT) return null
    const clampedX = Math.max(-MAX_XY, Math.min(MAX_XY, x))
    const clampedY = Math.max(-MAX_XY, Math.min(MAX_XY, y))
    return { x: clampedX, y: clampedY, width: Math.min(width, 10000), height: Math.min(height, 10000) }
  } catch {
    return null
  }
}

export function setStoredBounds(bounds: WindowBounds): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(STORAGE_KEY_BOUNDS, JSON.stringify(bounds))
  } catch {
    // ignore
  }
}

export async function applyWindowOverlay(): Promise<void> {
  if (!isTauriWindow()) return
  try {
    const { getCurrentWindow, PhysicalPosition, PhysicalSize } = await import(
      '@tauri-apps/api/window'
    )
    const win = getCurrentWindow()
    const alwaysOnTop = getStoredAlwaysOnTop()
    await win.setAlwaysOnTop(alwaysOnTop)
    const bounds = getStoredBounds()
    if (bounds) {
      await win.setPosition(new PhysicalPosition(bounds.x, bounds.y))
      await win.setSize(new PhysicalSize(bounds.width, bounds.height))
    }
  } catch {
    // Игнорируем ошибки окна — не ломаем запуск приложения
  }
}

export async function setAlwaysOnTop(value: boolean): Promise<void> {
  if (!isTauriWindow()) return
  setStoredAlwaysOnTop(value)
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().setAlwaysOnTop(value)
}

export async function saveWindowBounds(): Promise<void> {
  if (!isTauriWindow()) return
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const win = getCurrentWindow()
    const pos = await win.outerPosition()
    const size = await win.outerSize()
    const w = Math.max(MIN_WIDTH, size.width)
    const h = Math.max(MIN_HEIGHT, size.height)
    if (Number.isFinite(pos.x) && Number.isFinite(pos.y) && Number.isFinite(w) && Number.isFinite(h)) {
      setStoredBounds({ x: pos.x, y: pos.y, width: w, height: h })
    }
  } catch {
    // ignore
  }
}

export async function setupWindowBoundsSaving(): Promise<void> {
  if (!isTauriWindow()) return
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const win = getCurrentWindow()
    let timeoutId: ReturnType<typeof setTimeout> | null = null
    const debouncedSave = (): void => {
      if (timeoutId) clearTimeout(timeoutId)
      timeoutId = setTimeout(() => {
        void saveWindowBounds().catch(() => {})
        timeoutId = null
      }, 500)
    }
    win.onMoved(() => debouncedSave())
    win.onResized(() => debouncedSave())
    win.onCloseRequested(async () => {
      await saveWindowBounds().catch(() => {})
    })
  } catch {
    // Игнорируем — не ломаем запуск
  }
}
