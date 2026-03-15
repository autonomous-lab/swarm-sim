// ================================================================
// Swarm-Sim UI Components
// ================================================================

function renderAgentCard(agent) {
  return `
    <div class="agent-card" onclick="showAgent('${agent.id}')" data-tier="${agent.tier}">
      <div class="agent-tier-dot ${agent.tier}"></div>
      <div class="agent-info">
        <div class="agent-username">@${esc(agent.username)}</div>
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

function renderAgentDetail(data) {
  const { profile, state, recent_posts } = data;
  const tierLabel = { tier1: 'VIP', tier2: 'Standard', tier3: 'Figurant' }[profile.tier] || '';

  let memoryHtml = '';
  if (state && state.memory) {
    if (state.memory.pinned.length > 0) {
      memoryHtml += state.memory.pinned.map(m =>
        `<div class="memory-item pinned">${esc(m)}</div>`
      ).join('');
    }
    if (state.memory.recent.length > 0) {
      memoryHtml += state.memory.recent.slice(-10).map(([round, obs]) =>
        `<div class="memory-item">[R${round}] ${esc(obs)}</div>`
      ).join('');
    }
  }

  let postsHtml = '';
  if (recent_posts && recent_posts.length > 0) {
    postsHtml = recent_posts.slice(-10).map(p =>
      `<div class="post-card" style="margin-bottom:4px">
        <div class="post-content">${esc(p.content)}</div>
        <div class="post-actions-bar">
          <span class="post-action-item">${p.likes.length} likes</span>
          <span class="post-action-item">${p.replies.length} replies</span>
          <span class="post-time">R${p.created_at_round}</span>
        </div>
      </div>`
    ).join('');
  }

  return `
    <div class="agent-detail-header">
      <div class="agent-detail-name">${esc(profile.name)}</div>
      <div class="agent-detail-username">@${esc(profile.username)} &middot; ${tierLabel}</div>
    </div>
    <div class="agent-detail-section">
      <h4>Bio</h4>
      <p>${esc(profile.bio)}</p>
    </div>
    <div class="agent-detail-section">
      <h4>Persona</h4>
      <p>${esc(profile.persona)}</p>
    </div>
    <div class="agent-detail-section">
      <h4>Stance: ${profile.stance} | Sentiment: ${profile.sentiment_bias.toFixed(1)}</h4>
    </div>
    ${state ? `
    <div class="agent-detail-section">
      <h4>Stats</h4>
      <p>${state.post_ids.length} posts &middot; ${state.followers.length} followers &middot; ${state.following.length} following</p>
    </div>` : ''}
    ${memoryHtml ? `
    <div class="agent-detail-section">
      <h4>Memory</h4>
      ${memoryHtml}
    </div>` : ''}
    ${postsHtml ? `
    <div class="agent-detail-section">
      <h4>Recent Posts</h4>
      ${postsHtml}
    </div>` : ''}
  `;
}

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
  };
  return map[actionType] || '';
}

function esc(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
