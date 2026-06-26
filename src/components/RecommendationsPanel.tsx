import { useTranslation } from 'react-i18next'
import type { BuildItem, BuildRecommendation, PickSuggestion } from '../domain/draft'
import { getItemIconUrl, useChampionCatalog } from '../api/championCatalog'

interface Props {
  best: PickSuggestion[]
  worst: PickSuggestion[]
  builds: BuildRecommendation | null
}

function ItemRow({ items }: { items: BuildItem[] }) {
  const { t } = useTranslation()
  if (!Array.isArray(items) || items.length === 0) {
    return <p className="build-items">{t('draft.recommendations.noBuildData')}</p>
  }
  return (
    <p className="build-items" style={{ display: 'flex', gap: '0.3rem', alignItems: 'center', flexWrap: 'wrap' }}>
      {items.map((item, i) => (
        <img
          key={`${item.id}-${i}`}
          src={getItemIconUrl(item.id)}
          alt={item.name || t('draft.recommendations.itemFallback', { id: item.id })}
          title={item.name || t('draft.recommendations.itemFallback', { id: item.id })}
          width={28}
          height={28}
          loading="lazy"
          style={{ borderRadius: 4 }}
        />
      ))}
    </p>
  )
}

export const RecommendationsPanel: React.FC<Props> = ({ best, worst, builds }) => {
  const { t } = useTranslation()
  useChampionCatalog()
  const safeBest = Array.isArray(best) ? best : []
  const safeWorst = Array.isArray(worst) ? worst : []
  const noStats = safeBest.length === 0 && safeWorst.length === 0
  return (
    <div className="recommendations">
      {noStats && (
        <p className="field-help">
          {t('draft.recommendations.noStats')}
        </p>
      )}
      {safeBest.length > 0 && (
        <div className="recommendations-column">
          <h3>{t('draft.recommendations.bestPicks')}</h3>
          <ul className="recommendation-list">
            {safeBest.map((pick, i) => (
              <li key={pick?.championId ?? i} className="recommendation-item good">
                <div className="recommendation-title">
                  {pick?.championName ?? '?'}{' '}
                  <span className="pill">
                    {t('draft.recommendations.winrateStat', { winrate: ((pick?.winrate ?? 0) * 100).toFixed(1), games: pick?.games ?? 0 })}
                  </span>
                </div>
                <div className="recommendation-reason">{pick?.reason ?? ''}</div>
              </li>
            ))}
          </ul>
        </div>
      )}

      {safeWorst.length > 0 && (
        <div className="recommendations-column">
          <h3>{t('draft.recommendations.worstPicks')}</h3>
          <ul className="recommendation-list">
            {safeWorst.map((pick, i) => (
              <li key={pick?.championId ?? i} className="recommendation-item bad">
                <div className="recommendation-title">
                  {pick?.championName ?? '?'}{' '}
                  <span className="pill">
                    {t('draft.recommendations.winrateStat', { winrate: ((pick?.winrate ?? 0) * 100).toFixed(1), games: pick?.games ?? 0 })}
                  </span>
                </div>
                <div className="recommendation-reason">{pick?.reason ?? ''}</div>
              </li>
            ))}
          </ul>
        </div>
      )}

      {builds && typeof builds === 'object' && (
        <div className="recommendations-column builds">
          <h3>{t('draft.recommendations.buildsFor', { champion: builds.championName ?? '?' })}</h3>
          {Array.isArray(builds.bestWinrateBuild) && builds.bestWinrateBuild.length > 0 && (
            <div className="build-section">
              <h4>{t('draft.recommendations.bestByWinrate')}</h4>
              <ItemRow items={builds.bestWinrateBuild} />
            </div>
          )}
          {Array.isArray(builds.mostPopularBuild) && builds.mostPopularBuild.length > 0 && (
            <div className="build-section">
              <h4>{t('draft.recommendations.mostPopular')}</h4>
              <ItemRow items={builds.mostPopularBuild} />
            </div>
          )}
          {Array.isArray(builds.vsEnemyBuild) && builds.vsEnemyBuild.length > 0 && (
            <div className="build-section">
              <h4>{t('draft.recommendations.vsEnemy')}</h4>
              <ItemRow items={builds.vsEnemyBuild} />
            </div>
          )}
        </div>
      )}
    </div>
  )
}
