# Getting a Riot Games API key

Vantage Draft Assistant runs on **your own** Riot API key — nothing is sent to
third-party servers. This guide walks you through getting one, step by step.

There are two kinds of key:

| Key | How you get it | Lifetime | Rate limits | Best for |
| --- | --- | --- | --- | --- |
| **Development** | Instantly on the dashboard | **Expires every 24 h** | 20 req/s · 100 req/2 min | Quick try-out |
| **Personal** | Short application form | Long-lived (until unused) | Higher | Regular use |

> Screenshots for each step live in [`docs/screenshots/api/`](screenshots/api/).
> If a label looks slightly different, Riot occasionally tweaks the portal — the
> flow stays the same.

---

## Step 1 — Open the developer portal

Go to **[developer.riotgames.com](https://developer.riotgames.com)**.

![Developer portal home](screenshots/api/01-portal-home.png)

---

## Step 2 — Sign in with your Riot account

Click **Login** (top-right) and sign in with the **same Riot Games account you use
to play** League of Legends. The first time you may need to accept the developer
terms of use.

![Sign in](screenshots/api/02-login.png)

---

## Step 3 — Copy your Development API key (fastest start)

On the dashboard you'll see **DEVELOPMENT API KEY** — a string like
`RGAPI-xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.

- Click **Regenerate API Key** if it's empty or expired.
- Copy the key.

> ⚠️ A development key **expires after 24 hours** — you'll need to regenerate and
> re-paste it the next day. For everyday use, get a Personal key (Step 5).

![Development API key on the dashboard](screenshots/api/03-dev-key.png)

---

## Step 4 — Paste the key into the app

In Vantage Draft Assistant:

1. Open **Settings**.
2. Paste the key into **Riot API Key**.
3. Choose your **Region** (RU, EUW, NA…).
4. Click **Save settings**.

![App settings — paste key and region](screenshots/api/04-app-settings.png)

> **Getting a 403?** On the portal, make sure the LoL products are enabled
> (Summoner, Match, Spectator, League…) and generate a fresh key.

---

## Step 5 — (Recommended) Register a Personal API key

A Personal key doesn't expire every day and has higher rate limits.

1. On the dashboard, find **Register Product** and click **Register Product**.
2. Choose **Personal API Key**.
3. Fill in the form. Suggested values for a personal, non-commercial tool:

   | Field | What to enter |
   | --- | --- |
   | **Product Name** | `Vantage Draft Assistant (personal)` |
   | **Product Description** | `Personal, non-commercial desktop app that reads my own League of Legends match data to show draft recommendations and post-game stats. Not distributed commercially.` |
   | **Product URL** *(if asked)* | your GitHub repo link, or leave blank |
   | **Application/Group** *(if asked)* | leave default |

4. Accept the terms and **Submit**.
5. Approval is usually quick. Once approved, copy the **Personal API Key** from the
   dashboard and paste it into the app (Step 4).

![Register product — Personal API Key form](screenshots/api/05-register-personal.png)

---

## Notes

- **Never share your key or commit it to a repository.** Treat it like a password.
- The key and your settings are stored **locally** by the app, not in this repo.
- Development and Personal keys are for **non-commercial** use. A Production key
  (for published products) requires a separate review by Riot.
