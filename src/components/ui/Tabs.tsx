export interface TabItem<T extends string = string> {
  id: T
  label: string
}

interface TabsProps<T extends string> {
  items: TabItem<T>[]
  active: T
  onChange: (id: T) => void
}

/** Вкладки с подчёркиванием активного. Без капсул и заливки. */
export function Tabs<T extends string>({ items, active, onChange }: TabsProps<T>) {
  return (
    <div className="ui-tabs" role="tablist">
      {items.map((it) => (
        <button
          key={it.id}
          type="button"
          role="tab"
          aria-selected={it.id === active}
          className={`ui-tab${it.id === active ? ' is-active' : ''}`}
          onClick={() => onChange(it.id)}
        >
          {it.label}
        </button>
      ))}
    </div>
  )
}
