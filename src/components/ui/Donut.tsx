import React from 'react'

interface DonutProps {
  wins: number
  losses: number
  /** Диаметр в px. */
  size?: number
  /** Толщина кольца в px. */
  stroke?: number
}

/**
 * Донат винрейта: всё кольцо — поражения (--loss), дуга поверх — победы (--win).
 * В центре крупный процент и число игр. Плоско, без теней и легенды по кругу.
 * Чистый SVG (как LineChart), винрейт округляется до одного знака.
 */
export const Donut: React.FC<DonutProps> = ({ wins, losses, size = 120, stroke = 12 }) => {
  const total = wins + losses
  const wr = total > 0 ? (wins / total) * 100 : 0
  const center = size / 2
  const r = (size - stroke) / 2
  const c = 2 * Math.PI * r
  const winLen = total > 0 ? (wins / total) * c : 0

  return (
    <div className="ui-donut" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} role="img" aria-label={`Винрейт ${wr.toFixed(1)}%`}>
        <circle
          className="ui-donut__track"
          cx={center}
          cy={center}
          r={r}
          fill="none"
          strokeWidth={stroke}
        />
        {total > 0 && (
          <circle
            className="ui-donut__win"
            cx={center}
            cy={center}
            r={r}
            fill="none"
            strokeWidth={stroke}
            strokeDasharray={`${winLen.toFixed(2)} ${(c - winLen).toFixed(2)}`}
            transform={`rotate(-90 ${center} ${center})`}
          />
        )}
      </svg>
      <div className="ui-donut__center">
        <span className="ui-donut__pct num">{total > 0 ? `${wr.toFixed(1)}%` : '—'}</span>
        <span className="ui-donut__games num">{total} игр</span>
      </div>
    </div>
  )
}
