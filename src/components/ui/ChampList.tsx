import React from 'react'
import { wrClass } from './wr'

export interface ChampListItem {
  championId: number
  name: string
  games: number
  /** Винрейт 0..100. */
  winRate: number
  icon: string | null
}

/** Список чемпионов: иконка, ник, число игр, винрейт цветом по порогу. */
export const ChampList: React.FC<{ items: ChampListItem[] }> = ({ items }) => (
  <div className="ui-champlist">
    {items.map((c) => (
      <div key={c.championId} className="ui-champlist__row">
        {c.icon ? (
          <img className="ui-champlist__icon" src={c.icon} alt="" width={28} height={28} />
        ) : (
          <span className="ui-champlist__icon" />
        )}
        <span className="ui-champlist__name">{c.name}</span>
        <span className="ui-champlist__games">{c.games} игр</span>
        <span className={`ui-champlist__wr ${wrClass(c.winRate)}`}>{Math.round(c.winRate)}%</span>
      </div>
    ))}
  </div>
)
