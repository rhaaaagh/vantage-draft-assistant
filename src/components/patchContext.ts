import { createContext, useContext } from 'react'

export interface PatchState {
  /** Выбранный для показа статистики патч. */
  patch: string
  setPatch: (p: string) => void
  /** Текущий патч (дефолт выбора). */
  current: string
  /** Предыдущий патч (или null). */
  previous: string | null
}

export const PatchContext = createContext<PatchState>({
  patch: '',
  setPatch: () => {},
  current: '',
  previous: null,
})

/** Хук доступа к выбранному патчу из любой вкладки. */
export const usePatch = () => useContext(PatchContext)
