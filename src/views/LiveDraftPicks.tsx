import React, { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchLiveDraftRecommendations } from '../api/draftLiveApi'
import type { LiveDraftRecommendations } from '../api/draftLiveApi'
import { getChampionIconUrl, getItemIconUrl, useChampionCatalog } from '../api/championCatalog'
import { Card } from '../components/ui'
import { usePatch } from '../components/patchContext'
import './PickHelper.css'

const ROLES = ['TOP', 'JUNGLE', 'MID', 'BOT', 'SUPPORT'] as const

const POLL_MS = 3500

interface Props {
  /** Сообщает родителю, активен ли сейчас живой режим (в чемпион-селекте). */
  onActiveChange?: (active: boolean) => void
}

/**
 * Живой подбор пика: тянет драфт из клиента LoL (LCU) и показывает рекомендации
 * движка для авто-определённой роли. Если авто-детект роли не сработал —
 * можно выбрать роль вручную (отправляется как roleOverride).
 */
export const LiveDraftPicks: React.FC<Props> = ({ onActiveChange }) => {
  const { t } = useTranslation()
  useChampionCatalog()
  const { patch } = usePatch()
  const [data, setData] = useState<LiveDraftRecommendations | null>(null)
  // Последний реальный драфт — держим на экране после старта игры, пока не
  // начнётся следующий чемпион-селект (чтобы советы не пропадали в самой игре).
  const [lastDraft, setLastDraft] = useState<LiveDraftRecommendations | null>(null)
  const [roleOverride, setRoleOverride] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  // Чтобы не дёргать setState после размонтирования.
  const aliveRef = useRef(true)

  useEffect(() => {
    aliveRef.current = true
    let timer: ReturnType<typeof setTimeout> | undefined

    const tick = async () => {
      try {
        const res = await fetchLiveDraftRecommendations(patch || undefined, roleOverride || undefined)
        if (!aliveRef.current) return
        setData(res)
        // Запоминаем драфт, только пока идёт чемпион-селект; иначе сохранённый
        // снимок остаётся нетронутым до следующего драфта.
        if (res.inChampSelect) setLastDraft(res)
        setError(null)
      } catch (e) {
        if (!aliveRef.current) return
        setError(String(e))
      } finally {
        if (aliveRef.current) timer = setTimeout(() => void tick(), POLL_MS)
      }
    }

    void tick()
    return () => {
      aliveRef.current = false
      if (timer) clearTimeout(timer)
    }
  }, [patch, roleOverride])

  const live = !!data?.inChampSelect
  useEffect(() => {
    onActiveChange?.(live)
  }, [live, onActiveChange])

  // В чемпион-селекте показываем живые данные; после старта игры — последний
  // драфт (заморожённый), пока не появится следующий. Если драфта ещё не было —
  // ничего не рисуем (PickHelper остаётся ручным режимом ниже).
  const shown = live ? data : lastDraft
  if (!shown) return null
  const isFrozen = !live

  const recs = shown.recommendations ?? []
  const autoRole = shown.myRole ?? ''
  const roleKnown = autoRole.length > 0
  const build = shown.build ?? null

  return (
    <Card title={isFrozen ? t('draft.live.titleFrozen') : t('draft.live.titleLive')}>
      {isFrozen && (
        <p className="ui-muted">
          {t('draft.live.frozenHint')}
          {roleKnown ? t('draft.live.frozenRole', { role: t(`roles.${autoRole}`, autoRole) }) : ''}
        </p>
      )}

      {!isFrozen && (
        <>
          <div className="ph-row">
            <span className="ph-label">{t('draft.live.myRole')}</span>
            <div className="ph-roles">
              {ROLES.map((r) => {
                const isActive = (roleOverride ?? autoRole) === r
                return (
                  <button
                    key={r}
                    type="button"
                    className={`ph-role-tab${isActive ? ' is-active' : ''}`}
                    onClick={() => setRoleOverride((prev) => (prev === r ? null : r))}
                  >
                    {t(`roles.${r}`)}
                  </button>
                )
              })}
            </div>
          </div>

          {roleKnown && !roleOverride && (
            <p className="ui-muted">{t('draft.live.roleAuto', { role: t(`roles.${autoRole}`, autoRole) })}</p>
          )}
          {!roleKnown && !roleOverride && (
            <p className="ui-muted">{t('draft.live.roleUnknown')}</p>
          )}
          {roleOverride && (
            <p className="ui-muted">{t('draft.live.roleManual', { role: t(`roles.${roleOverride}`, roleOverride) })}</p>
          )}

          {error && <p className="ui-error">{error}</p>}
        </>
      )}

      {recs.length === 0 && (
        <p className="ui-muted">
          {t('draft.live.noRecs')}
        </p>
      )}

      {recs.length > 0 && (
        <div className="ph-recs">
          {recs.map((r, i) => {
            const icon = getChampionIconUrl(r.championId)
            return (
              <div className="ph-rec" key={r.championId}>
                <span className="ph-rec-rank num">{i + 1}</span>
                {icon && <img src={icon} alt="" width={36} height={36} className="ph-rec-icon" />}
                <div className="ph-rec-main">
                  <span className="ph-rec-name">{r.championName}</span>
                  <span className="ph-rec-reason">{r.reason}</span>
                </div>
                <div className="ph-rec-score">
                  <div className="ph-rec-bar">
                    <div className="ph-rec-bar-fill" style={{ width: `${Math.round(r.score)}%` }} />
                  </div>
                  <span className="num">{Math.round(r.score)}</span>
                </div>
              </div>
            )
          })}
        </div>
      )}

      {build && (
        <div className="ph-build">
          <div className="ph-build-head">
            {(() => {
              const champIcon = getChampionIconUrl(build.championId)
              return champIcon ? (
                <img src={champIcon} alt="" width={28} height={28} className="ph-build-champ" />
              ) : null
            })()}
            <span className="ph-build-title">{t('draft.live.build', { champion: build.championName })}</span>
            <span className="ph-build-role">{t(`roles.${build.role}`, build.role)}</span>
          </div>

          {build.firstItems.length > 0 && (
            <div className="ph-build-line">
              <span className="ph-build-label">{t('draft.live.buildStart')}</span>
              <div className="ph-build-items">
                {build.firstItems.map((it) => (
                  <img
                    key={it.itemId}
                    src={getItemIconUrl(it.itemId)}
                    alt=""
                    width={30}
                    height={30}
                    className="ph-build-item"
                    title={t('draft.live.gamesTooltip', { count: it.games })}
                  />
                ))}
              </div>
            </div>
          )}

          {build.boots.length > 0 && (
            <div className="ph-build-line">
              <span className="ph-build-label">{t('draft.live.buildBoots')}</span>
              <div className="ph-build-items">
                {build.boots.map((it) => (
                  <img
                    key={it.itemId}
                    src={getItemIconUrl(it.itemId)}
                    alt=""
                    width={30}
                    height={30}
                    className="ph-build-item"
                    title={t('draft.live.gamesTooltip', { count: it.games })}
                  />
                ))}
              </div>
            </div>
          )}

          {build.buildPath.length > 0 && (
            <div className="ph-build-line">
              <span className="ph-build-label">{t('draft.live.buildOrder')}</span>
              <div className="ph-build-path">
                {build.buildPath.map((bs, i) => {
                  const top = bs.items[0]
                  if (!top) return null
                  return (
                    <React.Fragment key={bs.slot}>
                      {i > 0 && <span className="ph-build-arrow">›</span>}
                      <img
                        src={getItemIconUrl(top.itemId)}
                        alt=""
                        width={30}
                        height={30}
                        className="ph-build-item"
                        title={t('draft.live.gamesTooltip', { count: top.games })}
                      />
                    </React.Fragment>
                  )
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </Card>
  )
}

export default LiveDraftPicks
