import React, { createContext, useContext, useEffect } from 'react'

/** Контекст правого рейла: активная вкладка кладёт сюда контекстный контент. */
const RailContext = createContext<(node: React.ReactNode) => void>(() => {})

export const RailProvider = RailContext.Provider

/**
 * Хук: задаёт контент правого рейла на время жизни вкладки.
 * При смене node — обновляет, при размонтировании — очищает.
 */
export function useRail(node: React.ReactNode, deps: React.DependencyList): void {
  const setRail = useContext(RailContext)
  useEffect(() => {
    setRail(node)
    return () => setRail(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)
}
