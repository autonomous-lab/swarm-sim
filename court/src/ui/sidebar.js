// Sidebar panel — agent detail on click
import { store } from '../state/store.js';
import { convictionToCss } from '../utils/colors.js';

let sidebarEl = null;
let contentEl = null;

export function initSidebar() {
  sidebarEl = document.getElementById('sidebar');
  contentEl = document.getElementById('sidebar-content');
  const closeBtn = document.getElementById('sidebar-close');

  closeBtn.addEventListener('click', () => {
    sidebarEl.classList.add('hidden');
    store.set('selectedAgentId', null);
  });

  store.on('selectedAgentId', (id) => {
    if (id) {
      renderAgent(id);
      sidebarEl.classList.remove('hidden');
    } else {
      sidebarEl.classList.add('hidden');
    }
  });

  // Close on Escape
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      sidebarEl.classList.add('hidden');
      store.set('selectedAgentId', null);
    }
  });
}

function renderAgent(id) {
  if (!contentEl) return;

  // Check if it's a juror
  if (typeof id === 'string' && id.startsWith('juror-')) {
    const seat = parseInt(id.split('-')[1]);
    renderJuror(seat);
    return;
  }

  // Check roles
  switch (id) {
    case 'judge': renderJudge(); break;
    case 'prosecutor': renderAttorney('Prosecutor', 'prosecution'); break;
    case 'defense_attorney': renderAttorney('Defense Attorney', 'defense'); break;
    case 'witness': renderWitness(); break;
    default: contentEl.innerHTML = `<h2>Agent</h2><p>ID: ${id}</p>`;
  }
}

function renderJuror(seat) {
  const jurors = store.get('jurors');
  const data = jurors.get(seat) || {};
  const conviction = data.conviction || 0;
  const confidence = data.confidence || 0.2;
  const name = data.name || `Juror #${seat}`;

  let stanceLabel = 'Undecided';
  if (conviction > 0.6) stanceLabel = 'Strongly Guilty';
  else if (conviction > 0.2) stanceLabel = 'Leaning Guilty';
  else if (conviction < -0.6) stanceLabel = 'Strongly Innocent';
  else if (conviction < -0.2) stanceLabel = 'Leaning Innocent';

  const barPos = ((conviction + 1) / 2 * 100).toFixed(0);
  const barColor = convictionToCss(conviction);

  let keyMomentsHtml = '';
  if (data.keyMoments && data.keyMoments.length > 0) {
    keyMomentsHtml = '<h3 style="margin-top:16px;font-size:13px;color:#d4a843">Key Moments</h3>';
    for (const m of data.keyMoments.slice(0, 5)) {
      const dir = m.conviction_delta > 0 ? '+' : '';
      const color = m.conviction_delta > 0 ? '#e74c3c' : '#3498db';
      keyMomentsHtml += `
        <div style="padding:4px 0;border-bottom:1px solid #333338;font-size:12px;">
          <span style="color:${color}">${dir}${m.conviction_delta.toFixed(2)}</span>
          R${m.round}: "${m.content_summary}"
        </div>`;
    }
  }

  let historyHtml = '';
  if (data.convictionHistory && data.convictionHistory.length > 1) {
    historyHtml = '<h3 style="margin-top:16px;font-size:13px;color:#d4a843">Conviction History</h3>';
    historyHtml += '<div style="display:flex;gap:2px;align-items:flex-end;height:40px;">';
    for (const [round, conv] of data.convictionHistory) {
      const h = Math.abs(conv) * 40;
      const c = convictionToCss(conv);
      historyHtml += `<div style="width:4px;height:${Math.max(2, h)}px;background:${c};border-radius:1px;" title="R${round}: ${conv.toFixed(2)}"></div>`;
    }
    historyHtml += '</div>';
  }

  contentEl.innerHTML = `
    <h2>${name}</h2>
    <div class="stat-row"><span class="stat-label">Seat</span><span class="stat-value">#${seat}</span></div>
    <div class="stat-row"><span class="stat-label">Position</span><span class="stat-value" style="color:${barColor}">${stanceLabel}</span></div>
    <div class="stat-row"><span class="stat-label">Conviction</span><span class="stat-value">${conviction.toFixed(2)}</span></div>
    <div class="stat-row"><span class="stat-label">Confidence</span><span class="stat-value">${(confidence * 100).toFixed(0)}%</span></div>
    <div class="conviction-bar">
      <div class="conviction-marker" style="left:${barPos}%;background:${barColor}"></div>
    </div>
    <div style="display:flex;justify-content:space-between;font-size:10px;color:#888890">
      <span>Innocent</span><span>Guilty</span>
    </div>
    <div class="stat-row"><span class="stat-label">Trust Prosecution</span><span class="stat-value">${((data.trustProsecution || 0.5) * 100).toFixed(0)}%</span></div>
    <div class="stat-row"><span class="stat-label">Trust Defense</span><span class="stat-value">${((data.trustDefense || 0.5) * 100).toFixed(0)}%</span></div>
    ${keyMomentsHtml}
    ${historyHtml}
  `;
}

function renderJudge() {
  const sustained = store.get('objectionsSustained');
  const overruled = store.get('objectionsOverruled');
  const total = sustained + overruled;
  const balance = total > 0 ? ((sustained / total) * 100).toFixed(0) : '—';

  const transcript = store.get('transcript') || [];
  const statements = transcript.filter(e => e.speakerRole === 'judge');

  let historyHtml = '';
  if (statements.length > 0) {
    historyHtml = '<h3 style="margin-top:16px;font-size:13px;color:#d4a843;border-bottom:1px solid #333338;padding-bottom:4px">Rulings & Statements</h3>';
    for (const s of [...statements].reverse()) {
      historyHtml += `
        <div style="padding:8px 0;border-bottom:1px solid rgba(51,51,56,0.4);font-size:12px;line-height:1.5">
          <span style="color:#888890;font-size:10px">Round ${s.round}</span>
          <div style="color:#e8e8ec">"${s.content.length > 200 ? s.content.slice(0, 197) + '...' : s.content}"</div>
        </div>`;
    }
  }

  contentEl.innerHTML = `
    <h2 style="color:#d4a843">Judge</h2>
    <div class="stat-row"><span class="stat-label">Objections Sustained</span><span class="stat-value">${sustained}</span></div>
    <div class="stat-row"><span class="stat-label">Objections Overruled</span><span class="stat-value">${overruled}</span></div>
    <div class="stat-row"><span class="stat-label">Sustain Rate</span><span class="stat-value">${balance}%</span></div>
    <div class="stat-row"><span class="stat-label">Total Statements</span><span class="stat-value">${statements.length}</span></div>
    ${historyHtml}
  `;
}

function renderAttorney(name, party) {
  const color = party === 'prosecution' ? '#e74c3c' : '#3498db';
  const roleMatch = party === 'prosecution' ? 'prosecutor' : 'defense_attorney';
  const jurors = store.get('jurors');
  let leaning = 0;
  for (const j of jurors.values()) {
    if (party === 'prosecution' && j.conviction > 0.2) leaning++;
    if (party === 'defense' && j.conviction < -0.2) leaning++;
  }
  const total = jurors.size || 12;

  // Get all statements from transcript
  const transcript = store.get('transcript') || [];
  const statements = transcript.filter(e =>
    e.type === 'argument' && (e.speakerRole === roleMatch || e.speakerRole === party)
  );

  // Compute total jury impact
  let totalPositive = 0, totalNegative = 0;
  for (const s of statements) {
    for (const imp of (s.juryImpact || [])) {
      if (imp.delta > 0) totalPositive += imp.delta;
      else totalNegative += Math.abs(imp.delta);
    }
  }

  let historyHtml = '';
  if (statements.length > 0) {
    historyHtml = '<h3 style="margin-top:16px;font-size:13px;color:#d4a843;border-bottom:1px solid #333338;padding-bottom:4px">Arguments History</h3>';
    // Show all statements, most recent first
    for (const s of [...statements].reverse()) {
      const impactCount = (s.juryImpact || []).length;
      const avgDelta = impactCount > 0
        ? (s.juryImpact.reduce((sum, i) => sum + i.delta, 0) / impactCount)
        : 0;
      const impactColor = avgDelta > 0 ? '#e74c3c' : avgDelta < 0 ? '#3498db' : '#888890';
      const impactArrow = avgDelta > 0.01 ? '&#9650;' : avgDelta < -0.01 ? '&#9660;' : '&#9644;';
      const impactLabel = avgDelta !== 0 ? `<span style="color:${impactColor}">${impactArrow} ${avgDelta > 0 ? '+' : ''}${avgDelta.toFixed(3)}</span>` : '';

      historyHtml += `
        <div style="padding:8px 0;border-bottom:1px solid rgba(51,51,56,0.4);font-size:12px;line-height:1.5">
          <div style="display:flex;justify-content:space-between;margin-bottom:2px">
            <span style="color:#888890;font-size:10px">Round ${s.round}</span>
            ${impactLabel}
          </div>
          <div style="color:#e8e8ec">"${s.content.length > 200 ? s.content.slice(0, 197) + '...' : s.content}"</div>
        </div>`;
    }
  }

  contentEl.innerHTML = `
    <h2 style="color:${color}">${name}</h2>
    <div class="stat-row"><span class="stat-label">Jurors Leaning Their Way</span><span class="stat-value">${leaning}/${total}</span></div>
    <div class="stat-row"><span class="stat-label">Persuasion Rate</span><span class="stat-value">${(leaning / total * 100).toFixed(0)}%</span></div>
    <div class="stat-row"><span class="stat-label">Total Statements</span><span class="stat-value">${statements.length}</span></div>
    <div class="stat-row"><span class="stat-label">Cumulative Impact</span><span class="stat-value" style="color:#27ae60">+${totalPositive.toFixed(2)}</span> / <span class="stat-value" style="color:#e74c3c">-${totalNegative.toFixed(2)}</span></div>
    ${historyHtml}
  `;
}

function renderWitness() {
  const transcript = store.get('transcript') || [];
  const statements = transcript.filter(e => e.speakerRole === 'witness');

  // Current witness = last one who spoke
  const currentWitness = statements.length > 0 ? statements[statements.length - 1].speakerName : null;

  if (!currentWitness) {
    contentEl.innerHTML = `<h2 style="color:#cccccc">Witness Stand</h2><p style="color:#888890;font-size:13px">No witness currently on the stand.</p>`;
    return;
  }

  // Only show THIS witness's statements
  const thisWitnessStatements = statements.filter(s => s.speakerName === currentWitness);

  let historyHtml = '';
  if (thisWitnessStatements.length > 0) {
    historyHtml = '<h3 style="margin-top:16px;font-size:13px;color:#d4a843;border-bottom:1px solid #333338;padding-bottom:4px">Testimony</h3>';
    for (const s of [...thisWitnessStatements].reverse()) {
      const impactCount = (s.juryImpact || []).length;
      const avgDelta = impactCount > 0
        ? (s.juryImpact.reduce((sum, i) => sum + i.delta, 0) / impactCount)
        : 0;
      const impactColor = avgDelta > 0 ? '#e74c3c' : avgDelta < 0 ? '#3498db' : '#888890';
      const impactLabel = impactCount > 0
        ? `<span style="color:${impactColor}">${avgDelta > 0 ? '+' : ''}${avgDelta.toFixed(3)}</span>`
        : '';

      historyHtml += `
        <div style="padding:8px 0;border-bottom:1px solid rgba(51,51,56,0.4);font-size:12px;line-height:1.5">
          <div style="display:flex;justify-content:space-between;margin-bottom:2px">
            <span style="color:#888890;font-size:10px">Round ${s.round}</span>
            ${impactLabel}
          </div>
          <div style="color:#e8e8ec">"${s.content.length > 200 ? s.content.slice(0, 197) + '...' : s.content}"</div>
        </div>`;
    }
  }

  // Count all witnesses who have testified
  const allWitnesses = [...new Set(statements.map(s => s.speakerName))];

  contentEl.innerHTML = `
    <h2 style="color:#cccccc">${currentWitness}</h2>
    <div class="stat-row"><span class="stat-label">Status</span><span class="stat-value" style="color:#27ae60">On the stand</span></div>
    <div class="stat-row"><span class="stat-label">Statements</span><span class="stat-value">${thisWitnessStatements.length}</span></div>
    <div class="stat-row"><span class="stat-label">Total Witnesses Called</span><span class="stat-value">${allWitnesses.length}</span></div>
    ${historyHtml}
  `;
}
