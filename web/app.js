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
  updateLaunchPanelVisibility();
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

  // Show scenario banner
  if (snap.scenario_prompt) {
    const banner = document.getElementById('scenario-banner');
    document.getElementById('scenario-text').textContent = snap.scenario_prompt;
    banner.classList.remove('hidden');
  }

  updateLaunchPanelVisibility();
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

// ----------------------------------------------------------------
// Launch panel visibility
// ----------------------------------------------------------------

function updateLaunchPanelVisibility() {
  const panel = document.getElementById('launch-panel');
  const main = document.getElementById('app');
  if (currentStatus === 'idle' || currentStatus === 'finished') {
    panel.style.display = 'flex';
    // Don't hide main — let user browse results while launch panel is visible
  } else {
    panel.style.display = 'none';
  }
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

  const contentMap = { feed: 'feed-list', trending: 'trending-list', timeline: 'timeline-list', dashboard: 'dashboard-content' };
  document.getElementById(contentMap[tab]).classList.add('active');

  if (tab === 'trending') refreshTrending();
  if (tab === 'timeline') refreshTimeline();
  if (tab === 'dashboard') refreshDashboard();
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
  updateLaunchPanelVisibility();
}

// ----------------------------------------------------------------
// Launch simulation from UI
// ----------------------------------------------------------------

async function launchSim() {
  const scenario = document.getElementById('launch-scenario').value.trim();
  const rounds = parseInt(document.getElementById('launch-rounds').value) || 5;
  const seed = document.getElementById('launch-seed').value.trim();
  const statusEl = document.getElementById('launch-status');

  if (!scenario) {
    statusEl.textContent = 'Please enter a scenario prompt.';
    statusEl.className = 'launch-status error';
    return;
  }

  // Disable button, show loading
  const btn = document.getElementById('btn-launch');
  btn.disabled = true;
  btn.textContent = 'Launching...';
  statusEl.textContent = 'Extracting entities and generating agents...';
  statusEl.className = 'launch-status loading';

  try {
    const body = {
      scenario_prompt: scenario,
      total_rounds: rounds,
    };
    if (seed) body.seed_document_text = seed;

    const res = await fetch(`${API}/api/simulation/launch`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const data = await res.json();

    if (res.ok) {
      // Reset UI state
      feedActions = [];
      roundSummaries = [];
      syntheses = [];
      stats = { posts: 0, likes: 0, replies: 0, reposts: 0, actions: 0 };
      updateStats();
      document.getElementById('feed-list').innerHTML = '';
      document.getElementById('synthesis-panel').classList.add('hidden');

      currentStatus = 'preparing';
      updateStatusBadge('preparing');
      updateLaunchPanelVisibility();

      statusEl.textContent = 'Simulation launched!';
      statusEl.className = 'launch-status success';

      // Refresh agents after short delay
      setTimeout(loadAgents, 2000);
    } else {
      statusEl.textContent = data.error || 'Launch failed';
      statusEl.className = 'launch-status error';
    }
  } catch (e) {
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
// Polling (fallback + initial load)
// ----------------------------------------------------------------

async function initialLoad() {
  try {
    const status = await fetchJson('/api/status');
    updateStatus(status);

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
// Keyboard shortcuts
// ----------------------------------------------------------------

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') closeModal();
  if (e.key === 'p' && !e.target.matches('input,textarea,select')) pauseSim();
  if (e.key === 'r' && !e.target.matches('input,textarea,select')) resumeSim();
});

// ----------------------------------------------------------------
// Init
// ----------------------------------------------------------------

connectWebSocket();
initialLoad();
