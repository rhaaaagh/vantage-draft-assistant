import React from 'react'

export interface MatchRowProps {
  win: boolean
  championIcon: string | null
  championName: string
  kills: number
  deaths: number
  assists: number
  cs: number
  csPerMin?: string
  /** Урон по чемпионам (если есть в данных). */
  damage?: number | null
  items: number[]
  itemIconUrl: (id: number) => string
  /** Длительность игры в секундах. */
  durationSec: number
  open?: boolean
  onClick?: () => void
}

function kdaRatio(k: number, d: number, a: number): string {
  return d === 0 ? 'Perfect' : ((k + a) / d).toFixed(2)
}

/**
 * Строка матча: цветная полоска win/loss слева, иконка чемпиона, KDA, CS,
 * урон, предметы, время справа. Кликабельна (раскрытие деталей).
 */
export const MatchRow: React.FC<MatchRowProps> = ({
  win,
  championIcon,
  championName,
  kills,
  deaths,
  assists,
  cs,
  csPerMin,
  damage,
  items,
  itemIconUrl,
  durationSec,
  onClick,
}) => {
  const mins = Math.floor(durationSec / 60)
  const secs = durationSec % 60
  return (
    <button
      type="button"
      className={`ui-matchrow ${win ? 'ui-matchrow--win' : 'ui-matchrow--loss'}`}
      onClick={onClick}
    >
      <span className="ui-matchrow__stripe" />

      <span className="ui-matchrow__champ">
        {championIcon && <img src={championIcon} alt="" width={36} height={36} />}
        <span className="ui-matchrow__champ-meta">
          <span className="ui-matchrow__result">{win ? 'Победа' : 'Поражение'}</span>
          <span className="ui-matchrow__champ-name">{championName}</span>
        </span>
      </span>

      <span className="ui-cell">
        <span className="ui-cell__main">
          {kills} / <span className="ui-matchrow__deaths">{deaths}</span> / {assists}
        </span>
        <span className="ui-cell__sub">{kdaRatio(kills, deaths, assists)} KDA</span>
      </span>

      <span className="ui-cell">
        <span className="ui-cell__main">{cs} CS</span>
        {csPerMin && <span className="ui-cell__sub">{csPerMin}/мин</span>}
      </span>

      <span className="ui-cell">
        {damage != null ? (
          <>
            <span className="ui-cell__main">{damage.toLocaleString('ru')}</span>
            <span className="ui-cell__sub">урон</span>
          </>
        ) : (
          <span className="ui-cell__sub">—</span>
        )}
      </span>

      <span className="ui-matchrow__items">
        {items.map((id, j) => (
          <img key={`${id}-${j}`} src={itemIconUrl(id)} alt="" width={24} height={24} />
        ))}
      </span>

      <span className="ui-matchrow__time">
        {mins}:{secs.toString().padStart(2, '0')}
      </span>
    </button>
  )
}
