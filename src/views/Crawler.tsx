import React, { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { getCrawlStatus, startCrawl, stopCrawl } from '../api/crawlerApi'
import type { CrawlStatus } from '../api/crawlerApi'

const POLL_MS = 2000

function getCreds(): { apiKey: string; region: string } {
  if (typeof window === 'undefined') return { apiKey: '', region: 'ru' }
  return {
    apiKey: window.localStorage.getItem('lolda_riot_api_key') ?? '',
    region: window.localStorage.getItem('lolda_region') ?? 'ru',
  }
}

function Bar({ done, total, label }: { done: number; total: number; label: string }) {
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0
  return (
    <div className="crawl-bar-block">
      <div className="crawl-bar-label">
        <span>{label}</span>
        <span>{done.toLocaleString('ru')} / {total.toLocaleString('ru')}</span>
      </div>
      <div className="crawl-bar">
        <div className="crawl-bar-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  )
}

export const Crawler: React.FC = () => {
  const { t } = useTranslation()
  const [includeDiamond, setIncludeDiamond] = useState(true)
  const [maxMatches, setMaxMatches] = useState(20000)
  const [reset, setReset] = useState(false)
  const [status, setStatus] = useState<CrawlStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const timer = useRef<number | null>(null)

  const poll = async () => {
    try {
      setStatus(await getCrawlStatus())
    } catch {
      // тихо игнорируем разовые сбои поллинга
    }
  }

  useEffect(() => {
    // Поллинг через таймеры (не синхронный setState в эффекте).
    const first = window.setTimeout(() => void poll(), 0)
    timer.current = window.setInterval(() => void poll(), POLL_MS)
    return () => {
      window.clearTimeout(first)
      if (timer.current) window.clearInterval(timer.current)
    }
  }, [])

  const onStart = async () => {
    setError(null)
    const { apiKey, region } = getCreds()
    if (!apiKey.trim()) {
      setError(t('crawler.needApiKey'))
      return
    }
    try {
      await startCrawl(apiKey, region, includeDiamond, maxMatches, reset)
      void poll()
    } catch (e) {
      setError(String(e))
    }
  }

  const onStop = async () => {
    try {
      await stopCrawl()
      void poll()
    } catch (e) {
      setError(String(e))
    }
  }

  const running = status?.running ?? false
  const matchesDone = status?.matchesDone ?? 0
  const hoursLeft = ((maxMatches - matchesDone) / 1400).toFixed(1)

  return (
    <div className="crawler-view">
      <section className="panel">
        <h2>{t('crawler.title')}</h2>
        <p className="field-help">
          {t('crawler.intro', { diamond: includeDiamond ? t('crawler.introDiamond') : '' })}
        </p>

        <div className="crawl-controls">
          <label className="crawl-check">
            <input
              type="checkbox"
              checked={includeDiamond}
              disabled={running}
              onChange={(e) => setIncludeDiamond(e.target.checked)}
            />
            <span>{t('crawler.includeDiamond')}</span>
          </label>

          <label className="crawl-check">
            <input
              type="checkbox"
              checked={reset}
              disabled={running}
              onChange={(e) => setReset(e.target.checked)}
            />
            <span>{t('crawler.resetDb')}</span>
          </label>

          <label className="crawl-field">
            <span>{t('crawler.targetMatches')}</span>
            <input
              type="number"
              min={100}
              step={1000}
              value={maxMatches}
              disabled={running}
              onChange={(e) => setMaxMatches(Math.max(100, Number(e.target.value) || 0))}
            />
          </label>

          {!running ? (
            <button type="button" className="btn-primary" onClick={() => void onStart()}>
              {matchesDone > 0 ? t('crawler.resume') : t('crawler.start')}
            </button>
          ) : (
            <button type="button" className="btn-secondary" onClick={() => void onStop()}>
              {t('crawler.stop')}
            </button>
          )}
        </div>

        {!running && (
          <p className="field-help">
            {t('crawler.etaHelp', { hours: hoursLeft })}
          </p>
        )}

        {error && <p className="field-help text-danger">{error}</p>}
      </section>

      {status && (
        <section className="panel">
          <div className="crawl-status-head">
            <span className={`crawl-dot ${running ? 'on' : 'off'}`} />
            <span>{status.message}</span>
          </div>
          <div className="crawl-bars">
            <Bar done={status.puuidsDone} total={status.puuidsTotal} label={t('crawler.playersDone')} />
            <Bar done={status.matchesDone} total={Math.max(status.target, status.matchesTotal)} label={t('crawler.matchesDone')} />
          </div>
          {status.lastError && <p className="field-help text-danger">{t('crawler.lastError', { error: status.lastError })}</p>}
        </section>
      )}
    </div>
  )
}
