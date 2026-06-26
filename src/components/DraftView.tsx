import { useTranslation } from 'react-i18next'
import type { DraftState, TeamDraft } from '../domain/draft'
import { getChampionIconUrl, getChampionName, useChampionCatalog } from '../api/championCatalog'

const getChampionDisplay = (id: number): string => getChampionName(id)

/** Извлекает массив ID банов из объекта команды (учитывает разные варианты ключа при сериализации). */
function getTeamBans(team: unknown, override?: number[]): number[] {
  if (override !== undefined && Array.isArray(override)) return override
  if (!team || typeof team !== 'object') return []
  const t = team as Record<string, unknown>
  const b = t.bans ?? t.Bans
  return Array.isArray(b) ? (b as number[]) : []
}

/** Баны для отображения: явные пропы в приоритете, иначе из draft. */
function resolveBans(override: number[] | undefined, team: unknown): number[] {
  if (override !== undefined && Array.isArray(override)) return override
  return getTeamBans(team, undefined)
}

const renderTeam = (
  team: TeamDraft | null | undefined,
  teamLabel: string,
  t: (key: string) => string,
  bansOverride?: number[],
) => {
  if (!team || typeof team !== 'object') return null
  const slots = Array.isArray(team.slots) ? team.slots : []
  const bans = resolveBans(bansOverride, team)
  return (
    <div className="team-column">
      <h3>{teamLabel}</h3>
      <ul className="slot-list">
        {slots.map((slot, index) => {
          const champId = slot && ('championId' in slot ? slot.championId : (slot as { champion_id?: number }).champion_id)
          const name = slot && ('championName' in slot ? slot.championName : (slot as { champion_name?: string }).champion_name)
          const display = name || (champId ? getChampionDisplay(champId) : null)
          const iconUrl = champId ? getChampionIconUrl(champId) : null
          return (
            <li key={index} className="slot">
              <div className="slot-role">{slot?.role ?? '—'}</div>
              <div className="slot-champion">
                {iconUrl && (
                  <img
                    src={iconUrl}
                    alt=""
                    className="champion-icon"
                    width={28}
                    height={28}
                    loading="lazy"
                  />
                )}
                <span className="slot-champion-name">{display || t('draft.draftView.notPicked')}</span>
              </div>
            </li>
          )
        })}
      </ul>
      <div className="bans">
        <span className="bans-label">{t('draft.draftView.bans')}</span>
        {bans.length > 0 ? (
          <ul className="bans-list">
            {bans.map((id, i) => {
              const iconUrl = getChampionIconUrl(id)
              return (
                <li key={i} className="bans-item">
                  {iconUrl && (
                    <img
                      src={iconUrl}
                      alt=""
                      className="champion-icon champion-icon--ban"
                      width={20}
                      height={20}
                      loading="lazy"
                    />
                  )}
                  <span>{getChampionDisplay(id)}</span>
                </li>
              )
            })}
          </ul>
        ) : (
          <span className="bans-empty">—</span>
        )}
      </div>
    </div>
  )
}

export interface DraftViewProps {
  draft: DraftState
  /** Явно переданные баны (если с бэкенда пришли, но из draft не читаются). */
  blueBans?: number[]
  redBans?: number[]
}

export const DraftView: React.FC<DraftViewProps> = ({ draft, blueBans: blueBansProp, redBans: redBansProp }) => {
  const { t } = useTranslation()
  // Перерендер после загрузки каталога чемпионов (имена и иконки).
  useChampionCatalog()
  if (!draft || typeof draft !== 'object') {
    return <div className="draft-view-empty">{t('draft.draftView.noDraftData')}</div>
  }

  const phase = draft.phase as string | undefined
  const isChampSelect = String(phase).toUpperCase() === 'CHAMP_SELECT'
  const hasTeams = draft.blue && draft.red && typeof draft.blue === 'object' && typeof draft.red === 'object'
  const blueBans = resolveBans(blueBansProp, draft.blue)
  const redBans = resolveBans(redBansProp, draft.red)
  const allBans = [...blueBans, ...redBans]

  return (
    <div className="draft-view">
      {!isChampSelect && (
        <p className="field-help" style={{ marginBottom: '0.5rem' }}>
          {t('draft.draftView.notChampSelect')}
        </p>
      )}
      {allBans.length > 0 && (
        <div className="draft-bans-summary">
          <span className="bans-label">{t('draft.draftView.bannedInDraft')}</span>
          <ul className="bans-list">
            {allBans.map((id, i) => {
              const iconUrl = getChampionIconUrl(id)
              return (
                <li key={i} className="bans-item">
                  {iconUrl && (
                    <img src={iconUrl} alt="" className="champion-icon champion-icon--ban" width={20} height={20} loading="lazy" />
                  )}
                  <span>{getChampionDisplay(id)}</span>
                </li>
              )
            })}
          </ul>
        </div>
      )}
      {hasTeams ? (
        <>
          {renderTeam(draft.blue, t('draft.draftView.myTeam'), t, blueBans)}
          {renderTeam(draft.red, t('draft.draftView.enemyTeam'), t, redBans)}
        </>
      ) : (
        <div className="draft-view-empty">{t('draft.draftView.noTeamData')}</div>
      )}
    </div>
  )
}

