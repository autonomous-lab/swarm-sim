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

function getActionBadge(actionType) {
  const map = {
    create_post: '<span class="action-badge post">POST</span>',
    reply: '<span class="action-badge reply">REPLY</span>',
    like: '<span class="action-badge like">LIKE</span>',
    repost: '<span class="action-badge repost">REPOST</span>',
    follow: '<span class="action-badge follow">FOLLOW</span>',
    pin_memory: '<span class="action-badge pin">PIN</span>',
  };
  return map[actionType] || '';
}

function esc(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
