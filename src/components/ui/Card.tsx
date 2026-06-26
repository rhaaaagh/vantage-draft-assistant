import React from 'react'

interface CardProps {
  /** Заголовок карточки (необязателен). */
  title?: React.ReactNode
  /** Действие справа в шапке (кнопка, фильтр). */
  action?: React.ReactNode
  /** Убрать внутренний паддинг (для таблиц во всю ширину). */
  flush?: boolean
  className?: string
  children: React.ReactNode
}

/** Базовая карточка: surface + border 1px + radius 12 + паддинг 16. */
export const Card: React.FC<CardProps> = ({ title, action, flush, className, children }) => (
  <section className={`ui-card${flush ? ' ui-card--flush' : ''}${className ? ` ${className}` : ''}`}>
    {(title || action) && (
      <div className="ui-card__head">
        {title ? <h3 className="ui-card__title">{title}</h3> : <span />}
        {action && <div className="ui-card__action">{action}</div>}
      </div>
    )}
    {children}
  </section>
)
