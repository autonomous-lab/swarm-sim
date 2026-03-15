// ================================================================
// Swarm-Sim — WebSocket Client & State Management
// ================================================================

const API = window.location.origin;
let ws = null;
let currentFilter = 'all';
let currentTab = 'feed';
let feedActions = [];
let roundSummaries = [];
let stats = { posts: 0, likes: 0, replies: 0, reposts: 0, actions: 0 };

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
    // Limit DOM nodes
    while (el.children.length > 200) el.removeChild(el.lastChild);
  }
}

function handleRoundStart(msg) {
  document.getElementById('round-info').textContent = `Round ${msg.round}`;
  document.getElementById('agent-count').textContent = `${msg.active_agents} active`;

  // Add separator in feed
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
  refreshTrending();
}

function handleGodEyeInject(event) {
  const log = document.getElementById('event-log');
  log.insertAdjacentHTML('afterbegin', renderEventEntry(event));
}

function handleSimEnd(msg) {
  updateStatusBadge('finished');
  document.getElementById('btn-pause').disabled = true;
  document.getElementById('btn-stop').disabled = true;
}

// ----------------------------------------------------------------
// Status
// ----------------------------------------------------------------

function updateStatus(snap) {
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
  const agents = await fetchJson('/api/agents');
  renderAgentsList(agents);
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

  const contentMap = { feed: 'feed-list', trending: 'trending-list', timeline: 'timeline-list' };
  document.getElementById(contentMap[tab]).classList.add('active');

  if (tab === 'trending') refreshTrending();
  if (tab === 'timeline') refreshTimeline();
}

async function refreshTrending() {
  const trending = await fetchJson('/api/trending');
  const el = document.getElementById('trending-list');
  if (trending.length === 0) {
    el.innerHTML = '<div style="color:var(--text-muted);padding:20px;text-align:center">No trending posts yet</div>';
  } else {
    el.innerHTML = trending.map((p, i) => renderTrendingPost(p, i + 1)).join('');
  }
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

// ----------------------------------------------------------------
// Simulation controls
// ----------------------------------------------------------------

async function pauseSim() {
  await postJson('/api/simulation/pause');
  updateStatusBadge('paused');
  document.getElementById('btn-pause').disabled = true;
  document.getElementById('btn-resume').disabled = false;
}

async function resumeSim() {
  await postJson('/api/simulation/resume');
  updateStatusBadge('running');
  document.getElementById('btn-pause').disabled = false;
  document.getElementById('btn-resume').disabled = true;
}

async function stopSim() {
  await postJson('/api/simulation/stop');
  updateStatusBadge('finished');
  document.getElementById('btn-pause').disabled = true;
  document.getElementById('btn-resume').disabled = true;
  document.getElementById('btn-stop').disabled = true;
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

  // Add to log
  const log = document.getElementById('event-log');
  log.insertAdjacentHTML('afterbegin', renderEventEntry({ event_type: eventType, content }));

  // Clear form
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
  } catch (e) {
    console.log('Server not ready yet, retrying...');
    setTimeout(initialLoad, 2000);
    return;
  }

  loadAgents();

  // Refresh agents periodically
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
