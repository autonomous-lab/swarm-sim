# Swarm-Sim

**Multi-agent social simulation engine with tiered LLM batching.**

Built in Rust. Async. Fast. Configurable. With a real-time web UI.

---

## The Problem

Existing multi-agent simulation frameworks (like [OASIS](https://github.com/camel-ai/oasis), used by [MiroFish](https://github.com/666ghj/MiroFish)) make **one LLM API call per agent per round**. With 100 agents running 72 rounds, that's 7,200 API calls — slow and expensive.

## The Solution

Swarm-Sim introduces **tiered batching**: agents are grouped by importance, and multiple agents are processed in a single LLM call.

| Tier | Role | Batch Size | Model | Calls (100 agents, 72 rounds) |
|------|------|-----------|-------|-------------------------------|
| **Tier 1 — VIP** | Opinion leaders, key figures | 1 (individual) | Best (GPT-4o, Claude) | 360 |
| **Tier 2 — Standard** | Active participants | 5-10 | Mid (GPT-4o-mini) | 288 |
| **Tier 3 — Figurants** | Background crowd | 20-50 | Cheap (Qwen, DeepSeek) | 216 |
| | | | **Total** | **864** (vs 7,200 = **88% reduction**) |

### Causality Model

Within each round, tiers execute **sequentially** to preserve causal chains:

```
Tier 1 executes → results feed into →
Tier 2 executes → results feed into →
Tier 3 executes
```

VIP agents set the narrative. Standard agents react to VIPs. Figurants react to everyone. Within each tier, batches fire **concurrently** (async tokio tasks with semaphore-based concurrency control).

---

## Features

- **Tiered batching engine** — the core innovation, fully configurable per tier (model, batch size, concurrency, temperature, retries)
- **Real-time web UI** — dark theme SPA with live feed, agent inspector, trending, timeline
- **God's Eye** — inject events mid-simulation via web UI or file watcher (breaking news, mood shifts, viral content)
- **Pause / Resume / Stop** — full simulation control from the browser
- **Document parsing** — feed PDF, Markdown, or plain text as seed scenarios
- **Entity extraction** — LLM automatically extracts entities from seed documents and generates agent profiles
- **Agent memory** — rolling observation window + pinned key memories that persist across rounds
- **Social simulation** — posts, replies, likes, reposts, follows with feed scoring (recency + popularity + relevance)
- **JSONL action log** — every action logged with agent, tier, reasoning, timestamps
- **Markdown report** — LLM-generated analysis of the simulation results
- **Multi-provider LLM** — any OpenAI-compatible API (OpenAI, Anthropic proxy, DashScope, DeepSeek, Ollama...)
- **7.9 MB binary** — single static binary, no runtime dependencies

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- An OpenAI-compatible LLM API key

### Build

```bash
git clone <repo-url>
cd swarm-sim
cargo build --release
```

The binary is at `target/release/swarm-sim`.

### Configure

```bash
cp config.example.toml config.toml
```

Edit `config.toml` — set your API keys (via environment variables or inline):

```toml
[tiers.tier1]
batch_size = 1
model = "gpt-4o"
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"

[tiers.tier2]
batch_size = 8
model = "gpt-4o-mini"
base_url = "https://api.openai.com/v1"
api_key = "${OPENAI_API_KEY}"

[tiers.tier3]
batch_size = 25
model = "qwen-plus"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
api_key = "${DASHSCOPE_API_KEY}"
```

Create a seed document (e.g. `data/news.md`) with your scenario content, and reference it in the config:

```toml
[simulation]
total_rounds = 72
seed_documents = ["./data/news.md"]
scenario_prompt = "Simulate public reaction to a major tech company announcing 10,000 layoffs."
```

### Run

```bash
# Run simulation + start web UI
swarm-sim run -c config.toml

# Open http://localhost:3000 in your browser
```

### Other Commands

```bash
# Validate config
swarm-sim validate -c config.toml

# Extract entities only (dry run — no simulation)
swarm-sim extract -c config.toml -o entities.json

# Generate report from existing data
swarm-sim report -c config.toml -o report.md
```

---

## Web UI

The web interface runs at `http://localhost:3000` (configurable) and provides:

```
┌──────────────────────────────────────────────────────────┐
│  HEADER: swarm-sim | Round 14/72 | 42 active | ⏸ Pause  │
├────────────────┬─────────────────────┬───────────────────┤
│  AGENTS PANEL  │   FEED / TIMELINE   │   GOD'S EYE       │
│                │                     │                   │
│ Filter by tier │  Live action feed   │  Inject events:   │
│ Click agent →  │  Trending posts     │  - Breaking news  │
│   profile,     │  Round timeline     │  - Viral content  │
│   memory,      │  Color-coded by     │  - Mood shifts    │
│   action       │  tier + action type │  - Announcements  │
│   history      │                     │                   │
│                │                     │  Live stats       │
├────────────────┴─────────────────────┴───────────────────┤
│  STATS: Posts | Likes | Replies | Reposts | Actions      │
└──────────────────────────────────────────────────────────┘
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `P` | Pause simulation |
| `R` | Resume simulation |
| `Esc` | Close agent modal |

---

## God's Eye — Live Event Injection

Inject events while the simulation runs, either through the **web UI** or by editing `events.toml`:

```toml
# events.toml — add events here while the simulation runs

[[events]]
id = "breaking-001"
inject_at_round = 10           # Optional: delay until specific round
event_type = "breaking_news"
content = "BREAKING: The CEO has just resigned effective immediately."

[[events]]
id = "mood-shift-001"
event_type = "agent_mood"
content = "agent:analyst_jane sentiment_bias:-0.8"

[[events]]
id = "viral-001"
event_type = "viral_content"
content = "A leaked memo reveals the real layoff numbers are 15,000."

[[events]]
id = "announcement-001"
event_type = "system_announcement"
content = "The platform is now trending #TechLayoffs worldwide."
```

Event types:
- `breaking_news` — appears as a system post visible to all agents
- `viral_content` — same as breaking_news, boosted visibility
- `agent_mood` — change an agent's sentiment bias mid-simulation
- `system_announcement` — platform-level announcement

Events are tracked by ID — each event is injected exactly once.

---

## Configuration Reference

All parameters are configurable via `config.toml`. See [`config.example.toml`](config.example.toml) for the full schema.

### Simulation

| Parameter | Default | Description |
|-----------|---------|-------------|
| `total_rounds` | 72 | Number of simulation rounds |
| `minutes_per_round` | 60 | Simulated minutes per round |
| `seed_documents` | — | Paths to seed documents (PDF, MD, TXT) |
| `scenario_prompt` | — | Natural language description of the scenario |
| `random_seed` | 0 | RNG seed (0 = system entropy) |

### Tiers (tier1 / tier2 / tier3)

| Parameter | Default | Description |
|-----------|---------|-------------|
| `batch_size` | — | Agents per LLM call (1 = individual) |
| `model` | — | LLM model identifier |
| `base_url` | — | OpenAI-compatible API base URL |
| `api_key` | — | API key (supports `${ENV_VAR}` syntax) |
| `temperature` | 0.7 | Generation temperature |
| `max_tokens` | 4096 | Max tokens per response |
| `max_concurrency` | 10 | Max concurrent API calls within this tier |
| `max_retries` | 2 | Retry count on failure |
| `timeout_secs` | 90 | Request timeout in seconds |

### World

| Parameter | Default | Description |
|-----------|---------|-------------|
| `feed_size` | 15 | Posts per agent feed |
| `trending_count` | 10 | Number of trending posts |
| `recency_weight` | 0.4 | Feed scoring weight for recent posts |
| `popularity_weight` | 0.3 | Feed scoring weight for engagement |
| `relevance_weight` | 0.3 | Feed scoring weight for followed authors |

### Server

| Parameter | Default | Description |
|-----------|---------|-------------|
| `host` | 0.0.0.0 | Web server bind address |
| `port` | 3000 | Web server port |
| `enabled` | true | Enable/disable web UI |

---

## Architecture

```
swarm-sim/
├── Cargo.toml
├── config.example.toml          # Full config reference
├── events.example.toml          # God's Eye events example
├── src/
│   ├── main.rs                  # CLI + orchestration
│   ├── config.rs                # TOML config with ${ENV_VAR} resolution
│   ├── agent.rs                 # Profiles, memory (rolling + pinned), tiers
│   ├── world.rs                 # Posts, social graph, feed scoring, trending
│   ├── engine.rs                # Simulation loop + tiered batching (the core)
│   ├── llm.rs                   # Async multi-provider LLM client + prompt templates
│   ├── parser.rs                # PDF/MD/TXT parsing + LLM entity extraction
│   ├── god_eye.rs               # File watcher for live event injection
│   ├── report.rs                # Post-simulation markdown report via LLM
│   ├── output.rs                # JSONL logger + terminal progress bars
│   └── server.rs                # Axum REST API + WebSocket + static files
└── web/
    ├── index.html               # SPA shell
    ├── style.css                # Dark theme
    ├── app.js                   # WebSocket client + state management
    └── components.js            # UI rendering components
```

### Key Design Decisions

1. **Async everything** — tokio runtime, reqwest for HTTP, axum for the web server. No blocking calls in the simulation loop.

2. **Semaphore-based concurrency** — each tier has its own concurrency limit to avoid overwhelming the LLM API.

3. **4-layer JSON parsing** — LLM responses are parsed with fallbacks: strict JSON → extract from markdown blocks → fix truncated JSON → default to do_nothing.

4. **Shared state via `Arc<RwLock>`** — the engine writes, the web server reads. No mutex contention on reads.

5. **Agent memory** — two layers: a rolling window of recent observations (default 20) and pinned key memories (default 5) that the LLM can mark as important.

6. **Feed scoring** — personalized per agent: `score = recency * W1 + engagement * W2 + is_followed * W3`. Weights are configurable.

7. **God's Eye dual input** — events can come from the file watcher OR from the web UI's REST API. Both feed into the same `mpsc` channel.

---

## API Reference

### REST Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/status` | Simulation status (round, agents, state) |
| `GET` | `/api/agents` | All agents (sorted by tier + followers) |
| `GET` | `/api/agents/:id` | Agent detail (profile + memory + posts) |
| `GET` | `/api/posts` | All posts (paginated, filterable by tier) |
| `GET` | `/api/posts/:id` | Single post with reply tree |
| `GET` | `/api/trending` | Top posts by engagement |
| `GET` | `/api/timeline` | Per-round summaries |
| `GET` | `/api/graph` | Social graph (nodes + edges) |
| `POST` | `/api/simulation/pause` | Pause simulation |
| `POST` | `/api/simulation/resume` | Resume simulation |
| `POST` | `/api/simulation/stop` | Stop + generate report |
| `POST` | `/api/god-eye/inject` | Inject event |

### WebSocket

Connect to `ws://localhost:3000/ws` for real-time events:

```json
{"type": "action",       "data": {...}}
{"type": "round_start",  "round": 5, "active_agents": 42}
{"type": "round_end",    "round": 5, "summary": {...}}
{"type": "god_eye_inject", "event": {...}}
{"type": "simulation_end", "total_rounds": 72, "total_actions": 5840}
```

---

## Output

### JSONL Action Log

Every agent action is logged to `output/actions.jsonl`:

```json
{"id":"a1b2c3","round":1,"agent_name":"analyst_jane","agent_tier":"tier1","action_type":"create_post","content":"Breaking: massive layoffs announced...","reasoning":"As an analyst, I need to comment first."}
```

### Markdown Report

After the simulation, a report is generated at `output/report.md` with:

1. Executive Summary
2. Timeline of Key Events
3. Agent Analysis (VIP behavior, most active, sentiment)
4. Viral Content Analysis
5. Network Dynamics
6. Methodology Notes

---

## Cost Estimation

Approximate costs for a simulation with 100 agents, 72 rounds:

| Approach | Calls | Estimated Cost (GPT-4o-mini) |
|----------|-------|------------------------------|
| Traditional (1 call/agent/round) | 7,200 | ~$15-30 |
| **Swarm-Sim tiered batching** | 864 | ~$2-5 |

Using cheap models (Qwen, DeepSeek) for Tier 3 further reduces costs. A typical simulation costs **$1-3**.

---

## Background

This project was born from analyzing [MiroFish](https://github.com/666ghj/MiroFish), a Python wrapper around the OASIS framework that makes 1 LLM call per agent per round. The core question was: *what if we batch multiple agents into a single LLM call?*

The answer: it works. With intelligent tiering (VIPs get individual calls, figurants get batched), you get 88% fewer API calls while maintaining behavioral diversity where it matters.

Built from scratch in Rust — no OASIS dependency, no Python subprocess, no external simulation framework. Just async Rust, LLM APIs, and a web browser.

---

## License

MIT
