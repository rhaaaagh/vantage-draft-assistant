import React from 'react'

type StatTone = 'default' | 'accent' | 'win' | 'loss'

interface StatProps {
  label: string
  value: React.ReactNode
  /** Мелкая подпись под значением. */
  sub?: React.ReactNode
  tone?: StatTone
}

/** Метрика: мелкая серая подпись сверху, крупное число снизу. */
export const Stat: React.FC<StatProps> = ({ label, value, sub, tone = 'default' }) => (
  <div className={`ui-stat${tone !== 'default' ? ` ui-stat--${tone}` : ''}`}>
    <span className="ui-stat__label">{label}</span>
    <span className="ui-stat__value">{value}</span>
    {sub != null && <span className="ui-stat__sub">{sub}</span>}
  </div>
)

/** Горизонтальный ряд метрик. */
export const StatRow: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="ui-statrow">{children}</div>
)
