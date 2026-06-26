import { useTranslation } from 'react-i18next'
import type { DraftAnalytics } from '../domain/draft'

export const AnalyticsSummary: React.FC<{ analytics: DraftAnalytics }> = ({ analytics }) => {
  const { t } = useTranslation()
  if (!analytics || typeof analytics !== 'object') return null
  // Метрики без реальных данных приходят как null и не показываются.
  const blueWinProbability =
    typeof analytics.blueWinProbability === 'number' ? analytics.blueWinProbability : null
  const redWinProbability =
    typeof analytics.redWinProbability === 'number' ? analytics.redWinProbability : null
  const blueSynergyScore =
    typeof analytics.blueSynergyScore === 'number' ? analytics.blueSynergyScore : null
  const redSynergyScore =
    typeof analytics.redSynergyScore === 'number' ? analytics.redSynergyScore : null
  const blueDamageProfile = analytics.blueDamageProfile && typeof analytics.blueDamageProfile === 'object'
    ? { ad: Number(analytics.blueDamageProfile.ad) || 0, ap: Number(analytics.blueDamageProfile.ap) || 0 }
    : { ad: 0.5, ap: 0.5 }
  const redDamageProfile = analytics.redDamageProfile && typeof analytics.redDamageProfile === 'object'
    ? { ad: Number(analytics.redDamageProfile.ad) || 0, ap: Number(analytics.redDamageProfile.ap) || 0 }
    : { ad: 0.5, ap: 0.5 }
  const blueWeaknesses = Array.isArray(analytics.blueWeaknesses) ? analytics.blueWeaknesses : []
  const redWeaknesses = Array.isArray(analytics.redWeaknesses) ? analytics.redWeaknesses : []

  const showWinRow = blueWinProbability !== null || blueSynergyScore !== null

  return (
    <div className="analytics-summary">
      {showWinRow && (
        <div className="analytics-row">
          {blueWinProbability !== null && (
            <div className="analytics-card">
              <h3>{t('draft.analytics.winChance')}</h3>
              <p><strong>{t('draft.analytics.myTeam')}</strong> {(blueWinProbability * 100).toFixed(0)}%</p>
              {redWinProbability !== null && (
                <p><strong>{t('draft.analytics.enemyTeam')}</strong> {(redWinProbability * 100).toFixed(0)}%</p>
              )}
            </div>
          )}
          {blueSynergyScore !== null && redSynergyScore !== null && (
            <div className="analytics-card">
              <h3>{t('draft.analytics.synergy')}</h3>
              <p>{t('draft.analytics.synergyLine', { blue: (blueSynergyScore * 100).toFixed(0), red: (redSynergyScore * 100).toFixed(0) })}</p>
            </div>
          )}
        </div>
      )}
      {!showWinRow && (
        <p className="field-help">
          {t('draft.analytics.noWinSynergy')}
        </p>
      )}

      <div className="analytics-row">
        <div className="analytics-card">
          <h3>{t('draft.analytics.damageMyTeam')}</h3>
          <p>
            {t('draft.analytics.damageLine', { ad: (blueDamageProfile.ad * 100).toFixed(0), ap: (blueDamageProfile.ap * 100).toFixed(0) })}
          </p>
          {blueDamageProfile.ad > 0.75 && (
            <p className="analytics-warning">{t('draft.analytics.highAdMine')}</p>
          )}
          {blueDamageProfile.ap > 0.75 && (
            <p className="analytics-warning">{t('draft.analytics.highApMine')}</p>
          )}
        </div>
        <div className="analytics-card">
          <h3>{t('draft.analytics.damageEnemy')}</h3>
          <p>
            {t('draft.analytics.damageLine', { ad: (redDamageProfile.ad * 100).toFixed(0), ap: (redDamageProfile.ap * 100).toFixed(0) })}
          </p>
          {redDamageProfile.ad > 0.75 && (
            <p className="analytics-warning">{t('draft.analytics.highAdEnemy')}</p>
          )}
          {redDamageProfile.ap > 0.75 && (
            <p className="analytics-warning">{t('draft.analytics.highApEnemy')}</p>
          )}
        </div>
      </div>
      <p className="field-help" style={{ marginTop: 0 }}>
        {t('draft.analytics.damageNote')}
      </p>

      <div className="analytics-row">
        <div className="analytics-card">
          <h3>{t('draft.analytics.weaknessesMine')}</h3>
          {blueWeaknesses.length === 0 ? (
            <p>{t('draft.analytics.noWeaknesses')}</p>
          ) : (
            <ul>
              {blueWeaknesses.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          )}
        </div>
        <div className="analytics-card">
          <h3>{t('draft.analytics.weaknessesEnemy')}</h3>
          {redWeaknesses.length === 0 ? (
            <p>{t('draft.analytics.noWeaknesses')}</p>
          ) : (
            <ul>
              {redWeaknesses.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  )
}
