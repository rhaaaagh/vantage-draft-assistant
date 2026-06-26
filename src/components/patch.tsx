import React, { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { getPatchOptions } from '../api/patchApi'
import { PatchContext, usePatch } from './patchContext'
import './patch.css'

/**
 * Глобальный выбор патча. По умолчанию (строго ручной режим) — текущий патч;
 * пользователь может вручную переключиться на предыдущий через селектор в шапке.
 */
export const PatchProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [current, setCurrent] = useState('')
  const [previous, setPrevious] = useState<string | null>(null)
  const [patch, setPatch] = useState('')

  useEffect(() => {
    getPatchOptions()
      .then((o) => {
        setCurrent(o.current)
        setPrevious(o.previous)
        // Дефолт — текущий патч. Если выбор ещё не сделан.
        setPatch((p) => p || o.current)
      })
      .catch(() => {})
  }, [])

  return (
    <PatchContext.Provider value={{ patch, setPatch, current, previous }}>
      {children}
    </PatchContext.Provider>
  )
}

/** Селектор патча для шапки. Скрыт, пока патч неизвестен (например, в браузере). */
export const PatchSelector: React.FC = () => {
  const { t } = useTranslation()
  const { patch, setPatch, current, previous } = usePatch()
  if (!current) return null

  const options = previous ? [current, previous] : [current]

  return (
    <label className="patch-selector">
      <span className="patch-selector__label">{t('misc.patch.label')}</span>
      <select
        className="patch-selector__select"
        value={patch}
        onChange={(e) => setPatch(e.target.value)}
      >
        {options.map((p) => (
          <option key={p} value={p}>
            {p}
            {p === current ? t('misc.patch.newSuffix') : ''}
          </option>
        ))}
      </select>
    </label>
  )
}
