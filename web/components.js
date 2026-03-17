// ================================================================
// Swarm-Sim UI Components
// ================================================================

function renderAgentCard(agent) {
  const stanceDot = getStanceDot(agent.stance);
  return `
    <div class="agent-card" onclick="showAgent('${agent.id}')" data-tier="${agent.tier}">
      <div class="agent-tier-dot ${agent.tier}"></div>
      <div class="agent-info">
        <div class="agent-username">@${esc(agent.username)} ${stanceDot}</div>
        <div class="agent-bio">${esc(agent.bio)}</div>
        <div class="agent-stats">${agent.post_count} posts &middot; ${agent.follower_count} followers</div>
      </div>
    </div>`;
}

function renderPostCard(action) {
  const tierClass = action.agent_tier || 'tier3';
  const tierLabel = { tier1: 'VIP', tier2: 'STD', tier3: 'FIG' }[tierClass] || '';
  const actionBadge = getActionBadge(action.action_type);
  const time = action.simulated_time ? new Date(action.simulated_time).toLocaleTimeString() : '';

  let content = '';
  if (action.content) {
    content = `<div class="post-content">${esc(action.content)}</div>`;
  }

  let targetInfo = '';
  if (action.target_post_id && action.action_type !== 'create_post') {
    targetInfo = `<span style="color:var(--text-muted);font-size:11px">on ${action.target_post_id.slice(0, 8)}</span>`;
  }

  return `
    <div class="post-card">
      <div class="post-header">
        <span class="post-author">@${esc(action.agent_name)}</span>
        <span class="post-tier ${tierClass}">${tierLabel}</span>
        ${actionBadge}
        ${targetInfo}
        <span class="post-time">R${action.round} ${time}</span>
      </div>
      ${content}
      ${action.reasoning ? `<div style="font-size:11px;color:var(--text-muted);font-style:italic">${esc(action.reasoning)}</div>` : ''}
    </div>`;
}

function renderThread(root, replies) {
  const likeCount = root.likes ? root.likes.length : 0;
  const replyCount = replies.length;
  const collapsed = replyCount > 3;
  const visibleReplies = collapsed ? replies.slice(0, 2) : replies;
  const hiddenCount = collapsed ? replyCount - 2 : 0;
  const threadId = root.id.slice(0, 8);

  let repliesHtml = visibleReplies.map(r => {
    const time = r.simulated_time ? new Date(r.simulated_time).toLocaleTimeString() : '';
    return `
      <div class="thread-reply">
        <div class="thread-reply-header">
          <span class="post-author">@${esc(r.author_name)}</span>
          <span class="post-time">R${r.created_at_round} ${time}</span>
        </div>
        <div class="thread-reply-content">${esc(r.content)}</div>
      </div>`;
  }).join('');

  if (collapsed) {
    repliesHtml += `
      <button class="thread-toggle" onclick="expandThread(this, '${threadId}')">
        Show ${hiddenCount} more ${hiddenCount === 1 ? 'reply' : 'replies'}
      </button>`;
    const hiddenReplies = replies.slice(2).map(r => {
      const time = r.simulated_time ? new Date(r.simulated_time).toLocaleTimeString() : '';
      return `
        <div class="thread-reply thread-hidden" data-thread="${threadId}">
          <div class="thread-reply-header">
            <span class="post-author">@${esc(r.author_name)}</span>
            <span class="post-time">R${r.created_at_round} ${time}</span>
          </div>
          <div class="thread-reply-content">${esc(r.content)}</div>
        </div>`;
    }).join('');
    repliesHtml += hiddenReplies;
  }

  const time = root.simulated_time ? new Date(root.simulated_time).toLocaleTimeString() : '';

  return `
    <div class="thread-card">
      <div class="post-card" style="margin-bottom:0;border-bottom-left-radius:0;border-bottom-right-radius:0">
        <div class="post-header">
          <span class="post-author">@${esc(root.author_name)}</span>
          <span class="action-badge post">POST</span>
          <span class="post-time">R${root.created_at_round} ${time}</span>
        </div>
        <div class="post-content">${esc(root.content)}</div>
        <div class="thread-engagement">
          <span class="thread-stat">${likeCount} like${likeCount !== 1 ? 's' : ''}</span>
          <span class="thread-stat">${replyCount} repl${replyCount !== 1 ? 'ies' : 'y'}</span>
        </div>
      </div>
      <div class="thread-replies">
        ${repliesHtml}
      </div>
    </div>`;
}

function expandThread(btn, threadId) {
  const hidden = btn.parentElement.querySelectorAll(`.thread-hidden[data-thread="${threadId}"]`);
  hidden.forEach(el => el.classList.remove('thread-hidden'));
  btn.remove();
}

function renderRoundSeparator(round, summary) {
  const stats = summary
    ? `${summary.active_agents} active | ${summary.new_posts}p ${summary.new_replies}r ${summary.new_likes}l`
    : '';
  return `<div class="round-separator">Round ${round} ${stats}</div>`;
}

function renderTimelineEntry(summary) {
  return `
    <div class="timeline-entry">
      <span class="round-num">R${summary.round}</span>
      <span class="round-stats">
        ${summary.active_agents} agents |
        ${summary.new_posts} posts,
        ${summary.new_replies} replies,
        ${summary.new_likes} likes,
        ${summary.new_reposts} reposts
        ${summary.events_injected > 0 ? `| <span style="color:var(--accent-yellow)">${summary.events_injected} events</span>` : ''}
      </span>
    </div>`;
}

function renderTrendingPost(post, rank) {
  return `
    <div class="post-card">
      <div class="post-header">
        <span style="color:var(--accent-yellow);font-weight:700">#${rank}</span>
        <span class="post-author">@${esc(post.author)}</span>
        <span class="post-time">eng: ${post.engagement.toFixed(0)}</span>
      </div>
      <div class="post-content">${esc(post.content_preview)}</div>
      <div class="post-actions-bar">
        <span class="post-action-item">${post.likes} likes</span>
        <span class="post-action-item">${post.replies} replies</span>
      </div>
    </div>`;
}

// ---------------------------------------------------------------------------
// Enhanced Agent Detail Modal
// ---------------------------------------------------------------------------

function renderAgentDetail(data) {
  const { profile, state, recent_posts } = data;
  const tierLabel = { tier1: 'VIP', tier2: 'Standard', tier3: 'Figurant' }[profile.tier] || '';
  const stanceDot = getStanceDot(profile.stance);
  const stanceClass = profile.stance || 'neutral';

  // Profile section
  let profileHtml = `
    <div class="agent-detail-header">
      <div class="agent-detail-name">${esc(profile.name)}</div>
      <div class="agent-detail-username">@${esc(profile.username)} &middot; ${tierLabel}</div>
    </div>
    <div class="agent-detail-section">
      <p>${esc(profile.bio)}</p>
    </div>
    <div class="detail-meta-row">
      <span class="stance-pill ${stanceClass}">${stanceDot} ${esc(profile.stance)}</span>
      <span class="sentiment-indicator">Sentiment: ${renderSentimentBar(profile.sentiment_bias)}</span>
    </div>`;

  // Demographics
  let demoHtml = '';
  const demoItems = [];
  if (profile.age) demoItems.push(`Age: ${profile.age}`);
  if (profile.profession) demoItems.push(esc(profile.profession));
  if (profile.activity_level !== undefined) demoItems.push(`Activity: ${(profile.activity_level * 100).toFixed(0)}%`);
  if (demoItems.length > 0) {
    demoHtml = `<div class="detail-demo">${demoItems.join(' &middot; ')}</div>`;
  }

  // Interests
  let interestsHtml = '';
  if (profile.interests && profile.interests.length > 0) {
    interestsHtml = `<div class="detail-interests">${
      profile.interests.map(i => `<span class="interest-pill">${esc(i)}</span>`).join('')
    }</div>`;
  }

  // Stats
  let statsHtml = '';
  if (state) {
    statsHtml = `
    <div class="detail-stats-grid">
      <div class="detail-stat"><span class="detail-stat-num">${state.post_ids.length}</span><span class="detail-stat-label">Posts</span></div>
      <div class="detail-stat"><span class="detail-stat-num">${state.followers.length}</span><span class="detail-stat-label">Followers</span></div>
      <div class="detail-stat"><span class="detail-stat-num">${state.following.length}</span><span class="detail-stat-label">Following</span></div>
      <div class="detail-stat"><span class="detail-stat-num">${state.liked_post_ids.length}</span><span class="detail-stat-label">Likes</span></div>
    </div>`;
  }

  // Tabs: Activity, Memory, Posts
  let tabsHtml = `
    <div class="detail-tabs">
      <button class="detail-tab active" onclick="switchDetailTab(this,'activity')">Activity</button>
      <button class="detail-tab" onclick="switchDetailTab(this,'memory')">Memory</button>
      <button class="detail-tab" onclick="switchDetailTab(this,'posts')">Posts</button>
    </div>`;

  // Activity tab (action_log)
  let activityHtml = '<div class="detail-tab-content active" data-tab="activity">';
  if (state && state.action_log && state.action_log.length > 0) {
    const entries = [...state.action_log].reverse();
    activityHtml += entries.map(e => {
      const badge = getActionBadge(e.action_type);
      const preview = e.content ? esc(e.content).slice(0, 100) : '';
      return `<div class="action-log-entry">
        <span class="action-log-round">R${e.round}</span>
        ${badge}
        ${preview ? `<span class="action-log-text">${preview}</span>` : ''}
      </div>`;
    }).join('');
  } else {
    activityHtml += '<div class="detail-empty">No activity yet</div>';
  }
  activityHtml += '</div>';

  // Memory tab
  let memoryHtml = '<div class="detail-tab-content" data-tab="memory">';
  if (state && state.memory) {
    if (state.memory.pinned.length > 0) {
      memoryHtml += '<div class="memory-section-label">Pinned Memories</div>';
      memoryHtml += state.memory.pinned.map(m =>
        `<div class="memory-item pinned">${esc(m)}</div>`
      ).join('');
    }
    if (state.memory.recent.length > 0) {
      memoryHtml += '<div class="memory-section-label">Recent Observations</div>';
      memoryHtml += state.memory.recent.slice(-10).map(([round, obs]) =>
        `<div class="memory-item">[R${round}] ${esc(obs)}</div>`
      ).join('');
    }
    if (state.memory.pinned.length === 0 && state.memory.recent.length === 0) {
      memoryHtml += '<div class="detail-empty">No memories yet</div>';
    }
  } else {
    memoryHtml += '<div class="detail-empty">No memory data</div>';
  }
  memoryHtml += '</div>';

  // Posts tab
  let postsTabHtml = '<div class="detail-tab-content" data-tab="posts">';
  if (recent_posts && recent_posts.length > 0) {
    const sorted = [...recent_posts].sort((a, b) => b.created_at_round - a.created_at_round);
    postsTabHtml += sorted.slice(0, 15).map(p =>
      `<div class="post-card" style="margin-bottom:4px">
        <div class="post-content">${esc(p.content)}</div>
        <div class="post-actions-bar">
          <span class="post-action-item">${p.likes.length} likes</span>
          <span class="post-action-item">${p.replies.length} replies</span>
          <span class="post-time">R${p.created_at_round}</span>
        </div>
      </div>`
    ).join('');
  } else {
    postsTabHtml += '<div class="detail-empty">No posts yet</div>';
  }
  postsTabHtml += '</div>';

  return profileHtml + demoHtml + interestsHtml + statsHtml + tabsHtml + activityHtml + memoryHtml + postsTabHtml;
}

function switchDetailTab(btn, tabName) {
  const modal = btn.closest('.modal-content');
  modal.querySelectorAll('.detail-tab').forEach(b => b.classList.remove('active'));
  modal.querySelectorAll('.detail-tab-content').forEach(c => c.classList.remove('active'));
  btn.classList.add('active');
  modal.querySelector(`[data-tab="${tabName}"]`).classList.add('active');
}

function renderSentimentBar(bias) {
  const pct = ((bias + 1) / 2 * 100).toFixed(0);
  const color = bias > 0.3 ? 'var(--accent-green)' : bias < -0.3 ? 'var(--accent-red)' : 'var(--accent-yellow)';
  return `<span class="sentiment-bar"><span class="sentiment-fill" style="width:${pct}%;background:${color}"></span></span> <span style="font-size:11px">${bias.toFixed(1)}</span>`;
}

function getStanceDot(stance) {
  const colors = {
    supportive: 'var(--accent-green)',
    opposing: 'var(--accent-red)',
    neutral: 'var(--accent-yellow)',
    observer: 'var(--text-muted)'
  };
  const color = colors[stance] || 'var(--text-muted)';
  return `<span style="display:inline-block;width:6px;height:6px;border-radius:50%;background:${color};margin-left:4px"></span>`;
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

function renderDashboard(data) {
  if (!data || data.total_agents === 0) {
    return '<div class="detail-empty" style="padding:40px">No simulation data yet. Launch a simulation to see the dashboard.</div>';
  }

  // Metrics cards
  const metricsHtml = `
    <div class="dash-metrics">
      <div class="dash-metric-card"><div class="dash-metric-num">${data.total_agents}</div><div class="dash-metric-label">Agents</div></div>
      <div class="dash-metric-card"><div class="dash-metric-num">${data.total_posts}</div><div class="dash-metric-label">Posts</div></div>
      <div class="dash-metric-card"><div class="dash-metric-num">${data.total_actions}</div><div class="dash-metric-label">Actions</div></div>
      <div class="dash-metric-card"><div class="dash-metric-num">${data.activity_per_round.length}</div><div class="dash-metric-label">Rounds</div></div>
    </div>`;

  // Stance distribution (horizontal bars)
  const stanceEntries = Object.entries(data.stance_distribution);
  const maxStance = Math.max(...stanceEntries.map(([, v]) => v), 1);
  const stanceColors = { supportive: 'var(--accent-green)', opposing: 'var(--accent-red)', neutral: 'var(--accent-yellow)', observer: 'var(--text-muted)' };
  const stanceHtml = `
    <div class="dash-section">
      <h3>Stance Distribution</h3>
      <div class="dash-bars">
        ${stanceEntries.map(([stance, count]) => `
          <div class="dash-bar-row">
            <span class="dash-bar-label">${esc(stance)}</span>
            <div class="dash-bar-track"><div class="dash-bar-fill" style="width:${(count/maxStance*100).toFixed(0)}%;background:${stanceColors[stance] || 'var(--accent-cyan)'}"></div></div>
            <span class="dash-bar-value">${count}</span>
          </div>`).join('')}
      </div>
    </div>`;

  // Tier distribution
  const tierEntries = Object.entries(data.tier_distribution);
  const maxTier = Math.max(...tierEntries.map(([, v]) => v), 1);
  const tierColors = { VIP: 'var(--tier1)', Standard: 'var(--tier2)', Figurant: 'var(--tier3)' };
  const tierHtml = `
    <div class="dash-section">
      <h3>Tier Distribution</h3>
      <div class="dash-bars">
        ${tierEntries.map(([tier, count]) => `
          <div class="dash-bar-row">
            <span class="dash-bar-label">${esc(tier)}</span>
            <div class="dash-bar-track"><div class="dash-bar-fill" style="width:${(count/maxTier*100).toFixed(0)}%;background:${tierColors[tier] || 'var(--accent-cyan)'}"></div></div>
            <span class="dash-bar-value">${count}</span>
          </div>`).join('')}
      </div>
    </div>`;

  // Activity sparkline (per round)
  let sparkHtml = '';
  if (data.activity_per_round.length > 0) {
    const maxActivity = Math.max(...data.activity_per_round.map(r => r.posts + r.replies + r.likes), 1);
    sparkHtml = `
    <div class="dash-section">
      <h3>Activity per Round</h3>
      <div class="dash-sparkline">
        ${data.activity_per_round.map(r => {
          const total = r.posts + r.replies + r.likes;
          const h = (total / maxActivity * 60).toFixed(0);
          return `<div class="spark-col" title="R${r.round}: ${r.posts}p ${r.replies}r ${r.likes}l (${r.active_agents} active)">
            <div class="spark-bar" style="height:${h}px"></div>
            <div class="spark-label">R${r.round}</div>
          </div>`;
        }).join('')}
      </div>
    </div>`;
  }

  // Top agents leaderboard
  let leaderboardHtml = '';
  if (data.top_agents.length > 0) {
    leaderboardHtml = `
    <div class="dash-section">
      <h3>Top Agents</h3>
      <div class="dash-leaderboard">
        ${data.top_agents.map((a, i) => `
          <div class="leader-row">
            <span class="leader-rank">${i + 1}</span>
            <span class="leader-name">@${esc(a.username)}</span>
            <span class="leader-tier">${esc(a.tier)}</span>
            <span class="leader-stats">${a.post_count}p ${a.follower_count}f</span>
            <span class="leader-stance" style="color:${stanceColors[a.stance] || 'var(--text-muted)'}">${esc(a.stance)}</span>
          </div>`).join('')}
      </div>
    </div>`;
  }

  return metricsHtml + stanceHtml + tierHtml + sparkHtml + leaderboardHtml;
}

// ---------------------------------------------------------------------------
// Event + Badge helpers
// ---------------------------------------------------------------------------

function renderEventEntry(event) {
  const typeLabel = event.event_type || event.type || 'unknown';
  const content = event.content || event.data?.content || '';
  return `
    <div class="event-entry">
      <span class="event-type">[${esc(typeLabel)}]</span>
      ${esc(content).slice(0, 80)}${content.length > 80 ? '...' : ''}
    </div>`;
}

function renderSolution(solution, rank) {
  const engagement = solution.engagement || 0;
  return `
    <div class="solution-card">
      <div class="solution-rank">#${rank}</div>
      <div class="solution-body">
        <div class="solution-header">
          <span class="post-author">@${esc(solution.author_name)}</span>
          <span class="action-badge solution">SOLUTION</span>
          <span class="post-time">R${solution.created_at_round}</span>
        </div>
        <div class="solution-content">${esc(solution.content)}</div>
        <div class="solution-stats">
          <span class="solution-stat">${solution.likes || 0} likes</span>
          <span class="solution-stat">${solution.replies || 0} replies</span>
          <span class="solution-stat">${solution.reposts || 0} reposts</span>
          <span class="solution-engagement">${engagement.toFixed(0)} engagement</span>
        </div>
      </div>
    </div>`;
}

function getActionBadge(actionType) {
  const map = {
    create_post: '<span class="action-badge post">POST</span>',
    reply: '<span class="action-badge reply">REPLY</span>',
    like: '<span class="action-badge like">LIKE</span>',
    repost: '<span class="action-badge repost">REPOST</span>',
    follow: '<span class="action-badge follow">FOLLOW</span>',
    pin_memory: '<span class="action-badge pin">PIN</span>',
    propose_solution: '<span class="action-badge solution">SOLUTION</span>',
  };
  return map[actionType] || '';
}

function esc(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

// ================================================================
// Network Graph (D3.js force-directed) — MiroFish-style
// ================================================================

let _graphSimulation = null;
let _graphState = null; // { nodes, links, nodeCount, edgeCount, nodeMap, svg, g, link, node, simulation, container, zoomBehavior }

// Vibrant color palette for individual agents
const AGENT_COLORS = [
  '#e74c3c', '#e67e22', '#f1c40f', '#2ecc71', '#1abc9c',
  '#3498db', '#9b59b6', '#e91e63', '#00bcd4', '#ff5722',
  '#8bc34a', '#ff9800', '#673ab7', '#009688', '#f44336',
  '#2196f3', '#4caf50', '#ff6f00', '#7c4dff', '#00e676',
  '#ff1744', '#651fff', '#00b0ff', '#76ff03', '#ffc400',
  '#d500f9', '#1de9b6', '#ff3d00', '#304ffe', '#64dd17',
];

function renderNetworkGraph(data, container) {
  if (!data.nodes || data.nodes.length === 0) {
    container.innerHTML = '<div style="color:var(--text-muted);padding:40px;text-align:center">No agents yet. Launch a simulation to see the network.</div>';
    _graphState = null;
    if (_graphSimulation) { _graphSimulation.stop(); _graphSimulation = null; }
    return;
  }

  // Incremental update: if same nodes exist, just update edges + node data
  if (_graphState && _graphState.container === container && _graphState.nodeCount === data.nodes.length) {
    _updateGraphData(data);
    return;
  }

  // Full redraw needed (first render or node count changed)
  container.innerHTML = '';
  if (_graphSimulation) { _graphSimulation.stop(); _graphSimulation = null; }

  const width = container.clientWidth || 800;
  const height = container.clientHeight || 600;

  const tierLabel = { tier1: 'VIP', tier2: 'Standard', tier3: 'Figurant' };
  const stanceColors = { supportive: '#2ecc71', opposing: '#e74c3c', neutral: '#f39c12', observer: '#95a5a6' };

  // Assign unique color to each node
  const colorMap = {};
  data.nodes.forEach((n, i) => { colorMap[n.id] = AGENT_COLORS[i % AGENT_COLORS.length]; });

  // Count connections + compute degree centrality
  const inDegree = {}, outDegree = {};
  data.edges.forEach(e => {
    outDegree[e.source] = (outDegree[e.source] || 0) + 1;
    inDegree[e.target] = (inDegree[e.target] || 0) + 1;
  });

  // Adaptive sizing based on node count
  const isLarge = data.nodes.length > 50;
  const isMassive = data.nodes.length > 100;

  // Dynamic radius: base tier size + boost from connections/posts
  const maxDegree = Math.max(1, ...data.nodes.map(n => (inDegree[n.id] || 0) + (outDegree[n.id] || 0)));
  function nodeRadius(d) {
    const base = d.tier === 'tier1' ? (isLarge ? 18 : 22) : d.tier === 'tier2' ? (isLarge ? 10 : 14) : (isLarge ? 5 : 8);
    const degree = (inDegree[d.id] || 0) + (outDegree[d.id] || 0);
    const boost = (degree / maxDegree) * (isLarge ? 6 : 10);
    const postBoost = Math.min((d.post_count || 0) * (isLarge ? 0.8 : 1.5), isLarge ? 4 : 8);
    return base + boost + postBoost;
  }

  // Tier counts
  const tierCounts = { tier1: 0, tier2: 0, tier3: 0 };
  data.nodes.forEach(n => { tierCounts[n.tier] = (tierCounts[n.tier] || 0) + 1; });

  // Clone data for D3
  const nodes = data.nodes.map(n => ({ ...n, _r: nodeRadius(n) }));
  const links = data.edges.map(e => ({ source: e.source, target: e.target }));

  // SVG
  const svg = d3.select(container)
    .append('svg')
    .attr('width', width)
    .attr('height', height)
    .attr('viewBox', [0, 0, width, height]);

  // Defs: glow filter + arrow markers per color
  const defs = svg.append('defs');

  // Glow filter
  const glow = defs.append('filter').attr('id', 'glow');
  glow.append('feGaussianBlur').attr('stdDeviation', '3').attr('result', 'coloredBlur');
  const glowMerge = glow.append('feMerge');
  glowMerge.append('feMergeNode').attr('in', 'coloredBlur');
  glowMerge.append('feMergeNode').attr('in', 'SourceGraphic');

  // Zoom layer
  const g = svg.append('g');
  const zoomBehavior = d3.zoom()
    .scaleExtent([0.15, 4])
    .on('zoom', (event) => g.attr('transform', event.transform));
  svg.call(zoomBehavior);

  // Force simulation — adaptive to node count
  const nodeCount = nodes.length;
  const linkDist = isLarge ? 60 : 200;
  const linkStr = isLarge ? 0.02 : 0.08;
  const chargeT1 = isLarge ? -800 : -2000;
  const chargeT2 = isLarge ? -400 : -1200;
  const chargeT3 = isLarge ? -150 : -600;
  const collideGap = isLarge ? 8 : 25;

  const simulation = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(d => d.id).distance(linkDist).strength(linkStr))
    .force('charge', d3.forceManyBody().strength(d => d.tier === 'tier1' ? chargeT1 : d.tier === 'tier2' ? chargeT2 : chargeT3).distanceMax(isLarge ? 400 : 800))
    .force('center', d3.forceCenter(width / 2, height / 2).strength(0.01))
    .force('collide', d3.forceCollide().radius(d => d._r + collideGap).strength(1).iterations(isLarge ? 2 : 3))
    .force('x', d3.forceX(width / 2).strength(isLarge ? 0.015 : 0.008))
    .force('y', d3.forceY(height / 2).strength(isLarge ? 0.015 : 0.008))
    .alphaDecay(isLarge ? 0.02 : 0.01)
    .velocityDecay(0.3);
  _graphSimulation = simulation;

  // Links — curved paths, adaptive opacity
  const edgeCount = links.length;
  const defaultEdgeOpacity = edgeCount > 500 ? 0.06 : edgeCount > 200 ? 0.1 : 0.15;
  const defaultEdgeWidth = edgeCount > 500 ? 0.7 : edgeCount > 200 ? 0.9 : 1.2;

  const linkGroup = g.append('g').attr('class', 'graph-links');
  const link = linkGroup.selectAll('path')
    .data(links)
    .join('path')
    .attr('fill', 'none')
    .attr('stroke', d => {
      const c = colorMap[d.source.id || d.source] || '#555';
      return c;
    })
    .attr('stroke-opacity', defaultEdgeOpacity)
    .attr('stroke-width', defaultEdgeWidth);

  // Node groups
  const nodeGroup = g.append('g').attr('class', 'graph-nodes');
  const node = nodeGroup.selectAll('g')
    .data(nodes)
    .join('g')
    .attr('class', 'graph-node')
    .style('cursor', 'pointer')
    .call(d3.drag()
      .on('start', (event, d) => {
        if (!event.active) simulation.alphaTarget(0.3).restart();
        d.fx = d.x; d.fy = d.y;
      })
      .on('drag', (event, d) => { d.fx = event.x; d.fy = event.y; })
      .on('end', (event, d) => {
        if (!event.active) simulation.alphaTarget(0);
        d.fx = null; d.fy = null;
      })
    );

  // Outer glow ring (VIP only)
  node.filter(d => d.tier === 'tier1')
    .append('circle')
    .attr('r', d => d._r + 4)
    .attr('fill', 'none')
    .attr('stroke', d => colorMap[d.id])
    .attr('stroke-width', 2)
    .attr('stroke-opacity', 0.3)
    .attr('filter', 'url(#glow)');

  // Main circle
  node.append('circle')
    .attr('r', d => d._r)
    .attr('fill', d => colorMap[d.id])
    .attr('stroke', '#fff')
    .attr('stroke-width', d => d.tier === 'tier1' ? 3 : d.tier === 'tier2' ? 2 : 1)
    .attr('stroke-opacity', d => d.tier === 'tier3' ? 0.4 : 0.8);

  // Initials inside node
  node.append('text')
    .text(d => {
      const name = d.label.replace('@', '');
      return name.length <= 3 ? name : name.slice(0, 2).toUpperCase();
    })
    .attr('text-anchor', 'middle')
    .attr('dy', d => d._r > 12 ? '0.35em' : '0.3em')
    .attr('fill', '#fff')
    .attr('font-size', d => Math.max(8, d._r * 0.6) + 'px')
    .attr('font-weight', '700')
    .style('pointer-events', 'none')
    .style('text-shadow', '0 1px 2px rgba(0,0,0,0.5)');

  // Labels below nodes — hide tier3 when large graph
  node.append('text')
    .attr('class', 'node-label')
    .text(d => d.label)
    .attr('text-anchor', 'middle')
    .attr('dy', d => d._r + 14)
    .attr('fill', d => d.tier === 'tier1' ? '#fff' : d.tier === 'tier2' ? '#ccc' : '#888')
    .attr('font-size', d => d.tier === 'tier1' ? '12px' : d.tier === 'tier2' ? '10px' : '9px')
    .attr('font-weight', d => d.tier === 'tier1' ? '700' : '400')
    .style('pointer-events', 'none')
    .style('opacity', d => (isLarge && d.tier === 'tier3') ? 0 : 1);

  // Stance indicator dot
  node.append('circle')
    .attr('r', d => d.tier === 'tier3' ? 3 : 4)
    .attr('cx', d => d._r * 0.7)
    .attr('cy', d => -d._r * 0.7)
    .attr('fill', d => stanceColors[d.stance] || '#95a5a6')
    .attr('stroke', '#111')
    .attr('stroke-width', 1);

  // Tooltip
  const tooltip = d3.select(container)
    .append('div')
    .attr('class', 'graph-tooltip')
    .style('opacity', 0);

  let _selectedNode = null;

  node.on('mouseover', (event, d) => {
    tooltip.transition().duration(100).style('opacity', 1);
    const stanceColor = stanceColors[d.stance] || '#95a5a6';
    tooltip.html(`
      <div class="gt-header" style="border-left:3px solid ${colorMap[d.id]};padding-left:8px">
        <strong>${d.label}</strong>
        <span class="gt-tier gt-${d.tier}">${tierLabel[d.tier]}</span>
      </div>
      <div class="gt-stats">
        <span>${d.follower_count || 0} followers</span>
        <span>${d.following_count || 0} following</span>
        <span>${d.post_count || 0} posts</span>
      </div>
      <div class="gt-stance" style="color:${stanceColor}">${d.stance || 'unknown'}</div>
    `)
      .style('left', (event.offsetX + 16) + 'px')
      .style('top', (event.offsetY - 16) + 'px');

    if (!_selectedNode) highlightNode(d);
  })
  .on('mousemove', (event) => {
    tooltip.style('left', (event.offsetX + 16) + 'px')
           .style('top', (event.offsetY - 16) + 'px');
  })
  .on('mouseout', () => {
    tooltip.transition().duration(200).style('opacity', 0);
    if (!_selectedNode) resetHighlight();
  })
  .on('click', (event, d) => {
    event.stopPropagation();
    if (_selectedNode === d.id) {
      _selectedNode = null;
      resetHighlight();
    } else {
      _selectedNode = d.id;
      highlightNode(d);
    }
  });

  svg.on('click', () => {
    _selectedNode = null;
    resetHighlight();
  });

  function highlightNode(d) {
    const curLink = _graphState ? _graphState.link : link;
    const curLinks = _graphState ? _graphState.links : links;
    const connected = new Set();
    connected.add(d.id);
    curLinks.forEach(l => {
      const sid = l.source.id || l.source;
      const tid = l.target.id || l.target;
      if (sid === d.id) connected.add(tid);
      if (tid === d.id) connected.add(sid);
    });

    node.style('opacity', n => connected.has(n.id) ? 1 : 0.08);
    // Show labels for connected nodes
    node.selectAll('.node-label').style('opacity', n =>
      connected.has(n.id) ? 1 : 0
    );
    curLink.attr('stroke-opacity', l => {
      const sid = l.source.id || l.source;
      const tid = l.target.id || l.target;
      return (sid === d.id || tid === d.id) ? 0.7 : 0.01;
    }).attr('stroke-width', l => {
      const sid = l.source.id || l.source;
      const tid = l.target.id || l.target;
      return (sid === d.id || tid === d.id) ? 2.5 : defaultEdgeWidth;
    });
  }

  function resetHighlight() {
    const curLink = _graphState ? _graphState.link : link;
    node.style('opacity', 1);
    // Restore label visibility
    node.selectAll('.node-label').style('opacity', d =>
      (isLarge && d.tier === 'tier3') ? 0 : 1
    );
    curLink.attr('stroke-opacity', defaultEdgeOpacity).attr('stroke-width', defaultEdgeWidth);
  }

  // Curved link path generator
  function linkArc(d) {
    const dx = d.target.x - d.source.x;
    const dy = d.target.y - d.source.y;
    const dr = Math.sqrt(dx * dx + dy * dy) * 1.5;
    return `M${d.source.x},${d.source.y}A${dr},${dr} 0 0,1 ${d.target.x},${d.target.y}`;
  }

  // Tick
  simulation.on('tick', () => {
    link.attr('d', linkArc);
    node.attr('transform', d => `translate(${d.x},${d.y})`);
  });

  // Legend
  const legend = d3.select(container).append('div').attr('class', 'graph-legend');

  // Tier counts
  const tierIcons = { tier1: '★', tier2: '●', tier3: '·' };
  const tierColors = { tier1: '#f59e0b', tier2: '#3b82f6', tier3: '#6b7280' };
  ['tier1', 'tier2', 'tier3'].forEach(tier => {
    legend.append('div').attr('class', 'graph-legend-item')
      .html(`<span class="graph-legend-dot" style="background:${tierColors[tier]}">${tier === 'tier1' ? '★' : ''}</span>${tierLabel[tier]} (${tierCounts[tier]})`);
  });

  // Stance legend
  legend.append('div').attr('class', 'graph-legend-divider');
  Object.entries(stanceColors).forEach(([stance, color]) => {
    legend.append('div').attr('class', 'graph-legend-item')
      .html(`<span class="graph-legend-dot" style="background:${color}"></span>${stance}`);
  });

  legend.append('div').attr('class', 'graph-legend-item graph-legend-total')
    .html(`${data.nodes.length} agents · ${data.edges.length} connections`);

  // Auto-zoom to fit all nodes after layout settles
  let _autoZoomed = false;
  simulation.on('end', () => {
    if (_autoZoomed) return;
    _autoZoomed = true;
    const padding = 60;
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    nodes.forEach(n => {
      minX = Math.min(minX, n.x - n._r);
      maxX = Math.max(maxX, n.x + n._r);
      minY = Math.min(minY, n.y - n._r);
      maxY = Math.max(maxY, n.y + n._r);
    });
    const bw = maxX - minX + padding * 2;
    const bh = maxY - minY + padding * 2;
    const scale = Math.min(width / bw, height / bh, 0.9);
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    svg.transition().duration(800).call(
      zoomBehavior.transform,
      d3.zoomIdentity.translate(width / 2, height / 2).scale(scale).translate(-cx, -cy)
    );
  });

  // Save state for incremental updates
  const nodeMap = {};
  nodes.forEach(n => { nodeMap[n.id] = n; });
  _graphState = {
    container, svg, g, link, node, simulation, nodes, links, nodeMap,
    linkGroup, nodeGroup, zoomBehavior, tooltip,
    nodeCount: data.nodes.length,
    edgeCount: data.edges.length,
    colorMap, inDegree, outDegree,
    tierLabel, stanceColors, nodeRadius: nodeRadius,
    _selectedNode, highlightNode, resetHighlight, linkArc,
    defaultEdgeOpacity, defaultEdgeWidth, isLarge,
  };
}

// Incremental graph update — preserves positions, only updates edges + node stats
function _updateGraphData(data) {
  const gs = _graphState;
  if (!gs) return;

  // Update node data (post counts, follower counts, etc.)
  data.nodes.forEach(n => {
    const existing = gs.nodeMap[n.id];
    if (existing) {
      existing.post_count = n.post_count;
      existing.follower_count = n.follower_count;
      existing.following_count = n.following_count;
      existing.stance = n.stance;
      existing.sentiment = n.sentiment;
    }
  });

  // Check if edges changed
  if (data.edges.length === gs.edgeCount) return; // Nothing new

  // Recount degrees
  const inDegree = {}, outDegree = {};
  data.edges.forEach(e => {
    outDegree[e.source] = (outDegree[e.source] || 0) + 1;
    inDegree[e.target] = (inDegree[e.target] || 0) + 1;
  });
  gs.inDegree = inDegree;
  gs.outDegree = outDegree;

  // Build new links (mapping to existing node objects)
  const newLinks = data.edges.map(e => ({
    source: gs.nodeMap[e.source] || e.source,
    target: gs.nodeMap[e.target] || e.target,
  }));

  // Recalculate adaptive opacity for new edge count
  const newEdgeCount = data.edges.length;
  gs.defaultEdgeOpacity = newEdgeCount > 500 ? 0.06 : newEdgeCount > 200 ? 0.1 : 0.15;
  gs.defaultEdgeWidth = newEdgeCount > 500 ? 0.7 : newEdgeCount > 200 ? 0.9 : 1.2;

  // Update link selection
  gs.link = gs.linkGroup.selectAll('path')
    .data(newLinks)
    .join('path')
    .attr('fill', 'none')
    .attr('stroke', d => {
      const sid = d.source.id || d.source;
      return gs.colorMap[sid] || '#555';
    })
    .attr('stroke-opacity', gs.defaultEdgeOpacity)
    .attr('stroke-width', gs.defaultEdgeWidth);

  // Update simulation links
  gs.links.length = 0;
  gs.links.push(...newLinks);
  gs.simulation.force('link').links(newLinks);
  gs.simulation.alpha(0.1).restart();

  gs.edgeCount = data.edges.length;
  gs.link = gs.link; // reassign for highlight functions

  // Update legend total
  const legendTotal = gs.container.querySelector('.graph-legend-total');
  if (legendTotal) {
    legendTotal.innerHTML = `${data.nodes.length} agents · ${data.edges.length} connections`;
  }
}
