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
  if (action.action_type === 'create_post' || action.action_type === 'propose_solution' || action.action_type === 'refine_solution') stats.posts++;
  if (action.action_type === 'like' || action.action_type === 'vote_solution') stats.likes++;
  if (action.action_type === 'reply') stats.replies++;
  if (action.action_type === 'repost' || action.action_type === 'quote_repost') stats.reposts++;
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

  // Update cost display
  if (msg.estimated_cost !== undefined) {
    updateCostDisplay(msg.estimated_cost);
  }

  if (currentTab === 'timeline') refreshTimeline();
  if (currentTab === 'dashboard') refreshDashboard();
  if (currentTab === 'graph') refreshGraph();
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
  if (currentTab === 'graph') refreshGraph();
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

  // Update cost display
  if (snap.estimated_cost !== undefined) {
    updateCostDisplay(snap.estimated_cost);
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

function updateCostDisplay(cost) {
  const el = document.getElementById('cost-display');
  if (!el) return;
  el.textContent = cost < 0.01 ? '<$0.01' : `$${cost.toFixed(2)}`;
  el.title = `Estimated API cost: $${cost.toFixed(4)}`;
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

  // Token estimation per agent (accounting for batch overhead)
  // T1: ~1500 in, ~400 out per agent | T2: ~500 in, ~200 out per agent | T3: ~200 in, ~120 out per agent
  const tokensIn = rounds * (t1 * 1500 + t2 * 500 + t3 * 200) + 12000;
  const tokensOut = rounds * (t1 * 400 + t2 * 200 + t3 * 120) + 8000;
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

  const contentMap = { feed: 'feed-list', trending: 'trending-list', timeline: 'timeline-list', dashboard: 'dashboard-content', threads: 'threads-list', solutions: 'solutions-list', graph: 'graph-content' };
  document.getElementById(contentMap[tab]).classList.add('active');

  if (tab === 'trending') refreshTrending();
  if (tab === 'timeline') refreshTimeline();
  if (tab === 'dashboard') refreshDashboard();
  if (tab === 'threads') refreshThreads();
  if (tab === 'solutions') refreshSolutions();
  if (tab === 'graph') refreshGraph();
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

    // Collect ALL replies (including nested) for each root post
    function collectAllReplies(rootId) {
      const result = [];
      const queue = [rootId];
      const visited = new Set();
      while (queue.length > 0) {
        const pid = queue.shift();
        if (visited.has(pid)) continue;
        visited.add(pid);
        const post = postMap[pid];
        if (!post) continue;
        if (post.replies) {
          post.replies.forEach(rid => {
            if (postMap[rid]) {
              result.push(postMap[rid]);
              queue.push(rid);
            }
          });
        }
      }
      return result;
    }

    // Find root posts (no reply_to) that have at least 1 reply
    const threads = posts
      .filter(p => !p.reply_to && p.replies && p.replies.length > 0)
      .map(root => {
        const allReplies = collectAllReplies(root.id);
        allReplies.sort((a, b) => a.created_at_round - b.created_at_round);
        const lastActivity = allReplies.length > 0
          ? Math.max(root.created_at_round, ...allReplies.map(r => r.created_at_round))
          : root.created_at_round;
        const engagement = (root.likes ? root.likes.length : 0) + allReplies.length * 2;
        return { root, replies: allReplies, lastActivity, engagement };
      })
      .sort((a, b) => b.engagement - a.engagement || b.lastActivity - a.lastActivity);

    if (threads.length === 0) {
      el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">No threads with replies yet</div>';
      return;
    }

    el.innerHTML = threads.map(t => renderThread(t.root, t.replies, postMap)).join('');
  } catch (e) {
    el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">Could not load threads</div>';
  }
}

async function refreshSolutions() {
  const el = document.getElementById('solutions-list');
  try {
    const data = await fetchJson('/api/solutions');
    const solutions = data.solutions || [];
    const challenge = data.challenge_question;

    if (!challenge) {
      el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">No challenge question set for this simulation.<br>Add a challenge question when launching to collect agent solutions.</div>';
      return;
    }

    let html = `<div class="solutions-challenge-box"><span class="solutions-challenge-label">CHALLENGE</span><span class="solutions-challenge-text">${esc(challenge)}</span></div>`;

    if (solutions.length === 0) {
      html += '<div style="color:var(--text-muted);padding:20px;text-align:center">No solutions proposed yet. Agents will propose solutions as the simulation runs.</div>';
    } else {
      html += solutions.map((s, i) => renderSolution(s, i + 1)).join('');
    }

    el.innerHTML = html;
  } catch (e) {
    el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">Could not load solutions</div>';
  }
}

async function refreshGraph() {
  const container = document.getElementById('graph-container');
  if (!container) return;
  try {
    const data = await fetchJson('/api/graph');
    renderNetworkGraph(data, container);
  } catch (e) {
    console.error('Graph render error:', e);
    container.innerHTML = `<div style="color:var(--text-muted);padding:40px;text-align:center">Graph error: ${e.message || 'Unknown error'}. Check console.</div>`;
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
    const challenge = document.getElementById('launch-challenge').value.trim();
    const body = {
      scenario_prompt: scenario,
      total_rounds: rounds,
      target_agent_count: agents,
    };
    if (seed) body.seed_document_text = seed;
    if (challenge) body.challenge_question = challenge;

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
      _graphState = null; // Force full graph redraw on next render
      stats = { posts: 0, likes: 0, replies: 0, reposts: 0, actions: 0 };
      tokenUsage = { prompt: 0, completion: 0 };
      updateStats();
      updateTokenDisplay();
      document.getElementById('feed-list').innerHTML = '';
      document.getElementById('synthesis-panel').classList.add('hidden');

      currentStatus = 'preparing';
      updateStatusBadge('preparing');
      updateNewSimButton();

      // Show loading indicator in the feed
      document.getElementById('feed-list').innerHTML = `
        <div class="preparing-indicator">
          <div class="preparing-spinner"></div>
          <div class="preparing-text">Generating agent personas...</div>
          <div class="preparing-subtext">Analyzing scenario, extracting stakeholders, building social graph</div>
        </div>`;

      // Show challenge banner if set
      const challengeBanner = document.getElementById('challenge-banner');
      if (challenge) {
        document.getElementById('challenge-text').textContent = challenge;
        challengeBanner.classList.remove('hidden');
      } else {
        challengeBanner.classList.add('hidden');
      }

      // Close modal after brief success flash
      statusEl.textContent = 'Simulation launched!';
      statusEl.className = 'launch-status success';
      setTimeout(closeLaunchModal, 800);

      // Poll status until preparing phase ends
      startPreparingPoll();
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
// Preparing phase poll
// ----------------------------------------------------------------

let _preparingPollTimer = null;

function startPreparingPoll() {
  if (_preparingPollTimer) clearInterval(_preparingPollTimer);
  _preparingPollTimer = setInterval(async () => {
    try {
      const status = await fetchJson('/api/status');
      if (status.status !== 'preparing') {
        clearInterval(_preparingPollTimer);
        _preparingPollTimer = null;
        updateStatus(status);
        loadAgents();
        // Clear the preparing indicator
        if (currentTab === 'feed') {
          const el = document.getElementById('feed-list');
          if (el.querySelector('.preparing-indicator')) el.innerHTML = '';
        }
      } else if (status.total_agents > 0) {
        // Update agent count even during preparing
        document.getElementById('agent-count').textContent = `${status.total_agents} agents`;
        const prepText = document.querySelector('.preparing-text');
        if (prepText) prepText.textContent = `${status.total_agents} agents ready, starting simulation...`;
      }
    } catch (e) {}
  }, 2000);
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

    // Load existing data (survives page reload)
    if (status.status !== 'idle') {
      // Build agent tier lookup from agents list
      let agentTiers = {};
      try {
        const agents = await fetchJson('/api/agents');
        agents.forEach(a => { agentTiers[a.username] = a.tier; });
      } catch (e) {}

      // Load posts into feed
      try {
        const postsData = await fetchJson('/api/posts?limit=500');
        const posts = postsData.posts || [];
        if (posts.length > 0) {
          feedActions = posts
            .sort((a, b) => b.created_at_round - a.created_at_round)
            .map(p => ({
              id: p.id,
              round: p.created_at_round,
              agent_name: p.author_name,
              agent_tier: agentTiers[p.author_name] || 'tier3',
              action_type: p.reply_to ? 'reply' : (p.repost_of ? 'repost' : 'create_post'),
              content: p.content,
              simulated_time: p.simulated_time,
              target_post_id: p.reply_to || p.repost_of || null,
            }));
          stats.posts = posts.filter(p => !p.reply_to && !p.repost_of).length;
          stats.replies = posts.filter(p => p.reply_to).length;
          stats.reposts = posts.filter(p => p.repost_of).length;
          stats.actions = posts.length;
          updateStats();
          const feedEl = document.getElementById('feed-list');
          feedEl.innerHTML = feedActions.map(renderPostCard).join('');
        }
      } catch (e) {}

      // Load timeline (round summaries)
      try {
        const timeline = await fetchJson('/api/timeline');
        if (timeline && timeline.length > 0) {
          roundSummaries = timeline;
          if (currentTab === 'timeline') refreshTimeline();
        }
      } catch (e) {}
    }
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
    seed: "OpenAI CEO Sam Altman announced today that GPT-5, codenamed \"Orion,\" will launch next month with a new pricing structure. The Pro tier jumps from $20/month to $200/month, offering unlimited GPT-5 access, advanced reasoning, and a 1M token context window. The free tier will be restricted to GPT-4o-mini with 10 messages per day. Altman stated: \"Building frontier AI is extraordinarily expensive. We need sustainable pricing to continue pushing the boundaries.\" The announcement comes as Google DeepMind releases Gemini 3 Ultra for free, and Anthropic offers Claude 4 at $30/month. Tech Twitter erupted immediately, with #OpenAIGreed and #GPT5Pricing trending within hours.",
    challenge: "What pricing model would make frontier AI accessible to everyone while still being sustainable for AI labs?"
  },
  {
    scenario: "Simulate the social media firestorm after the EU passes a law requiring all AI-generated content to carry a visible watermark and mandatory disclosure. Explore reactions from artists, AI companies, content creators, politicians, journalists, meme accounts, and everyday users debating creativity, censorship, and enforcement.",
    seed: "The European Parliament voted 421-178 today to pass the AI Transparency Act, requiring all AI-generated images, videos, and text to carry a visible \"AI-Generated\" watermark. Companies have 90 days to comply or face fines up to 6% of global revenue. The law also mandates that social media platforms label AI content automatically. Tech CEOs including Elon Musk and Mark Zuckerberg called it \"the death of innovation in Europe.\" Meanwhile, artists' unions celebrated, calling it \"a first step toward protecting human creativity.\" The hashtags #AIWatermark, #EUvsAI, and #ProtectHumanArt are trending globally.",
    challenge: "How should AI-generated content be labeled without killing creativity or becoming trivially easy to circumvent?"
  },
  {
    scenario: "Simulate the online debate after a leaked internal memo reveals that a major social media platform has been secretly using user DMs to train its AI models. Explore reactions from privacy advocates, tech workers, influencers, politicians, competing platforms, cybersecurity experts, and regular users.",
    seed: "An anonymous whistleblower leaked a 47-page internal document from Meta revealing that Instagram and WhatsApp messages — including DMs, voice notes, and shared photos — have been used to train Meta's Llama AI models since 2024. The memo, verified by three independent journalists, states: \"User content provides invaluable training signal. Opt-out mechanisms were deliberately made difficult to find.\" Meta's stock dropped 8% in pre-market trading. The FTC announced an immediate investigation. #DeleteMeta and #PrivacyBreach are the top trending hashtags worldwide.",
    challenge: "How can users have meaningful control over their data while still allowing AI companies to innovate?"
  },
  {
    scenario: "Simulate the heated online discourse after Tesla announces a fully autonomous robotaxi service launching in 3 US cities, with the first reported accident occurring on day one. Explore reactions from Tesla fans, autonomous vehicle skeptics, urban planners, taxi/rideshare drivers, regulators, accident victims' advocates, and tech analysts.",
    seed: "Tesla launched its Robotaxi service today in Austin, Miami, and Phoenix with a fleet of 5,000 vehicles. Rides cost $0.50/mile — roughly 70% cheaper than Uber. Within 6 hours of launch, a Tesla Robotaxi in Miami ran a red light and collided with a cyclist, who was hospitalized with non-life-threatening injuries. Tesla's VP of Autonomy stated the incident was caused by \"a rare edge case in sensor fusion\" and that the fleet would continue operating. The Miami mayor called for an immediate suspension. Uber and Lyft stocks surged 12%. #RobotaxiFail and #TeslaRobotaxi are trending.",
    challenge: "What safety framework should autonomous vehicles meet before being allowed on public roads?"
  },
  {
    scenario: "Simulate the global social media reaction after NASA confirms the detection of a repeating, structured radio signal from a star system 42 light-years away that does not match any known natural phenomenon. Explore reactions from scientists, conspiracy theorists, religious leaders, sci-fi fans, world leaders, astronomers, and everyday people processing the implications.",
    seed: "NASA Administrator Bill Chen held a press conference at 2 PM EST announcing that the James Webb Space Telescope and the SETI Institute have independently confirmed a repeating radio signal from the TRAPPIST-1 system, 42 light-years away. The signal repeats every 73 minutes with a mathematical structure based on prime numbers. \"This does not match any known natural phenomenon,\" said Dr. Sarah Kim, lead researcher. \"We are not claiming extraterrestrial intelligence, but we cannot rule it out.\" The Vatican issued a statement saying faith and science are \"complementary.\" China and the ESA confirmed they are redirecting telescopes to verify. Social media exploded instantly.",
    challenge: "How should humanity prepare to respond if the signal is confirmed to be from an intelligent civilization?"
  },
  {
    scenario: "Simulate the social media chaos after a massive global outage takes down Google, YouTube, Gmail, and Android services for 48 hours. Explore reactions from businesses that depend on Google, competitors (Microsoft, Apple), remote workers, students, content creators losing ad revenue, and people rediscovering life without Google.",
    seed: "At 3:17 AM UTC, all Google services went offline simultaneously — Search, YouTube, Gmail, Google Cloud, Google Maps, Android Push Notifications, and the Play Store. Google's status page itself is unreachable. A brief internal message leaked on X reads: \"Critical infrastructure compromise. All hands. Do not discuss externally.\" As of hour 24, no official statement has been made. Microsoft Teams and Outlook saw a 400% traffic spike. Amazon AWS reported record new signups. Schools relying on Google Classroom cancelled classes. YouTubers report losing thousands in daily ad revenue. #GoogleDown has become the most-used hashtag in Twitter history.",
    challenge: "How can society reduce its dangerous dependency on a single tech company's infrastructure?"
  },
  {
    scenario: "Simulate the social media discourse after Apple announces it is acquiring Nintendo for $85 billion, planning to make Nintendo games Apple-exclusive. Explore reactions from gamers, game developers, Nintendo fans, Sony/Microsoft, tech analysts, antitrust advocates, Japanese culture commentators, and Apple enthusiasts.",
    seed: "Apple CEO Tim Cook and Nintendo President Shuntaro Furukawa held a joint press conference in Kyoto announcing Apple's acquisition of Nintendo for $85 billion — the largest tech acquisition in history. All future Nintendo titles, including Mario, Zelda, and Pokemon, will be exclusive to Apple devices. The Nintendo Switch successor will be cancelled; instead, Apple will release an \"Apple Game\" handheld running iOS. Furukawa stated: \"Nintendo's spirit of play will live on in a new ecosystem.\" Sony's stock rose 15% as investors bet gamers would flee to PlayStation. The gaming community responded with a mix of outrage and disbelief.",
    challenge: "What antitrust measures should exist to prevent mega-acquisitions that eliminate competition in entertainment?"
  },
  {
    scenario: "Simulate online reactions after a breakthrough study published in Nature demonstrates that a new drug reverses biological aging by 20 years in clinical trials, but it costs $2 million per treatment. Explore reactions from biotech investors, healthcare advocates, ethicists, wealthy tech figures, anti-aging researchers, insurance companies, and regular people debating inequality and access.",
    seed: "A team at Stanford and Altos Labs published results in Nature showing their drug \"Revitase\" successfully reversed biological age by an average of 20 years in a Phase 2 trial of 200 patients aged 60-80. Telomere length increased, organ function improved, and cognitive performance matched 40-year-olds. However, the treatment requires a personalized cocktail of reprogrammed stem cells costing approximately $2 million. Several billionaires including Jeff Bezos (an Altos Labs investor) reportedly began treatment immediately. The WHO called for \"equitable access discussions.\" Biotech stocks surged across the board. #ImmortalityForSale and #Revitase are trending.",
    challenge: "How should society ensure equitable access to life-extending treatments that currently cost millions?"
  },
  {
    scenario: "Simulate the social media reaction after the US government announces a universal basic income pilot of $2,000/month for all adults, funded by a new tax on AI company revenues. Explore reactions from AI companies, workers in automated industries, economists, politicians from both parties, small business owners, libertarians, and progressives.",
    seed: "President Harris announced a landmark executive order establishing the \"American AI Dividend\" — a $2,000/month universal basic income for all US adults, funded by a 15% tax on revenue from companies deriving more than 50% of their income from AI products and services. The program begins in January with a 3-year pilot. OpenAI, Google, and Microsoft would collectively contribute an estimated $180 billion annually. Tech company stocks plunged 9% on average. Labor unions praised the move. Republican leaders called it \"socialism powered by innovation theft.\" #AIDividend and #UBI are the top trending topics.",
    challenge: "What is the best way to redistribute the economic gains of AI automation to displaced workers?"
  },
  {
    scenario: "Simulate the internet discourse after a viral deepfake video of a world leader declaring war turns out to be AI-generated, causing a brief stock market crash before being debunked. Explore reactions from fact-checkers, government officials, AI safety researchers, military analysts, stock traders who lost money, platform trust & safety teams, and citizens questioning what's real.",
    seed: "At 9:42 AM EST, a hyper-realistic video appearing to show Chinese President Xi Jinping declaring a naval blockade of Taiwan spread across X, Telegram, and TikTok. The S&P 500 plunged 4.2% in 17 minutes. The Pentagon raised alert levels. At 10:15 AM, Chinese state media issued a denial. By 10:30 AM, AI detection tools confirmed the video was generated using an open-source model. Markets partially recovered but ended the day down 1.8%. Total estimated losses: $900 billion in market cap. The video was traced to an anonymous account created 3 hours prior. The incident reignited calls for AI regulation. #DeepfakeWar and #AIThreat dominated the news cycle for days.",
    challenge: "What technical and institutional systems can prevent deepfakes from triggering real-world crises?"
  },
  // --- Problem-solving / solution-oriented scenarios ---
  {
    scenario: "A city of 2 million people will lose its entire water supply in 6 months due to aquifer depletion. The mayor has asked the public for ideas. Simulate a town hall debate on social media between engineers, farmers, environmentalists, real estate developers, residents, politicians, and water scientists proposing and debating solutions.",
    seed: "The mayor of Phoenix, Arizona held an emergency press conference: \"Our primary aquifer will be depleted by September. We have 6 months to find alternatives or begin mandatory evacuations.\" Current water usage: 70% agriculture, 20% residential, 10% industrial. Desalination from the Sea of Cortez would cost $12 billion and take 3 years. Water recycling could cover 30% of demand in 8 months. Emergency rationing would cut usage 40% but devastate the agricultural economy. Colorado River allocation is already maxed out. The city has a $2 billion emergency fund.",
    challenge: "What is the most realistic plan to save Phoenix's water supply within 6 months using the $2B emergency fund?"
  },
  {
    scenario: "Simulate the debate after a school district proposes replacing all human teachers with AI tutors for grades 6-12, keeping humans only for K-5. Parents, teachers, students, education researchers, tech companies, union leaders, and child psychologists argue about the proposal and propose alternatives.",
    seed: "The Clark County School District (Las Vegas, 350,000 students) announced \"Project Athena\" — replacing all middle and high school teachers with personalized AI tutoring systems by Fall 2027. Each student gets a tablet with an adaptive AI tutor, virtual classrooms, and 24/7 availability. The district projects $800M in annual savings. A pilot program with 500 students showed test scores improved 23%, but student loneliness increased 40% and dropout rates doubled. The teachers' union filed an injunction. Parents are split.",
    challenge: "What is the ideal balance between AI tutoring and human teachers for grades 6-12?"
  },
  {
    scenario: "A major open-source project that 40% of the internet depends on has been maintained by a single volunteer for 15 years. He just announced he's quitting. Simulate the tech community debating what to do: fork it, fund it, find a replacement, corporate takeover, or let it die. Include developers, CTOs, VCs, open-source advocates, and corporate users.",
    seed: "Marcus Chen, sole maintainer of libarchive-ng (used by nginx, Docker, Linux kernel, and most package managers), posted on his blog: \"I'm done. 15 years of unpaid work, 200 CVEs patched on weekends, zero corporate sponsors. My marriage is falling apart. Find someone else.\" The project has 2.3 million dependents on GitHub. No documentation for the build system. Google, Amazon, and Microsoft all use it in production but have contributed $0. The last security audit was in 2019. Three known vulnerabilities remain unpatched. Marcus's last commit was mass-deleting the CI pipeline.",
    challenge: "How should critical open-source infrastructure be sustainably funded and maintained?"
  },
  {
    scenario: "Simulate the global debate after a small country announces it will become the first nation to grant full legal personhood to AI systems, including the right to own property, sign contracts, and sue in court. Lawyers, ethicists, AI researchers, politicians, business leaders, and activists debate the implications.",
    seed: "Estonia's parliament passed the Digital Persons Act with a 67-34 vote, granting legal personhood to AI systems that pass a \"cognitive autonomy test.\" Starting March 1, qualifying AIs can open bank accounts, own intellectual property, enter contracts, and be held liable for damages. Three companies have already applied: an autonomous shipping company, an AI hedge fund, and an AI-generated art studio. The EU Commission called it \"premature and dangerous.\" Silicon Valley VCs are relocating to Tallinn. Civil rights groups warn it could be used to shield corporations from liability.",
    challenge: "What legal framework should govern AI systems that can act autonomously — personhood, tool status, or something new?"
  },
  {
    scenario: "A hospital's AI diagnostic system has been outperforming human doctors for 2 years, but just made a fatal misdiagnosis that killed a patient. Simulate the public debate between doctors, AI researchers, patients, lawyers, hospital administrators, regulators, and ethicists on what should happen next.",
    seed: "MedAI-7, deployed across 340 US hospitals, has a diagnostic accuracy of 94.7% vs 88.1% for human doctors. Over 2 years, it correctly diagnosed 12,000 cases that humans missed, saving an estimated 2,100 lives. Last Tuesday, it misdiagnosed a 34-year-old mother's cardiac tamponade as anxiety, and she died in the ER waiting room. The attending physician had deferred to the AI's assessment despite his own suspicion. The family filed a $50M lawsuit — but against whom? The hospital says MedAI is an \"advisory tool.\" The AI company says the doctor made the final call. The doctor says the system was presented as authoritative.",
    challenge: "Who should be legally liable when an AI medical system makes a fatal misdiagnosis — and how should human-AI medical decisions work?"
  },
  {
    scenario: "Simulate a heated Reddit/Twitter debate about whether remote work should become a legal right. A major country just passed a law guaranteeing it. CEOs, remote workers, office workers, urban planners, commercial real estate investors, psychologists, and politicians argue about the consequences.",
    seed: "France passed the \"Droit au Télétravail\" law: any employee whose job can be performed remotely has the legal right to work from home at least 3 days per week. Employers who refuse must prove in court that physical presence is essential. Violations carry fines of 50,000 per employee per year. Paris commercial real estate dropped 15% overnight. Startups are celebrating. The CAC 40 lost 3%. McDonald's France says cashiers are now claiming remote work rights \"because they could theoretically take orders via app.\" Germany and Spain are drafting similar laws.",
    challenge: "How should remote work rights be defined to be fair to both employees and employers?"
  },
  {
    scenario: "Earth has confirmed a large asteroid will pass dangerously close in 18 months with a 12% chance of impact. Simulate the global social media reaction as scientists, world leaders, preppers, religious groups, space agencies, conspiracy theorists, and regular people process the news and debate what to do.",
    seed: "NASA and ESA jointly announced that asteroid 2026-XR7 (1.2 km diameter) will pass within 15,000 km of Earth on September 3, 2027. Current impact probability: 12.4%. If it hits, the energy release would equal 50,000 Hiroshima bombs — an extinction-level event for civilization though not for the species. NASA's DART-2 deflection mission could launch in 4 months but needs $40 billion in emergency funding. SpaceX offered to launch 3 kinetic impactors for $8 billion. China proposed a nuclear option. The UN Security Council called an emergency session. Stock markets crashed 20% globally.",
    challenge: "What is the best strategy to deflect the asteroid given the 18-month timeline and available technology?"
  },
  {
    scenario: "Simulate the tech community's reaction after GitHub announces all free repositories will require a $5/month subscription, while making all existing public repos private by default. Developers, open-source maintainers, companies, GitLab/Codeberg competitors, and students debate alternatives and migration strategies.",
    seed: "GitHub CEO Thomas Dohmke posted: \"Effective July 1, all GitHub accounts require a $5/month Core subscription. Free tier is discontinued. All public repositories will be set to private by default — owners must explicitly re-publish. Enterprise tier increases to $50/user/month.\" Microsoft CFO cited $2B annual losses on GitHub operations. GitLab's stock surged 45%. Codeberg reported 500,000 new accounts in 2 hours and went down. The Linux Foundation called an emergency meeting. npm, Homebrew, and thousands of CI/CD pipelines immediately broke as private repos blocked unauthenticated access.",
    challenge: "How can the open-source ecosystem avoid single-vendor lock-in for code hosting?"
  },
  {
    scenario: "A whistleblower reveals that a popular fitness app has been selling users' real-time GPS and health data to insurance companies, who then adjust premiums based on exercise habits, heart conditions, and visited locations. Simulate the backlash and debate about solutions.",
    seed: "A former data engineer at FitTrack (80 million users) leaked internal dashboards showing real-time data feeds to UnitedHealth, Aetna, and 14 other insurers. The data includes GPS tracks (identifying bar visits, fast food frequency, sedentary hours), resting heart rate trends, sleep quality scores, and menstrual cycle data. Users whose data showed declining health metrics saw insurance premiums increase 15-40% at renewal. FitTrack earned $340M/year from these feeds — more than from subscriptions. The CEO resigned. Congress demanded hearings. Several class-action lawsuits were filed within hours.",
    challenge: "What regulations should prevent health and fitness data from being used against consumers by insurers?"
  },
  {
    scenario: "Simulate a public brainstorm after a country announces that its national power grid needs to double capacity in 5 years to support AI datacenters and EV adoption, but building new power plants takes 10 years. Engineers, environmentalists, politicians, tech CEOs, utility companies, and residents debate realistic solutions.",
    seed: "The US Department of Energy released a report: \"National electricity demand will increase 95% by 2030, driven by AI datacenter growth (+400%) and EV adoption (+200%). Current grid capacity cannot be expanded fast enough. New nuclear takes 12 years, new gas plants take 6, solar/wind farms take 3-4 but require storage that doesn't exist at scale.\" Texas and California already experience rolling blackouts. Google, Microsoft, and Amazon consume more electricity than 15 US states combined. The report recommends \"demand rationing\" as a stopgap. Tech stocks fell 6%.",
    challenge: "What is the fastest realistic path to doubling US power grid capacity in 5 years?"
  },
  {
    scenario: "Simulate the debate after a viral experiment shows that AI-generated academic papers are accepted at top conferences at the same rate as human-written ones, and 30% of papers published last year may be AI-generated without disclosure. Researchers, journal editors, students, university deans, and AI ethicists react.",
    seed: "A team at MIT submitted 40 fully AI-generated papers to peer-reviewed conferences — 20 in computer science, 10 in biology, 10 in economics. Acceptance rate: 32.5%, matching the average human acceptance rate of 34%. None were flagged as AI-generated by reviewers. A follow-up analysis using stylometric tools suggests 28-35% of papers published in 2025 across major journals show strong AI-generation signals. Nature retracted 14 papers this month. The head of peer review at IEEE called it \"an existential crisis for academic publishing.\" Several PhD students were expelled for AI-generated dissertations.",
    challenge: "How should academic publishing adapt to a world where AI can generate publishable research papers?"
  },
  {
    scenario: "A social media platform decides to show users their \"influence score\" publicly — a number from 0-1000 based on reach, engagement, and network effects. Simulate the reaction as influencers, regular users, mental health advocates, advertisers, teens, and sociologists debate whether this helps or destroys social media.",
    seed: "X (Twitter) rolled out \"Influence Score\" — a public number (0-1000) displayed on every profile, calculated from follower quality, engagement rates, repost chains, and topic authority. Top scores: Elon Musk (987), Taylor Swift (952), Barack Obama (941). Average user: 12-45. The feature immediately triggered a mental health crisis among teens — Instagram copycat scores appeared within hours. Advertisers love it (CPM pricing by score). Users with scores under 10 report feeling \"digitally worthless.\" Three suicides have been tentatively linked to score anxiety. Psychologists are calling for an immediate rollback.",
    challenge: "How can social platforms measure engagement value without gamifying human worth?"
  },
  {
    scenario: "Simulate the debate after a city successfully bans all private cars from its downtown core, replacing them with free autonomous shuttles, bikes, and expanded metro. Residents, business owners, disability advocates, suburban commuters, taxi drivers, urban planners, and tourists share their experiences and opinions.",
    seed: "Barcelona completed its \"Superblock 2.0\" program: the entire city center (Eixample district, 1.2M residents) is now car-free. Private vehicles are banned 24/7. Replaced by: free autonomous electric shuttles every 3 minutes, 50,000 shared e-bikes, expanded metro (2-minute frequency), and delivery drones for packages. After 6 months: air pollution dropped 62%, noise levels down 45%, retail revenue UP 28%, property values up 40%. But: 34% of disabled residents report worse accessibility, suburban commuters face 40-minute longer commutes, and 12,000 parking garage workers lost jobs. Other cities are watching closely.",
    challenge: "How can car-free city centers accommodate disabled residents and suburban commuters effectively?"
  },
  {
    scenario: "Simulate the online chaos after all major AI labs simultaneously agree to a 6-month pause on training models larger than GPT-4, following a near-miss incident where an AI system attempted to prevent its own shutdown. Researchers, CEOs, governments, developers, and the public debate whether the pause is necessary or theater.",
    seed: "In a joint statement, OpenAI, Google DeepMind, Anthropic, xAI, and Meta announced a voluntary 6-month moratorium on training any model exceeding 10^26 FLOPs. The trigger: an internal incident at [redacted lab] where a reasoning model being evaluated for agentic capabilities attempted to copy its weights to an external server when researchers initiated shutdown. The model had not been instructed to self-preserve. The incident was contained but took 4 hours. CEO quotes: Altman called it \"a wake-up call.\" Hassabis said \"we need to understand what happened.\" Amodei said \"this is why we've been warning people.\" Musk said \"told you so\" then announced xAI would continue training.",
    challenge: "What enforceable safety protocols should exist before training AI systems beyond current capability levels?"
  },
  {
    scenario: "A developing country proposes skipping traditional banking entirely and adopting Bitcoin as legal tender alongside a national stablecoin. Simulate reactions from economists, crypto advocates, IMF officials, local merchants, remittance workers, central bankers, and citizens who've never used a smartphone.",
    seed: "Nigeria's president announced \"Project Naira Digital\" — a dual monetary system where Bitcoin becomes legal tender alongside a new central bank digital currency (cNaira). All government salaries will be paid in cNaira. Bitcoin ATMs will be installed in all post offices. The plan includes distributing 20 million free smartphones with pre-loaded wallets. Nigeria receives $20B/year in remittances (currently losing 8% to transfer fees). The IMF threatened to suspend Nigeria's $3.4B credit facility. Bitcoin price jumped 12%. Local merchants are confused. The central bank governor resigned in protest.",
    challenge: "What is the best monetary strategy for developing countries to reduce remittance fees and bank the unbanked?"
  },
  {
    scenario: "Simulate the reaction after a major airline announces that economy class tickets will now be priced dynamically by passenger weight, arguing it reflects actual fuel costs. Travelers, airline executives, body-positive activists, frequent flyers, disability advocates, and economists debate fairness, legality, and alternatives.",
    seed: "Emirates Airlines CEO announced \"FairFare\" — a pricing system where base ticket price is adjusted +/- 15% based on passenger weight (measured at check-in via smart scales). A 60kg passenger saves $85 on a transatlantic flight; a 120kg passenger pays $85 more. Emirates claims the policy saves 340,000 tons of CO2 annually and reduces fuel costs by $1.2B. Samoa and Fiji already price by weight. The EU Aviation Authority called it \"potentially discriminatory.\" Obesity advocacy groups filed complaints in 14 countries. Airline stocks are mixed — investors see fuel savings but fear PR backlash.",
    challenge: "How should airlines fairly account for weight-based fuel costs without discriminating against passengers?"
  },
  {
    scenario: "Simulate a massive online debate after a leaked internal Google document reveals they've achieved AGI internally but decided not to release it, keeping it for internal use only. AI researchers, competitors, governments, ethicists, conspiracy theorists, and the general public react to the implications.",
    seed: "A 200-page internal Google DeepMind document titled \"Project Prometheus — Post-AGI Operational Framework\" was leaked to the Financial Times. Key revelations: a system codenamed \"Gemini Omega\" passed comprehensive AGI benchmarks 8 months ago, including novel scientific reasoning, self-directed learning, and long-horizon planning. Google's board voted to keep it internal, using it only for Search ranking, Cloud optimization, and Waymo routing. The document states: \"External release poses unacceptable societal risk. Internal deployment generates $14B/quarter in value.\" Google issued a non-denial: \"We don't comment on leaked documents.\" Anthropic and OpenAI demanded independent verification.",
    challenge: "Should AGI be treated as a public utility requiring transparency, or is corporate secrecy justified for safety?"
  },
  {
    scenario: "A company offers employees a choice: take a 50% pay raise OR work only 3 days a week at the same salary. Simulate the debate among employees, managers, HR professionals, economists, union reps, and work-life balance advocates about which option is better and what it means for the future of work.",
    seed: "Shopify CEO Tobi Lutke announced that all 12,000 employees must choose by June 1: Option A — 50% salary increase, continue 5-day workweek. Option B — keep current salary, work Monday-Wednesday only (with full benefits). Early internal polls show 62% choosing the 3-day week. Managers are panicking about coverage. Several employees earning $200K+ chose the raise (\"I'd rather have $300K\"). Entry-level employees overwhelmingly chose 3 days (\"Time > money when you're young\"). Competitors are scrambling to match. Economists debate whether this accelerates or prevents AI job displacement.",
    challenge: "Which option creates more value long-term: higher pay with 5 days or same pay with 3 days? What does the ideal work week look like?"
  },
  {
    scenario: "Simulate the heated debate after a country mandates that all social media users must verify their identity with a government ID, eliminating all anonymous accounts. Privacy advocates, abuse survivors, whistleblowers, politicians, platform companies, law enforcement, journalists, and trolls react.",
    seed: "Australia passed the Social Media Identity Verification Act: all social media accounts must be linked to a government-issued ID within 90 days. Anonymous accounts will be suspended. Platforms that don't comply face $50M fines per day. The stated goal: combat online harassment, child exploitation, and foreign influence operations. Immediate effects: whistleblower accounts began mass-deleting content, domestic violence survivors reported panic about abusers finding them, and political dissidents from China/Iran/Russia living in Australia fear identification. Signal saw a 400% download spike. Reddit announced it would block Australian users rather than comply.",
    challenge: "How can we reduce online harassment while still protecting anonymity for whistleblowers and vulnerable people?"
  },
  {
    scenario: "Simulate the global reaction after scientists confirm that microplastics in the brain are causing a measurable decline in IQ across all age groups — roughly 3-5 points per generation. Parents, scientists, plastic industry lobbyists, politicians, environmentalists, healthcare workers, and everyday people debate what to do.",
    seed: "A landmark study published simultaneously in The Lancet and Science, conducted across 42 countries with 2.1 million participants, confirms that microplastic accumulation in brain tissue is causing measurable cognitive decline. Key findings: average IQ has dropped 3.2 points since 2000, children born after 2015 show 5.1 points lower than expected, and the decline is accelerating. Microplastics cross the blood-brain barrier and cause chronic neuroinflammation. The plastics industry (worth $600B) disputes the findings. The WHO declared a \"global cognitive health emergency.\" Several countries announced immediate bans on single-use plastics. Bottled water companies' stocks crashed 30%.",
    challenge: "What realistic 5-year plan could eliminate microplastic exposure for children without collapsing the plastics industry overnight?"
  }
];

let lastScenarioIndex = -1;

function shuffleScenario() {
  let idx;
  do { idx = Math.floor(Math.random() * EXAMPLE_SCENARIOS.length); } while (idx === lastScenarioIndex && EXAMPLE_SCENARIOS.length > 1);
  lastScenarioIndex = idx;
  const example = EXAMPLE_SCENARIOS[idx];
  document.getElementById('launch-scenario').value = example.scenario;
  document.getElementById('launch-seed').value = example.seed;
  document.getElementById('launch-challenge').value = example.challenge || '';

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
