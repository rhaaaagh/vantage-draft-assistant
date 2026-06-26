import React, { useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { DraftAnalysisResult } from '../domain/draft'
import { PickHelper } from './PickHelper'
import { LiveDraftPicks } from './LiveDraftPicks'
import { CurrentMatch } from './CurrentMatch'
import { DraftAssistant } from './DraftAssistant'

interface DraftScreenProps {
  draftResult: DraftAnalysisResult | null
  setDraftResult: React.Dispatch<React.SetStateAction<DraftAnalysisResult | null>>
}

/**
 * Экран вкладки «Драфт / Матч» — всё для текущей/предстоящей игры в одном месте:
 * — живые рекомендации пиков из клиента LoL (LCU), пока идёт чемпион-селект
 *   (после старта игры показывают последний драфт, пока не начнётся следующий);
 * — составы текущей игры и ранги (Текущий матч), как только Spectator её увидит;
 * — ниже ручной подбор пика (PickHelper) и анализатор чемпион-селекта по LCU.
 */
export const DraftScreen: React.FC<DraftScreenProps> = (props) => {
  const { t } = useTranslation()
  const [liveActive, setLiveActive] = useState(false)
  return (
    <>
      <LiveDraftPicks onActiveChange={setLiveActive} />
      {liveActive && (
        <p className="ui-muted" style={{ margin: '0 0 0.5rem' }}>
          {t('draft.manualModeHint')}
        </p>
      )}
      <CurrentMatch />
      <PickHelper />
      <DraftAssistant {...props} />
    </>
  )
}

export default DraftScreen
