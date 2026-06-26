import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import enBase from './locales/en.json'
import ruBase from './locales/ru.json'

/**
 * Интернационализация (react-i18next).
 *
 * Словари собираются из двух источников на каждый язык:
 *  • базовый `locales/<lang>.json` (app/nav/common/settings);
 *  • по-областные файлы `locales/<lang>/<область>.json` (scout/profile/champion/…),
 *    которые подхватываются АВТОМАТИЧЕСКИ через import.meta.glob. У каждой области
 *    свой top-level ключ (напр. `{ "scout": { … } }`), поэтому разные файлы можно
 *    добавлять/править параллельно — они просто сливаются по верхним ключам.
 *
 * ── Как добавить новый язык ───────────────────────────────────────────────
 * 1. Скопируй `locales/en.json` → `locales/<code>.json` и переведи.
 * 2. Скопируй папку `locales/en/` → `locales/<code>/` и переведи файлы внутри.
 * 3. Добавь язык сюда: импортируй базовый json в `BASE` и добавь glob-папку
 *    в `AREAS`, плюс запись в `LANGUAGES`. English — исходный язык и fallback.
 */
export const LANGUAGES = [
  { code: 'en', label: 'English' },
  { code: 'ru', label: 'Русский' },
] as const

export type LanguageCode = (typeof LANGUAGES)[number]['code']

const STORAGE_KEY = 'lolda_lang'

function initialLanguage(): string {
  if (typeof window === 'undefined') return 'en'
  return window.localStorage.getItem(STORAGE_KEY) || 'en' // по умолчанию English
}

// По-областные словари: каждый файл = объект с одним top-level namespace-ключом.
const enAreas = import.meta.glob('./locales/en/*.json', { eager: true, import: 'default' })
const ruAreas = import.meta.glob('./locales/ru/*.json', { eager: true, import: 'default' })

/** База + все по-областные файлы (по верхним ключам, без вложенного мёржа). */
function buildResource(base: object, areas: Record<string, unknown>): object {
  return Object.assign({}, base, ...Object.values(areas))
}

void i18n.use(initReactI18next).init({
  resources: {
    en: { translation: buildResource(enBase, enAreas) },
    ru: { translation: buildResource(ruBase, ruAreas) },
  },
  lng: initialLanguage(),
  fallbackLng: 'en',
  interpolation: { escapeValue: false }, // React сам экранирует
  returnNull: false,
})

/** Сменить язык и запомнить выбор. */
export function setLanguage(code: string): void {
  void i18n.changeLanguage(code)
  try {
    window.localStorage.setItem(STORAGE_KEY, code)
  } catch {
    /* localStorage недоступен — не критично */
  }
}

export default i18n
