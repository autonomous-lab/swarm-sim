# I reverse-engineered a $4M AI project. Then I rebuilt it in Rust. In one day.

Last week, a project called MiroFish hit #1 on GitHub trending. 22,000 stars. Backed by a Chinese billionaire. Built by a 20-year-old intern who became CEO overnight.

The pitch: an AI engine that creates thousands of digital humans with personalities, memories, and behaviors — then drops them into a virtual world to predict the future.

I cloned the repo. Read every line of code. Here's what I found.

---

## The $4M wrapper

MiroFish doesn't simulate anything. The actual simulation engine is OASIS — an open-source framework by CAMEL-AI. MiroFish is a configuration layer on top of it. A Flask backend. 414 lines of frontend code. One test file.

The "predict anything" claim? There are zero benchmarks. Zero backtests. Zero validation against real-world outcomes. The system generates plausible conversations between LLM agents on a simulated Twitter. That's not prediction. That's theater.

But the real problem isn't the marketing. It's the architecture.

## One call per agent per round

OASIS — and by extension MiroFish — makes **one LLM API call per agent per round**. Every single agent, every single round, gets its own call.

100 agents. 72 rounds. That's 7,200 API calls. On GPT-4o, you're looking at $30+. On a cheaper model, still $5-10. And most of those calls are for background characters who post things like "interesting, thanks for sharing."

7,200 calls where 864 would do.

## The insight

Not all agents are equal. In any social simulation, you have:

- **5 opinion leaders** who set the narrative
- **25 active participants** who react and engage
- **70 background characters** who like, scroll, and occasionally comment

Why are you giving the background characters the same treatment as the opinion leaders?

## Tiered batching

Here's what I built instead:

**Tier 1 — VIP agents.** Individual LLM calls. Best model. Full persona, full memory, personalized feed. These are your protagonists. They get the premium treatment because their actions shape everything downstream.

**Tier 2 — Standard agents.** Batched in groups of 8. One LLM call generates actions for all 8 simultaneously. Mid-tier model. They see what the VIPs did and react.

**Tier 3 — Figurants.** Batched in groups of 25. One call, 25 agents. Cheapest model available. They see what everyone above them did. They fill the world with noise, engagement, and occasional surprises.

The key: **tiers execute sequentially within each round**. Tier 1 goes first. Tier 2 sees Tier 1's output. Tier 3 sees everything. Causality is preserved. Emergence happens across tiers, not within batches.

Result: **864 calls instead of 7,200. Same simulation. 88% cost reduction.**

## Why Rust

Python is fine for prototypes. MiroFish uses Python. It also uses subprocess.Popen to run the simulation, threading for "concurrency," and regex to strip `<think>` tags from model outputs.

I wanted:
- True async concurrency (tokio) — fire all batches within a tier simultaneously
- Connection pooling per tier — each tier talks to a different API endpoint
- Semaphore-based rate limiting — don't overwhelm the API
- A single binary — no virtualenv, no pip install, no "works on my machine"

The release binary is 7.9 MB. It includes the web server, the simulation engine, the document parser, and the web UI. No dependencies.

## What I actually built

**swarm-sim** — a complete multi-agent social simulation engine.

Feed it a document — a news article, a financial report, a policy draft. It extracts entities, generates agent profiles, assigns tiers, and runs the simulation. You watch it unfold in real-time through a web UI.

The God's Eye feature: mid-simulation, you can inject events. "The CEO just resigned." "A leaked memo surfaces." "Interest rates drop 50bps." The entire simulated world reorganizes. VIPs react first, the crowd follows.

Pause. Resume. Inspect any agent's memory. Read their internal reasoning. See who's following who. Watch narratives form and opinions shift.

All configurable via a single TOML file. Change the models, the batch sizes, the concurrency limits, the feed scoring weights. Plug in any OpenAI-compatible API.

```
┌──────────────────────────────────────────────────────────┐
│  swarm-sim | Round 14/72 | 42 active | ⏸ Pause          │
├────────────────┬─────────────────────┬───────────────────┤
│  AGENTS        │   LIVE FEED         │   GOD'S EYE       │
│                │                     │                   │
│ VIP            │  @ceo_official POST  │  [Breaking News]  │
│  ● @ceo        │  "Today we made the │  [Content area]   │
│  ● @analyst    │   difficult..."     │  [Inject]         │
│                │                     │                   │
│ Standard       │  @analyst REPLY     │  Event Log:       │
│  ● @dev_mike   │  "The numbers don't │  ✓ CEO resigned   │
│  ● @pm_sarah   │   add up..."       │  ✓ Memo leaked    │
│                │                     │                   │
│ Figurants      │  @user_42 LIKE      │  Stats:           │
│  ● @user_42    │  @user_17 REPOST    │  142 posts        │
│  ...68 more    │                     │  891 actions      │
└────────────────┴─────────────────────┴───────────────────┘
```

## The numbers

| | MiroFish | swarm-sim |
|---|---|---|
| Language | Python | Rust |
| Simulation engine | OASIS (external) | Built-in |
| API calls (100 agents, 72 rounds) | 7,200 | 864 |
| Cost per simulation | $5-30 | $1-3 |
| Binary size | Python + deps | 7.9 MB |
| Web UI | 414 lines | Full SPA |
| God's Eye | No | Yes |
| Pause/Resume | No | Yes |
| Agent memory | Zep Cloud (paid) | Built-in |
| Tests | 1 file | — |
| Build time | 10 days (claimed) | 1 day |

## What this isn't

Let me be honest about what swarm-sim does NOT do:

- It does not predict the future. No multi-agent simulation does.
- It does not validate its outputs against reality. Neither does anyone else in this space.
- LLM agents are not real humans. Their "opinions" are statistical artifacts.
- Batching trades per-agent quality for cost efficiency. Tier 3 figurants are less individually nuanced than individual calls would produce.

What it does: it lets you explore scenarios. "What if X happened? How might different stakeholders react? What narratives might emerge?" It's a thinking tool, not an oracle.

The difference between swarm-sim and MiroFish isn't that one predicts better. It's that one costs 88% less to run, ships as a single binary, and doesn't pretend to be something it's not.

## Open source

The code is at `github.com/autonomous-lab/swarm-sim`. TOML config, Rust source, web UI — everything you need.

If you're building multi-agent simulations and spending $30 per run on API calls, you're doing it wrong. Batch your agents. Tier your models. Preserve causality. Ship a binary.

The era of 1-call-per-agent is over.
