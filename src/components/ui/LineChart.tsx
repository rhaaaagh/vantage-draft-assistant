import React from 'react'

interface LineChartProps {
  /** Значения по оси Y в порядке слева-направо. */
  values: number[]
  height?: number
  /** Подпись при отсутствии данных. */
  emptyLabel?: string
}

/**
 * Минималистичный линейный график: одна красная линия, без заливки,
 * без лишней сетки (только базовая горизонталь). Для динамики (ранг, KDA).
 */
export const LineChart: React.FC<LineChartProps> = ({
  values,
  height = 64,
  emptyLabel = 'Нет данных',
}) => {
  if (values.length < 2) {
    return <div className="ui-linechart__empty">{emptyLabel}</div>
  }

  const W = 100 // вьюпорт в условных единицах, тянется по ширине
  const H = 100
  const pad = 6
  const min = Math.min(...values)
  const max = Math.max(...values)
  const span = max - min || 1
  const stepX = (W - pad * 2) / (values.length - 1)

  const pts = values.map((v, i) => {
    const x = pad + i * stepX
    const y = pad + (H - pad * 2) * (1 - (v - min) / span)
    return [x, y] as const
  })

  const d = pts.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`).join(' ')
  const last = pts[pts.length - 1]

  return (
    <svg
      className="ui-linechart"
      viewBox={`0 0 ${W} ${H}`}
      height={height}
      preserveAspectRatio="none"
      role="img"
    >
      <line className="ui-linechart__grid" x1={pad} y1={H - pad} x2={W - pad} y2={H - pad} />
      <path className="ui-linechart__line" d={d} vectorEffect="non-scaling-stroke" />
      <circle className="ui-linechart__dot" cx={last[0]} cy={last[1]} r={2.4} />
    </svg>
  )
}
