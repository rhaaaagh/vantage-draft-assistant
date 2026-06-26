# Architecture

A high-level map of how Vantage Draft Assistant is built, and the design
decisions behind the parts that matter.

## Overview

```
┌─────────────────────────────┐         ┌──────────────────────────────┐
│  Frontend (React 19 + TS)   │  Tauri  │  Backend (Rust)              │
│  views/ · api/ · components/ │  IPC →  │  ~30 #[tauri::command]       │
│  i18n, drag-drop draft board │ ← invoke│  engines · crawler · db      │
└─────────────────────────────┘         └───────────────┬──────────────┘
                                                         │
        Riot API · Data Dragon · LCU  ◄─────────────────┤
                                          SQLite (OS app-data dir)
```

- **Frontend** (`src/`) is a React + TypeScript SPA. It never talks to the network
  directly — it calls the Rust backend through ~30 Tauri IPC commands.
- **Backend** (`src-tauri/src/`) owns all networking, the crawler, the analysis
  engines, and the local SQLite database.
- **Data** comes from the official Riot API, Data Dragon / CommunityDragon, and the
  local League client (LCU). Aggregated match stats live in a local SQLite DB in the
  OS app-data directory — not in the repo, with no telemetry.

## Backend modules (`src-tauri/src/`)

| Module | Responsibility |
| --- | --- |
| `main.rs` | App setup + all `#[tauri::command]` IPC handlers |
| `rate_limit.rs` | Concurrent, priority-aware Riot API rate limiter |
| `riot_api.rs` | Riot API client: typed errors, retries, regional routing |
| `db.rs` | SQLite schema, aggregates, queries |
| `crawler.rs` | Resumable match crawler → aggregates |
| `recommend.rs` | Pick recommendation engine (log-odds + Bayesian) |
| `archetypes.rs` | Player-pattern engine (death maps, routes) from timelines |
| `ddragon.rs` | Champion/static data (Data Dragon) |
| `lcu.rs` | Local client (LCU) integration for live draft |
| `profile.rs`, `match_detail.rs` | Profile and post-game breakdown |
| `paths.rs` | App-data paths, legacy file migration |

## Key design decisions

These are the parts worth reading first — and the ones I can walk through in detail.

### 1. Concurrent rate limiter with priorities (`rate_limit.rs`)
Riot's dev key allows **20 req/s and 100 req/2 min**. Both are modeled as
**sliding windows** (request timestamps in a `VecDeque`), so the published quota is
matched exactly and never exceeded — `wait_needed` blocks on the **max** of the two
windows.

- **Priorities:** interactive requests (profile, scout, draft) run `High`; the
  background crawler runs `Low`. The crawler *yields* while any `High` request is
  waiting, and never consumes a reserved pool of slots — so user-facing lookups stay
  responsive while crawling.
- **No deadlock:** the wait `sleep` happens **without holding the mutex**. Holding it
  would block every other thread (a real bug that's documented at the call site).
- Covered by 9 unit tests, including window boundaries and reserve logic.

### 2. Statistical recommendation engine (`recommend.rs`)
Pick scores combine base win rate, lane matchup, and ally synergies in
**log-odds (logit) space**, where probability signals add symmetrically; the result
maps back to 0..1 via a sigmoid. Small samples are **Bayesian-shrunk** toward a prior
(`shrink_wr`) so a champion with one 100%-win game can't top the list. The pure math
is isolated from the database and unit-tested.

### 3. Production-grade Riot API handling (`riot_api.rs`)
Typed errors (`RiotError`) mapped to human-readable messages, `429` retries that
honor `Retry-After`, and correct regional routing (separate clusters for account-v1
vs match-v5).

### 4. Player-pattern engine (`archetypes.rs`)
Behavioral profiles built from match timelines (`CHAMPION_KILL` events and
`participantFrames`): death maps with timings, movement routes, early jungle pathing —
optionally filtered to the opponent's current champion and role. Cached per player to
avoid re-downloading timelines.

### 5. Data pipeline (`crawler.rs` + `db.rs`)
A resumable crawler aggregates matches into a normalized SQLite schema
(`matchup_agg`, `synergy_agg`, `champion_role_agg`, `rune_agg`, …) with indexes tuned
to the read queries. WAL mode is enabled for concurrent crawler writes.

## Frontend (`src/`)

- `views/` — top-level screens (Draft, Scout, Champion, Profile, MatchBreakdown, …).
- `api/` — thin wrappers over Tauri `invoke` calls (one file per area).
- `components/` — reusable UI (draft board, Minimap with route drawing, charts).
- `utils/` — pure helpers (role assignment, Riot ID parsing) — easy to unit-test.
- `i18n/` — react-i18next, English (default) + Russian, lazy-loaded per area.

## Testing & CI

- **43 backend unit tests** (rate limiter, engines, parsing, aggregates).
- **CI** (`.github/workflows/ci.yml`): eslint + `tsc` on the frontend; `cargo clippy
  -D warnings` + `cargo test` on the backend, every push and PR.
- **Releases** (`.github/workflows/release.yml`): pushing a `v*` tag builds the
  Windows installer and publishes it to GitHub Releases.

## How this was built

Architecture and design decisions are mine. Implementation was carried out with the
help of AI coding agents working on parallel tracks, with all output reviewed and
integrated by me — including the rate limiter, the recommendation engine, and the
Riot API layer described above.
