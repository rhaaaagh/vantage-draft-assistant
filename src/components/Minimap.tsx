import React, { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { getCatalogVersion } from '../api/championCatalog'
import './Minimap.css'

/**
 * Точка на миникарте. Координаты — доли 0..1 в ИГРОВОЙ системе (как отдаёт
 * 0 = низ-лево синей базы, y растёт вверх). Компонент сам инвертирует
 * y под экранные координаты SVG. `minute` используется слайдером времени.
 */
export interface MinimapPoint {
  x: number
  y: number
  minute?: number
  /** Произвольная категория для окраски (см. `colorFor`). */
  kind?: string
  /** Подсказка при наведении. */
  title?: string
  /** Короткая подпись поверх точки (напр. номер смерти «1», «2»…). */
  label?: string
}

/** Ломаная (маршрут) из точек — например, лесной клир. */
export interface MinimapRoute {
  points: MinimapPoint[]
  /** Цвет линии; по умолчанию нейтральный. */
  color?: string
  title?: string
  /**
   * Рисовать направление: стрелки вдоль линии + зелёная метка «старт» в начале.
   * Без этого по точкам не понять, откуда и куда идёт маршрут.
   */
  directed?: boolean
}

/**
 * Один временно́й «кадр» для оконного просмотра (напр. 5-минутный бин позиций).
 * В отличие от накопительного слайдера, показывается ТОЛЬКО выбранный бин —
 * это убирает перегрузку карты сотнями точек к поздней игре.
 */
export interface MinimapBin {
  /** Человекочитаемая подпись окна, напр. «10–15 мин». */
  label: string
  points: MinimapPoint[]
}

export interface MinimapProps {
  /** Точки для отрисовки. */
  points?: MinimapPoint[]
  /** Маршруты (ломаные). */
  routes?: MinimapRoute[]
  /**
   * Оконный режим: набор временны́х бинов. Слайдер выбирает ОДИН бин и
   * показывает только его точки (не накопительно). Имеет приоритет над
   * `points` + `showTimeSlider` для отображаемых точек.
   */
  bins?: MinimapBin[]
  /** Размер квадрата в пикселях. */
  size?: number
  /**
   * Показывать слайдер времени. Требует, чтобы у точек был `minute`. Слайдер
   * фильтрует точки/маршруты по `<= выбранной минуты` (накопительно).
   */
  showTimeSlider?: boolean
  /** Цвет точки по её `kind`. Переопределяет дефолтную палитру. */
  colorFor?: (kind: string | undefined) => string
  /** Радиус точки в пикселях. */
  pointRadius?: number
  /** Подпись над картой. */
  caption?: string
}

const MAP_VERSION_FALLBACK = '16.12.1'

/**
 * Актуальный миникап Summoner's Rift (ПОСЛЕ реворка карты) — из CommunityDragon
 * (тег `latest`, обновляется с патчами). Data Dragon `map11.png` — это старый
 * арт до реворка, поэтому его используем только как запасной.
 */
const CURRENT_MAP_URL =
  'https://raw.communitydragon.org/latest/game/assets/maps/info/map11/2dlevelminimap_base_baron1.png'

/** Запасной бэкдроп — Data Dragon map11 (СТАРЫЙ арт). Только если CDragon недоступен. */
function legacyMapUrl(): string {
  const v = getCatalogVersion() ?? MAP_VERSION_FALLBACK
  return `https://ddragon.leagueoflegends.com/cdn/${v}/img/map/map11.png`
}

const DEFAULT_PALETTE: Record<string, string> = {
  death: '#e0507a',
  position: '#56b6ff',
  blue: '#56b6ff',
  red: '#e0507a',
  neutral: '#d8b14a',
}

function defaultColor(kind: string | undefined): string {
  return (kind && DEFAULT_PALETTE[kind]) || DEFAULT_PALETTE.neutral
}

/**
 * Переиспользуемая миникарта Summoner's Rift: бэкдроп map11 + точки/маршруты +
 * опциональный слайдер времени. Самодостаточна — другие треки могут давать свои
 * точки (смерти, позиции, варды и т. п.) через единый проп `points`.
 */
export const Minimap: React.FC<MinimapProps> = ({
  points = [],
  routes = [],
  bins,
  size = 300,
  showTimeSlider = false,
  colorFor,
  pointRadius = 4,
  caption,
}) => {
  const { t } = useTranslation()
  const windowed = bins != null && bins.length > 0
  // Диапазон минут для (накопительного) слайдера — максимум по всем точкам/маршрутам.
  const maxMinute = useMemo(() => {
    let m = 0
    for (const p of points) m = Math.max(m, p.minute ?? 0)
    for (const r of routes) for (const p of r.points) m = Math.max(m, p.minute ?? 0)
    return m
  }, [points, routes])

  const [minute, setMinute] = useState<number>(maxMinute)
  // Если данные сменились и текущая минута больше нового максимума — поджимаем.
  const cappedMinute = Math.min(minute, maxMinute)
  const activeMinute = showTimeSlider ? cappedMinute : maxMinute

  // Оконный режим: индекс выбранного бина (показываем только его точки).
  const [binIndex, setBinIndex] = useState<number>(0)
  const activeBin = windowed ? Math.min(binIndex, bins.length - 1) : 0

  // Фон карты: актуальный CDragon, с откатом на старый ddragon при ошибке загрузки.
  const [mapSrc, setMapSrc] = useState<string>(CURRENT_MAP_URL)

  const pickColor = colorFor ?? defaultColor

  // Экранные координаты: x как есть, y инвертирован (игровой 0 = низ карты).
  const sx = (x: number) => x * size
  const sy = (y: number) => (1 - y) * size

  const visiblePoints = windowed
    ? bins[activeBin].points
    : showTimeSlider
      ? points.filter((p) => (p.minute ?? 0) <= activeMinute)
      : points

  return (
    <div className="minimap">
      {caption && <div className="minimap-caption">{caption}</div>}
      <svg
        className="minimap-svg"
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        role="img"
        aria-label={caption ?? t('scout.minimapLabel')}
      >
        <image
          href={mapSrc}
          x={0}
          y={0}
          width={size}
          height={size}
          preserveAspectRatio="none"
          style={{ pointerEvents: 'none' }}
          onError={() => setMapSrc((s) => (s === CURRENT_MAP_URL ? legacyMapUrl() : s))}
        />
        {/* Затемнение для контраста точек поверх яркой карты. */}
        <rect x={0} y={0} width={size} height={size} className="minimap-dim" style={{ pointerEvents: 'none' }} />

        {/* Стрелка направления для маршрутов: наследует цвет линии (context-stroke). */}
        <defs>
          <marker
            id="mm-arrow"
            viewBox="0 0 10 10"
            refX="6"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
            markerUnits="userSpaceOnUse"
          >
            <path d="M0,0 L10,5 L0,10 z" fill="context-stroke" />
          </marker>
        </defs>

        {/* Маршруты (ломаные). */}
        {routes.map((route, ri) => {
          const pts = showTimeSlider
            ? route.points.filter((p) => (p.minute ?? 0) <= activeMinute)
            : route.points
          if (pts.length < 2) return null
          const color = route.color ?? DEFAULT_PALETTE.neutral
          const d = pts
            .map((p, i) => `${i ? 'L' : 'M'}${sx(p.x).toFixed(1)},${sy(p.y).toFixed(1)}`)
            .join(' ')
          const start = pts[0]
          const end = pts[pts.length - 1]
          return (
            <g key={`route-${ri}`}>
              <path
                d={d}
                fill="none"
                stroke={color}
                strokeWidth={2.5}
                strokeOpacity={0.9}
                strokeLinecap="round"
                strokeLinejoin="round"
                markerMid={route.directed ? 'url(#mm-arrow)' : undefined}
                markerEnd={route.directed ? 'url(#mm-arrow)' : undefined}
              >
                {route.title && <title>{route.title}</title>}
              </path>
              {route.directed && (
                <>
                  {/* Конец — куда пришёл (маленькая точка цвета маршрута). */}
                  <circle cx={sx(end.x)} cy={sy(end.y)} r={3} fill={color} stroke="rgba(0,0,0,0.55)" strokeWidth={0.75} />
                  {/* Старт — откуда начал: точка цвета маршрута с белым ободком
                      (для нескольких линий цвет = матч, виден старт каждой). */}
                  <circle cx={sx(start.x)} cy={sy(start.y)} r={5} fill={color} stroke="#fff" strokeWidth={1.75}>
                    <title>
                      {route.title
                        ? t('scout.minimapRouteStartNamed', { title: route.title })
                        : t('scout.minimapRouteStart')}
                    </title>
                  </circle>
                </>
              )}
            </g>
          )
        })}

        {/* Точки. */}
        {visiblePoints.map((p, i) => (
          <g key={`pt-${i}`}>
            <circle
              cx={sx(p.x)}
              cy={sy(p.y)}
              r={pointRadius}
              fill={pickColor(p.kind)}
              fillOpacity={p.label ? 0.92 : 0.8}
              stroke="rgba(0,0,0,0.55)"
              strokeWidth={p.label ? 1 : 0.75}
            >
              {p.title && <title>{p.title}</title>}
            </circle>
            {p.label && (
              <text
                x={sx(p.x)}
                y={sy(p.y)}
                className="minimap-point-label"
                textAnchor="middle"
                dominantBaseline="central"
                style={{ fontSize: Math.max(9, pointRadius * 1.3) }}
              >
                {p.label}
                {p.title && <title>{p.title}</title>}
              </text>
            )}
          </g>
        ))}
      </svg>

      {windowed ? (
        bins.length > 1 && (
          <div className="minimap-slider">
            <input
              type="range"
              min={0}
              max={bins.length - 1}
              value={activeBin}
              onChange={(e) => setBinIndex(Number(e.target.value))}
              aria-label={t('scout.minimapTimeWindow')}
            />
            <span className="minimap-slider-label num">{bins[activeBin].label}</span>
          </div>
        )
      ) : (
        showTimeSlider &&
        maxMinute > 0 && (
          <div className="minimap-slider">
            <input
              type="range"
              min={0}
              max={maxMinute}
              value={activeMinute}
              onChange={(e) => setMinute(Number(e.target.value))}
              aria-label={t('scout.minimapTimeMinutes')}
            />
            <span className="minimap-slider-label num">{t('scout.minimapMinutes', { count: activeMinute })}</span>
          </div>
        )
      )}
      {windowed && bins.length === 1 && (
        <div className="minimap-slider">
          <span className="minimap-slider-label num">{bins[0].label}</span>
        </div>
      )}
    </div>
  )
}

export default Minimap
