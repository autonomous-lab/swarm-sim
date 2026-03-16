// ================================================================
// Swarm-Sim — WebSocket Client & State Management
// ================================================================

const API = window.location.origin;
let ws = null;
let currentFilter = 'all';
let currentTab = 'feed';
let feedActions = [];
let roundSummaries = [];
let syntheses = [];
let stats = { posts: 0, likes: 0, replies: 0, reposts: 0, actions: 0 };
let tokenUsage = { prompt: 0, completion: 0 };
let currentStatus = 'idle';
let synthesisCollapsed = false;

// ----------------------------------------------------------------
// WebSocket
// ----------------------------------------------------------------

function connectWebSocket() {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  ws = new WebSocket(`${proto}//${location.host}/ws`);

  ws.onopen = () => console.log('WebSocket connected');

  ws.onmessage = (evt) => {
    try {
      const msg = JSON.parse(evt.data);
      handleWsMessage(msg);
    } catch (e) {
      console.error('WS parse error:', e);
    }
  };

  ws.onclose = () => {
    console.log('WebSocket closed, reconnecting in 3s...');
    setTimeout(connectWebSocket, 3000);
  };

  ws.onerror = (err) => console.error('WS error:', err);
}

function handleWsMessage(msg) {
  switch (msg.type) {
    case 'action':
      handleAction(msg.data);
      break;
    case 'round_start':
      handleRoundStart(msg);
      break;
    case 'round_end':
      handleRoundEnd(msg);
      break;
    case 'god_eye_inject':
      handleGodEyeInject(msg.event);
      break;
    case 'synthesis':
      handleSynthesis(msg);
      break;
    case 'simulation_end':
      handleSimEnd(msg);
      break;
    case 'status_update':
      updateStatus(msg);
      break;
    default:
      // Initial status snapshot
      if (msg.status !== undefined) {
        updateStatus(msg);
      }
  }
}

// ----------------------------------------------------------------
// Event handlers
// ----------------------------------------------------------------

function handleAction(action) {
  if (action.action_type === 'do_nothing' || action.action_type === 'pin_memory') return;

  feedActions.unshift(action);
  if (feedActions.length > 500) feedActions.length = 500;

  // Update stats
  stats.actions++;
  if (action.action_type === 'create_post') stats.posts++;
  if (action.action_type === 'like') stats.likes++;
  if (action.action_type === 'reply') stats.replies++;
  if (action.action_type === 'repost') stats.reposts++;
  updateStats();

  // Prepend to feed
  if (currentTab === 'feed') {
    const el = document.getElementById('feed-list');
    el.insertAdjacentHTML('afterbegin', renderPostCard(action));
    while (el.children.length > 200) el.removeChild(el.lastChild);
  }
}

function handleRoundStart(msg) {
  document.getElementById('round-info').textContent = `Round ${msg.round}`;
  document.getElementById('agent-count').textContent = `${msg.active_agents} active`;

  if (currentTab === 'feed') {
    const el = document.getElementById('feed-list');
    el.insertAdjacentHTML('afterbegin', renderRoundSeparator(msg.round));
  }
}

function handleRoundEnd(msg) {
  roundSummaries.push(msg.summary);
  document.getElementById('round-info').textContent =
    `Round ${msg.round}/${document.getElementById('round-info').dataset.total || '?'}`;

  // Update token counts
  if (msg.prompt_tokens !== undefined) {
    tokenUsage.prompt = msg.prompt_tokens;
    tokenUsage.completion = msg.completion_tokens || 0;
    updateTokenDisplay();
  }

  if (currentTab === 'timeline') refreshTimeline();
  if (currentTab === 'dashboard') refreshDashboard();
  refreshTrending();
}

function handleGodEyeInject(event) {
  const log = document.getElementById('event-log');
  log.insertAdjacentHTML('afterbegin', renderEventEntry(event));
}

function handleSynthesis(msg) {
  syntheses.push({ round: msg.round, text: msg.text });
  showSynthesis(msg.round, msg.text);
}

function handleSimEnd(msg) {
  currentStatus = 'finished';
  updateStatusBadge('finished');
  document.getElementById('btn-pause').disabled = true;
  document.getElementById('btn-stop').disabled = true;
  updateNewSimButton();
  updateContinueVisibility();
  if (currentTab === 'dashboard') refreshDashboard();
}

// ----------------------------------------------------------------
// Synthesis panel
// ----------------------------------------------------------------

function showSynthesis(round, text) {
  const panel = document.getElementById('synthesis-panel');
  panel.classList.remove('hidden');
  document.getElementById('synthesis-round').textContent = `Round ${round}`;
  document.getElementById('synthesis-body').textContent = text;
}

function toggleSynthesis() {
  synthesisCollapsed = !synthesisCollapsed;
  const body = document.getElementById('synthesis-body');
  const toggle = document.getElementById('synthesis-toggle');
  body.style.display = synthesisCollapsed ? 'none' : 'block';
  toggle.innerHTML = synthesisCollapsed ? '&#9654;' : '&#9660;';
}

// ----------------------------------------------------------------
// Status
// ----------------------------------------------------------------

function updateStatus(snap) {
  currentStatus = snap.status;
  updateStatusBadge(snap.status);
  document.getElementById('round-info').textContent =
    `Round ${snap.current_round}/${snap.total_rounds}`;
  document.getElementById('round-info').dataset.total = snap.total_rounds;
  document.getElementById('agent-count').textContent = `${snap.total_agents} agents`;

  const isRunning = snap.status === 'running';
  const isPaused = snap.status === 'paused';
  document.getElementById('btn-pause').disabled = !isRunning;
  document.getElementById('btn-resume').disabled = !isPaused;
  document.getElementById('btn-stop').disabled = !isRunning && !isPaused;

  // Update token counts
  if (snap.prompt_tokens !== undefined) {
    tokenUsage.prompt = snap.prompt_tokens;
    tokenUsage.completion = snap.completion_tokens || 0;
    updateTokenDisplay();
  }

  // Show scenario banner
  if (snap.scenario_prompt) {
    const banner = document.getElementById('scenario-banner');
    document.getElementById('scenario-text').textContent = snap.scenario_prompt;
    banner.classList.remove('hidden');
  }

  updateNewSimButton();
  updateContinueVisibility();
}

function updateStatusBadge(status) {
  const badge = document.getElementById('status-badge');
  badge.textContent = status.toUpperCase();
  badge.className = 'badge ' + status;
}

function updateStats() {
  document.getElementById('stat-posts').textContent = stats.posts;
  document.getElementById('stat-likes').textContent = stats.likes;
  document.getElementById('stat-replies').textContent = stats.replies;
  document.getElementById('stat-reposts').textContent = stats.reposts;
  document.getElementById('stat-actions').textContent = stats.actions;
}

function formatTokenCount(n) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toString();
}

function updateTokenDisplay() {
  const total = tokenUsage.prompt + tokenUsage.completion;
  const el = document.getElementById('token-count');
  el.textContent = `${formatTokenCount(total)} tokens`;
  el.title = `Prompt: ${tokenUsage.prompt.toLocaleString()} | Completion: ${tokenUsage.completion.toLocaleString()} | Total: ${total.toLocaleString()}`;
}

// ----------------------------------------------------------------
// Launch modal
// ----------------------------------------------------------------

function openLaunchModal() {
  document.getElementById('launch-modal').classList.add('active');
  updateCostEstimate();
}

function closeLaunchModal() {
  document.getElementById('launch-modal').classList.remove('active');
}

function updateCostEstimate() {
  const rounds = parseInt(document.getElementById('launch-rounds').value) || 10;
  const agents = parseInt(document.getElementById('launch-agents').value) || 40;

  // Tier distribution: ~10% T1, ~20% T2, ~70% T3
  const t1 = Math.max(3, Math.round(agents * 0.10));
  const t2 = Math.max(3, Math.round(agents * 0.20));
  const t3 = Math.max(1, agents - t1 - t2);

  // API calls per round: T1=individual, T2=batch of 8, T3=batch of 25
  const callsPerRound = t1 + Math.ceil(t2 / 8) + Math.ceil(t3 / 25);
  const extractionCalls = 2; // stakeholder + figurant generation
  const totalCalls = (callsPerRound * rounds) + extractionCalls;

  // Token estimation (avg per call)
  // T1: ~1500 in, ~400 out | T2: ~3000 in, ~1500 out | T3: ~5000 in, ~3000 out
  const tokensIn = rounds * (t1 * 1500 + Math.ceil(t2/8) * 3000 + Math.ceil(t3/25) * 5000) + 12000;
  const tokensOut = rounds * (t1 * 400 + Math.ceil(t2/8) * 1500 + Math.ceil(t3/25) * 3000) + 8000;
  const totalTokens = tokensIn + tokensOut;

  // Cost: Gemini 2.0 Flash via OpenRouter: $0.10/1M in, $0.40/1M out
  const costIn = (tokensIn / 1_000_000) * 0.10;
  const costOut = (tokensOut / 1_000_000) * 0.40;
  const totalCost = costIn + costOut;

  document.getElementById('est-calls').textContent = `~${totalCalls}`;
  document.getElementById('est-tokens').textContent = totalTokens > 1_000_000
    ? `~${(totalTokens / 1_000_000).toFixed(1)}M`
    : `~${Math.round(totalTokens / 1000)}K`;
  document.getElementById('est-cost').textContent = totalCost < 0.01
    ? `<$0.01`
    : `~$${totalCost.toFixed(2)}`;
}

function updateNewSimButton() {
  const btn = document.getElementById('btn-new-sim');
  const isActive = currentStatus === 'running' || currentStatus === 'preparing';
  btn.disabled = isActive;
}

// ----------------------------------------------------------------
// API calls
// ----------------------------------------------------------------

async function fetchJson(path) {
  const res = await fetch(`${API}${path}`);
  return res.json();
}

async function postJson(path, body) {
  const res = await fetch(`${API}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  return res.json();
}

// ----------------------------------------------------------------
// Agent panel
// ----------------------------------------------------------------

async function loadAgents() {
  try {
    const agents = await fetchJson('/api/agents');
    renderAgentsList(agents);
  } catch (e) {
    // Silently ignore during idle state
  }
}

function renderAgentsList(agents) {
  const list = document.getElementById('agents-list');
  const filtered = currentFilter === 'all'
    ? agents
    : agents.filter(a => a.tier === currentFilter);
  list.innerHTML = filtered.map(renderAgentCard).join('');
}

function filterAgents(tier) {
  currentFilter = tier;
  document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
  event.target.classList.add('active');
  loadAgents();
}

async function showAgent(id) {
  const data = await fetchJson(`/api/agents/${id}`);
  document.getElementById('agent-detail').innerHTML = renderAgentDetail(data);
  document.getElementById('agent-modal').classList.add('active');
}

function closeModal() {
  document.getElementById('agent-modal').classList.remove('active');
}

// ----------------------------------------------------------------
// Tabs
// ----------------------------------------------------------------

function switchTab(tab) {
  currentTab = tab;
  document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
  document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
  event.target.classList.add('active');

  const contentMap = { feed: 'feed-list', trending: 'trending-list', timeline: 'timeline-list', dashboard: 'dashboard-content', threads: 'threads-list' };
  document.getElementById(contentMap[tab]).classList.add('active');

  if (tab === 'trending') refreshTrending();
  if (tab === 'timeline') refreshTimeline();
  if (tab === 'dashboard') refreshDashboard();
  if (tab === 'threads') refreshThreads();
}

async function refreshTrending() {
  try {
    const trending = await fetchJson('/api/trending');
    const el = document.getElementById('trending-list');
    if (trending.length === 0) {
      el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">No trending posts yet</div>';
    } else {
      el.innerHTML = trending.map((p, i) => renderTrendingPost(p, i + 1)).join('');
    }
  } catch (e) {}
}

function refreshTimeline() {
  const el = document.getElementById('timeline-list');
  if (roundSummaries.length === 0) {
    el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">Simulation not started</div>';
  } else {
    el.innerHTML = roundSummaries.map(renderTimelineEntry).join('');
    el.scrollTop = el.scrollHeight;
  }
}

async function refreshDashboard() {
  try {
    const data = await fetchJson('/api/dashboard');
    document.getElementById('dashboard-content').innerHTML = renderDashboard(data);
  } catch (e) {
    document.getElementById('dashboard-content').innerHTML =
      '<div style="color:var(--text-muted);padding:20px;text-align:center">Dashboard unavailable</div>';
  }
}

async function refreshThreads() {
  const el = document.getElementById('threads-list');
  try {
    const data = await fetchJson('/api/posts?limit=1000');
    const posts = data.posts || [];

    if (posts.length === 0) {
      el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">No posts yet</div>';
      return;
    }

    // Index all posts by ID
    const postMap = {};
    posts.forEach(p => { postMap[p.id] = p; });

    // Find root posts (no reply_to) that have at least 1 reply
    const threads = posts
      .filter(p => !p.reply_to && p.replies && p.replies.length > 0)
      .map(root => {
        const replies = root.replies
          .map(rid => postMap[rid])
          .filter(Boolean)
          .sort((a, b) => a.created_at_round - b.created_at_round);
        const lastActivity = replies.length > 0
          ? Math.max(root.created_at_round, ...replies.map(r => r.created_at_round))
          : root.created_at_round;
        const engagement = (root.likes ? root.likes.length : 0) + (root.replies ? root.replies.length : 0) * 2;
        return { root, replies, lastActivity, engagement };
      })
      .sort((a, b) => b.engagement - a.engagement || b.lastActivity - a.lastActivity);

    if (threads.length === 0) {
      el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">No threads with replies yet</div>';
      return;
    }

    el.innerHTML = threads.map(t => renderThread(t.root, t.replies)).join('');
  } catch (e) {
    el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">Could not load threads</div>';
  }
}

// ----------------------------------------------------------------
// Simulation controls
// ----------------------------------------------------------------

async function pauseSim() {
  await postJson('/api/simulation/pause');
  currentStatus = 'paused';
  updateStatusBadge('paused');
  document.getElementById('btn-pause').disabled = true;
  document.getElementById('btn-resume').disabled = false;
}

async function resumeSim() {
  await postJson('/api/simulation/resume');
  currentStatus = 'running';
  updateStatusBadge('running');
  document.getElementById('btn-pause').disabled = false;
  document.getElementById('btn-resume').disabled = true;
}

async function stopSim() {
  await postJson('/api/simulation/stop');
  currentStatus = 'finished';
  updateStatusBadge('finished');
  document.getElementById('btn-pause').disabled = true;
  document.getElementById('btn-resume').disabled = true;
  document.getElementById('btn-stop').disabled = true;
  updateNewSimButton();
}

// ----------------------------------------------------------------
// Launch simulation from UI
// ----------------------------------------------------------------

async function launchSim() {
  const scenario = document.getElementById('launch-scenario').value.trim();
  const rounds = parseInt(document.getElementById('launch-rounds').value) || 10;
  const agents = parseInt(document.getElementById('launch-agents').value) || 40;
  const seed = document.getElementById('launch-seed').value.trim();
  const statusEl = document.getElementById('launch-status');

  if (!scenario) {
    statusEl.textContent = 'Please enter a scenario prompt.';
    statusEl.className = 'launch-status error';
    return;
  }

  // Disable button, show loading with progress steps
  const btn = document.getElementById('btn-launch');
  btn.disabled = true;
  btn.textContent = 'Launching...';

  const steps = [
    'Analyzing scenario...',
    'Extracting stakeholder personas...',
    `Generating ${agents} agent profiles...`,
    'Building social graph...',
    'Starting simulation engine...',
  ];
  let stepIndex = 0;
  statusEl.textContent = steps[0];
  statusEl.className = 'launch-status loading';
  const stepTimer = setInterval(() => {
    stepIndex++;
    if (stepIndex < steps.length) {
      statusEl.textContent = steps[stepIndex];
    }
  }, 3000);

  try {
    const body = {
      scenario_prompt: scenario,
      total_rounds: rounds,
      target_agent_count: agents,
    };
    if (seed) body.seed_document_text = seed;

    const res = await fetch(`${API}/api/simulation/launch`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const data = await res.json();
    clearInterval(stepTimer);

    if (res.ok) {
      // Reset UI state
      feedActions = [];
      roundSummaries = [];
      syntheses = [];
      stats = { posts: 0, likes: 0, replies: 0, reposts: 0, actions: 0 };
      tokenUsage = { prompt: 0, completion: 0 };
      updateStats();
      updateTokenDisplay();
      document.getElementById('feed-list').innerHTML = '';
      document.getElementById('synthesis-panel').classList.add('hidden');

      currentStatus = 'preparing';
      updateStatusBadge('preparing');
      updateNewSimButton();

      // Close modal after brief success flash
      statusEl.textContent = 'Simulation launched!';
      statusEl.className = 'launch-status success';
      setTimeout(closeLaunchModal, 800);

      // Refresh agents after short delay
      setTimeout(loadAgents, 2000);
    } else {
      statusEl.textContent = data.error || 'Launch failed';
      statusEl.className = 'launch-status error';
    }
  } catch (e) {
    clearInterval(stepTimer);
    statusEl.textContent = 'Network error: ' + e.message;
    statusEl.className = 'launch-status error';
  } finally {
    btn.disabled = false;
    btn.textContent = 'Launch Simulation';
  }
}

// ----------------------------------------------------------------
// God's Eye
// ----------------------------------------------------------------

async function injectEvent() {
  const eventType = document.getElementById('event-type').value;
  const content = document.getElementById('event-content').value.trim();
  const roundInput = document.getElementById('event-round').value;

  if (!content) return;

  const body = {
    event_type: eventType,
    content: content,
  };
  if (roundInput) body.inject_at_round = parseInt(roundInput);

  const result = await postJson('/api/god-eye/inject', body);

  const log = document.getElementById('event-log');
  log.insertAdjacentHTML('afterbegin', renderEventEntry({ event_type: eventType, content }));

  document.getElementById('event-content').value = '';
  document.getElementById('event-round').value = '';
}

// ----------------------------------------------------------------
// Continue simulation
// ----------------------------------------------------------------

async function continueSim() {
  const roundsInput = document.getElementById('continue-rounds');
  const statusEl = document.getElementById('continue-status');
  const extraRounds = parseInt(roundsInput.value) || 5;

  statusEl.textContent = `Continuing for ${extraRounds} rounds...`;
  statusEl.className = 'launch-status';

  try {
    const data = await postJson('/api/simulation/continue', { extra_rounds: extraRounds });
    if (data.status === 'continuing') {
      statusEl.textContent = 'Resumed!';
      statusEl.className = 'launch-status success';
      setTimeout(() => { statusEl.textContent = ''; }, 2000);
    } else {
      statusEl.textContent = data.error || 'Continue failed';
      statusEl.className = 'launch-status error';
    }
  } catch (e) {
    statusEl.textContent = 'Network error: ' + e.message;
    statusEl.className = 'launch-status error';
  }
}

function updateContinueVisibility() {
  const section = document.getElementById('continue-section');
  if (section) {
    section.style.display = currentStatus === 'finished' ? '' : 'none';
  }
}

// ----------------------------------------------------------------
// Polling (fallback + initial load)
// ----------------------------------------------------------------

async function initialLoad() {
  try {
    const status = await fetchJson('/api/status');
    updateStatus(status);

    // Auto-open launch modal when server starts idle
    if (status.status === 'idle') {
      openLaunchModal();
    }

    // Load existing syntheses
    try {
      const synths = await fetchJson('/api/syntheses');
      if (synths && synths.length > 0) {
        syntheses = synths;
        const latest = synths[synths.length - 1];
        showSynthesis(latest.round, latest.text);
      }
    } catch (e) {}
  } catch (e) {
    console.log('Server not ready yet, retrying...');
    setTimeout(initialLoad, 2000);
    return;
  }

  loadAgents();
  setInterval(loadAgents, 10000);
}

// ----------------------------------------------------------------
// Example scenarios
// ----------------------------------------------------------------

const EXAMPLE_SCENARIOS = [
  {
    scenario: "Simulate the public reaction on social media after OpenAI announces that GPT-5 will cost $200/month for the Pro tier, while making the free tier significantly more limited. Explore reactions from AI developers, startup founders, students, competitors (Google, Anthropic, Meta), tech journalists, and everyday users debating accessibility, pricing, and the future of AI democratization.",
    seed: "OpenAI CEO Sam Altman announced today that GPT-5, codenamed \"Orion,\" will launch next month with a new pricing structure. The Pro tier jumps from $20/month to $200/month, offering unlimited GPT-5 access, advanced reasoning, and a 1M token context window. The free tier will be restricted to GPT-4o-mini with 10 messages per day. Altman stated: \"Building frontier AI is extraordinarily expensive. We need sustainable pricing to continue pushing the boundaries.\" The announcement comes as Google DeepMind releases Gemini 3 Ultra for free, and Anthropic offers Claude 4 at $30/month. Tech Twitter erupted immediately, with #OpenAIGreed and #GPT5Pricing trending within hours."
  },
  {
    scenario: "Simulate the social media firestorm after the EU passes a law requiring all AI-generated content to carry a visible watermark and mandatory disclosure. Explore reactions from artists, AI companies, content creators, politicians, journalists, meme accounts, and everyday users debating creativity, censorship, and enforcement.",
    seed: "The European Parliament voted 421-178 today to pass the AI Transparency Act, requiring all AI-generated images, videos, and text to carry a visible \"AI-Generated\" watermark. Companies have 90 days to comply or face fines up to 6% of global revenue. The law also mandates that social media platforms label AI content automatically. Tech CEOs including Elon Musk and Mark Zuckerberg called it \"the death of innovation in Europe.\" Meanwhile, artists' unions celebrated, calling it \"a first step toward protecting human creativity.\" The hashtags #AIWatermark, #EUvsAI, and #ProtectHumanArt are trending globally."
  },
  {
    scenario: "Simulate the online debate after a leaked internal memo reveals that a major social media platform has been secretly using user DMs to train its AI models. Explore reactions from privacy advocates, tech workers, influencers, politicians, competing platforms, cybersecurity experts, and regular users.",
    seed: "An anonymous whistleblower leaked a 47-page internal document from Meta revealing that Instagram and WhatsApp messages — including DMs, voice notes, and shared photos — have been used to train Meta's Llama AI models since 2024. The memo, verified by three independent journalists, states: \"User content provides invaluable training signal. Opt-out mechanisms were deliberately made difficult to find.\" Meta's stock dropped 8% in pre-market trading. The FTC announced an immediate investigation. #DeleteMeta and #PrivacyBreach are the top trending hashtags worldwide."
  },
  {
    scenario: "Simulate the heated online discourse after Tesla announces a fully autonomous robotaxi service launching in 3 US cities, with the first reported accident occurring on day one. Explore reactions from Tesla fans, autonomous vehicle skeptics, urban planners, taxi/rideshare drivers, regulators, accident victims' advocates, and tech analysts.",
    seed: "Tesla launched its Robotaxi service today in Austin, Miami, and Phoenix with a fleet of 5,000 vehicles. Rides cost $0.50/mile — roughly 70% cheaper than Uber. Within 6 hours of launch, a Tesla Robotaxi in Miami ran a red light and collided with a cyclist, who was hospitalized with non-life-threatening injuries. Tesla's VP of Autonomy stated the incident was caused by \"a rare edge case in sensor fusion\" and that the fleet would continue operating. The Miami mayor called for an immediate suspension. Uber and Lyft stocks surged 12%. #RobotaxiFail and #TeslaRobotaxi are trending."
  },
  {
    scenario: "Simulate the global social media reaction after NASA confirms the detection of a repeating, structured radio signal from a star system 42 light-years away that does not match any known natural phenomenon. Explore reactions from scientists, conspiracy theorists, religious leaders, sci-fi fans, world leaders, astronomers, and everyday people processing the implications.",
    seed: "NASA Administrator Bill Chen held a press conference at 2 PM EST announcing that the James Webb Space Telescope and the SETI Institute have independently confirmed a repeating radio signal from the TRAPPIST-1 system, 42 light-years away. The signal repeats every 73 minutes with a mathematical structure based on prime numbers. \"This does not match any known natural phenomenon,\" said Dr. Sarah Kim, lead researcher. \"We are not claiming extraterrestrial intelligence, but we cannot rule it out.\" The Vatican issued a statement saying faith and science are \"complementary.\" China and the ESA confirmed they are redirecting telescopes to verify. Social media exploded instantly."
  },
  {
    scenario: "Simulate the social media chaos after a massive global outage takes down Google, YouTube, Gmail, and Android services for 48 hours. Explore reactions from businesses that depend on Google, competitors (Microsoft, Apple), remote workers, students, content creators losing ad revenue, and people rediscovering life without Google.",
    seed: "At 3:17 AM UTC, all Google services went offline simultaneously — Search, YouTube, Gmail, Google Cloud, Google Maps, Android Push Notifications, and the Play Store. Google's status page itself is unreachable. A brief internal message leaked on X reads: \"Critical infrastructure compromise. All hands. Do not discuss externally.\" As of hour 24, no official statement has been made. Microsoft Teams and Outlook saw a 400% traffic spike. Amazon AWS reported record new signups. Schools relying on Google Classroom cancelled classes. YouTubers report losing thousands in daily ad revenue. #GoogleDown has become the most-used hashtag in Twitter history."
  },
  {
    scenario: "Simulate the social media discourse after Apple announces it is acquiring Nintendo for $85 billion, planning to make Nintendo games Apple-exclusive. Explore reactions from gamers, game developers, Nintendo fans, Sony/Microsoft, tech analysts, antitrust advocates, Japanese culture commentators, and Apple enthusiasts.",
    seed: "Apple CEO Tim Cook and Nintendo President Shuntaro Furukawa held a joint press conference in Kyoto announcing Apple's acquisition of Nintendo for $85 billion — the largest tech acquisition in history. All future Nintendo titles, including Mario, Zelda, and Pokemon, will be exclusive to Apple devices. The Nintendo Switch successor will be cancelled; instead, Apple will release an \"Apple Game\" handheld running iOS. Furukawa stated: \"Nintendo's spirit of play will live on in a new ecosystem.\" Sony's stock rose 15% as investors bet gamers would flee to PlayStation. The gaming community responded with a mix of outrage and disbelief."
  },
  {
    scenario: "Simulate online reactions after a breakthrough study published in Nature demonstrates that a new drug reverses biological aging by 20 years in clinical trials, but it costs $2 million per treatment. Explore reactions from biotech investors, healthcare advocates, ethicists, wealthy tech figures, anti-aging researchers, insurance companies, and regular people debating inequality and access.",
    seed: "A team at Stanford and Altos Labs published results in Nature showing their drug \"Revitase\" successfully reversed biological age by an average of 20 years in a Phase 2 trial of 200 patients aged 60-80. Telomere length increased, organ function improved, and cognitive performance matched 40-year-olds. However, the treatment requires a personalized cocktail of reprogrammed stem cells costing approximately $2 million. Several billionaires including Jeff Bezos (an Altos Labs investor) reportedly began treatment immediately. The WHO called for \"equitable access discussions.\" Biotech stocks surged across the board. #ImmortalityForSale and #Revitase are trending."
  },
  {
    scenario: "Simulate the social media reaction after the US government announces a universal basic income pilot of $2,000/month for all adults, funded by a new tax on AI company revenues. Explore reactions from AI companies, workers in automated industries, economists, politicians from both parties, small business owners, libertarians, and progressives.",
    seed: "President Harris announced a landmark executive order establishing the \"American AI Dividend\" — a $2,000/month universal basic income for all US adults, funded by a 15% tax on revenue from companies deriving more than 50% of their income from AI products and services. The program begins in January with a 3-year pilot. OpenAI, Google, and Microsoft would collectively contribute an estimated $180 billion annually. Tech company stocks plunged 9% on average. Labor unions praised the move. Republican leaders called it \"socialism powered by innovation theft.\" #AIDividend and #UBI are the top trending topics."
  },
  {
    scenario: "Simulate the internet discourse after a viral deepfake video of a world leader declaring war turns out to be AI-generated, causing a brief stock market crash before being debunked. Explore reactions from fact-checkers, government officials, AI safety researchers, military analysts, stock traders who lost money, platform trust & safety teams, and citizens questioning what's real.",
    seed: "At 9:42 AM EST, a hyper-realistic video appearing to show Chinese President Xi Jinping declaring a naval blockade of Taiwan spread across X, Telegram, and TikTok. The S&P 500 plunged 4.2% in 17 minutes. The Pentagon raised alert levels. At 10:15 AM, Chinese state media issued a denial. By 10:30 AM, AI detection tools confirmed the video was generated using an open-source model. Markets partially recovered but ended the day down 1.8%. Total estimated losses: $900 billion in market cap. The video was traced to an anonymous account created 3 hours prior. The incident reignited calls for AI regulation. #DeepfakeWar and #AIThreat dominated the news cycle for days."
  }
];

let currentScenarioIndex = 0;

function shuffleScenario() {
  currentScenarioIndex = (currentScenarioIndex + 1) % EXAMPLE_SCENARIOS.length;
  const example = EXAMPLE_SCENARIOS[currentScenarioIndex];
  document.getElementById('launch-scenario').value = example.scenario;
  document.getElementById('launch-seed').value = example.seed;

  // Animate the dice button
  const btn = document.querySelector('.dice-btn');
  btn.classList.add('spin');
  setTimeout(() => btn.classList.remove('spin'), 300);
}

// ----------------------------------------------------------------
// Keyboard shortcuts
// ----------------------------------------------------------------

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') { closeModal(); closeLaunchModal(); }
  if (e.key === 'p' && !e.target.matches('input,textarea,select')) pauseSim();
  if (e.key === 'r' && !e.target.matches('input,textarea,select')) resumeSim();
  if (e.key === 'n' && !e.target.matches('input,textarea,select')) openLaunchModal();
});

// ----------------------------------------------------------------
// Init
// ----------------------------------------------------------------

connectWebSocket();
initialLoad();
