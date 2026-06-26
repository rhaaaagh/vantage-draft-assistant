# Vantage Draft Assistant

A desktop companion for **League of Legends**: live draft from the game client,
pick & build recommendations, opponent scouting, patch-aware tier lists and
champion pages, and post-game analysis. Runs **locally on your own Riot API key** —
nothing is sent to third-party servers.

[![CI](https://github.com/rhaaaagh/vantage-draft-assistant/actions/workflows/ci.yml/badge.svg)](https://github.com/rhaaaagh/vantage-draft-assistant/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> **Stack:** Tauri 2 (Rust) · React 19 · TypeScript · SQLite.
> **Data:** official Riot Games API, Data Dragon / CommunityDragon, and the local
> LoL client (LCU). All match statistics are crawled into a local SQLite database.

---

## Screenshots

<!--
  Add real images to docs/screenshots/ and uncomment the tags below.
  Recommended: 1280×800 PNG, one per major view.
-->

| Draft & recommendations | Scout (player patterns) | Champion page |
| --- | --- | --- |
| _add `docs/screenshots/draft.png`_ | _add `docs/screenshots/scout.png`_ | _add `docs/screenshots/champion.png`_ |

<!--
![Draft view](docs/screenshots/draft.png)
![Scout view](docs/screenshots/scout.png)
![Champion page](docs/screenshots/champion.png)
-->

---

## Features

- **Draft / Match** — live draft from the game client (LCU): picks/bans, auto role
  detection, pick and build recommendations from a custom engine. After the game
  starts: team compositions and ranks (Spectator API). Manual draft analyzer as a
  fallback mode.
- **Scout** — live-game reconnaissance: compositions, ranks, auto-roles, match
  history, mastery, personal & meta matchups, and **player patterns** (death map
  with timings, movement routes, and jungle pathing per match).
- **Profile** — rank, win rate, most-played champions, history, mastery.
- **Tier list** — champions by role and patch (win rate); click a name → champion page.
- **Champion** — win/pick/ban rate, matchups, synergies, runes, item build path.
- **Crawler** — collects match statistics (Challenger → Diamond) into the local
  database; tier lists, champion pages and the recommendation engine are built on it.
- **Match breakdown** — post-game: score, runes, gold/XP/CS charts, death map,
  objectives, per-player analysis.
- **i18n** — English (default) and Russian, with lazy-loaded per-area locale files.

---

## Technical highlights

The parts of the codebase worth a closer look:

### Concurrent rate limiter with priorities — [`src-tauri/src/rate_limit.rs`](src-tauri/src/rate_limit.rs)
Riot's dev key allows **20 req/s and 100 req/2 min**. The limiter models both as
**sliding windows** (timestamps in a `VecDeque`) so the published quota is matched
exactly and never exceeded — `wait_needed` blocks on the **max** of the two windows.
Interactive requests (profile, scout, draft) run at `High` priority; the background
crawler runs at `Low`, **yields** while any `High` request is waiting, and never
touches a reserved pool of slots — so user-facing lookups stay responsive while the
crawler works. The sleep is performed **without holding the mutex** (a held lock
would block every other thread — a real bug that's documented in the code). Covered
by 9 unit tests, including window boundaries and the reserve logic.

### Statistical recommendation engine — [`src-tauri/src/recommend.rs`](src-tauri/src/recommend.rs)
Pick scores combine several signals — base champion win rate, lane matchup, and ally
synergies — in **log-odds (logit) space**, where probabilities add symmetrically; the
result is mapped back to 0..1 with a sigmoid. Small samples are **Bayesian-shrunk**
toward a prior (`shrink_wr`) so a 100%-from-one-game champion can't dominate the
ranking. The pure math is isolated from the database and covered by unit tests.

### Production-grade Riot API handling — [`src-tauri/src/riot_api.rs`](src-tauri/src/riot_api.rs)
Typed errors (`RiotError`) mapped to human-readable messages, `429` retries that
honor the `Retry-After` header, and correct regional routing (separate clusters for
account-v1 and match-v5).

### Player-pattern engine — [`src-tauri/src/archetypes.rs`](src-tauri/src/archetypes.rs)
Builds behavioral profiles from match timelines (`CHAMPION_KILL` events and
`participantFrames`): death maps with timings, movement routes, and early jungle
pathing — optionally filtered to the opponent's **current champion and role**.
Results are cached per player to avoid re-downloading timelines.

### Data pipeline & schema — [`src-tauri/src/crawler.rs`](src-tauri/src/crawler.rs) · [`src-tauri/src/db.rs`](src-tauri/src/db.rs)
A resumable crawler aggregates matches into a normalized SQLite schema
(`matchup_agg`, `synergy_agg`, `champion_role_agg`, `rune_agg`, …) with indexes
tuned to the read queries. WAL mode is enabled for concurrent crawler writes.

---

## Architecture

```
┌─────────────────────────────┐         ┌──────────────────────────────┐
│  Frontend (React + TS)      │  Tauri  │  Backend (Rust)              │
│  views/ · api/ · components/ │  IPC →  │  ~30 #[tauri::command]       │
│  i18n, drag-drop draft board │ ← invoke│  rate_limit · recommend ·    │
└─────────────────────────────┘         │  crawler · archetypes · db   │
                                         └───────────────┬──────────────┘
                       Riot API / Data Dragon / LCU      │      SQLite (AppData)
                       ◄─────────────────────────────────┘
```

- **Frontend** (`src/`) talks to the backend through ~30 Tauri IPC commands.
- **Backend** (`src-tauri/src/`) owns all networking, the crawler, the engines, and
  the database. The local SQLite DB lives in the OS app-data directory, not the repo.

---

## Getting started

**Prerequisites:** [Node.js](https://nodejs.org) 18+, [Rust](https://rustup.rs), and
the Tauri [system dependencies](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev      # development mode
npm run tauri build    # build installer / .exe
```

### Riot API key (required)

The app runs on **your** key — nothing is sent to third-party servers.

📖 **Step-by-step with screenshots: [docs/RIOT_API_KEY.md](docs/RIOT_API_KEY.md)**

Quick version:

1. Get a key at [developer.riotgames.com](https://developer.riotgames.com) (sign in
   with your Riot account).
2. In the app: **Settings** → **Riot API Key** → paste → **Save settings**.
3. Set your **region** (RU, EUW, NA…) and, if needed, the path to your League folder.
4. On a **403**, enable the LoL products on the portal (Summoner, Match, Spectator,
   League…) and generate a new key. A development key lasts 24 hours; register a
   **Personal** key for everyday use.

---

## Testing

```bash
cd src-tauri && cargo test      # backend unit tests (43)
npm run lint                    # eslint
npx tsc -b                      # type check
```

CI runs all of the above plus `cargo clippy -D warnings` on every push and PR
(see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

---

## Data storage & privacy

Your API key and settings are stored locally (inside the app); the crawler database
lives in the OS app-data directory, not in the repository. No telemetry.

---

## Disclaimer

Vantage Draft Assistant isn't endorsed by Riot Games and doesn't reflect the views or
opinions of Riot Games or anyone officially involved in producing or managing Riot
Games properties. Riot Games and all associated properties are trademarks or registered
trademarks of Riot Games, Inc.

## License

[MIT](LICENSE).
