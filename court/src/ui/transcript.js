// Transcript feed — renders courtroom events as a live log
import { store } from '../state/store.js';

let feedEl = null;
let lastRound = 0;
let collapsed = false;

export function initTranscript() {
  feedEl = document.getElementById('transcript-feed');
  const toggle = document.getElementById('transcript-toggle');
  const container = document.getElementById('transcript-container');

  toggle.addEventListener('click', () => {
    collapsed = !collapsed;
    container.classList.toggle('collapsed', collapsed);
    toggle.innerHTML = collapsed ? '&#9650; Transcript' : '&#9660; Transcript';
  });

  store.on('transcript', (entry) => {
    appendEntry(entry);
  });
}

function appendEntry(entry) {
  if (!feedEl) return;

  // Round header
  if (entry.round > lastRound) {
    lastRound = entry.round;
    const phase = store.get('phaseLabel') || '';
    const header = document.createElement('div');
    header.className = 'transcript-round-header';
    header.textContent = `Round ${entry.round} — ${phase}`;
    feedEl.appendChild(header);
  }

  // Remove highlight from previous entry
  const prev = feedEl.querySelector('.transcript-entry.active');
  if (prev) prev.classList.remove('active');

  const div = document.createElement('div');
  div.className = 'transcript-entry active';

  switch (entry.type) {
    case 'argument':
      div.innerHTML = renderArgument(entry);
      break;
    case 'objection':
      div.innerHTML = renderObjection(entry);
      break;
    case 'jury_shift':
      div.innerHTML = renderJuryShift(entry);
      break;
    case 'witness_called':
      div.innerHTML = renderWitnessCalled(entry);
      break;
    case 'testimony':
      div.innerHTML = renderTestimony(entry);
      break;
    case 'deliberation':
      div.innerHTML = renderDeliberation(entry);
      break;
    case 'vote':
      div.innerHTML = renderVote(entry);
      break;
    default:
      div.textContent = JSON.stringify(entry);
  }

  feedEl.appendChild(div);

  // Auto-scroll: use scrollIntoView for precise positioning
  div.scrollIntoView({ behavior: 'smooth', block: 'nearest' });

  // Also expand transcript if collapsed
  const container = document.getElementById('transcript-container');
  if (container.classList.contains('collapsed')) {
    container.classList.remove('collapsed');
    const toggle = document.getElementById('transcript-toggle');
    toggle.innerHTML = '&#9660; Transcript';
  }
}

/**
 * Highlight and scroll to a specific transcript entry by content match.
 * Called by TTS onStartPlaying to sync transcript with audio.
 */
export function highlightTranscriptEntry(content) {
  if (!feedEl) return;

  // Remove previous active
  const prev = feedEl.querySelector('.transcript-entry.active');
  if (prev) prev.classList.remove('active');

  // Find the entry that matches this content (search from bottom)
  const entries = feedEl.querySelectorAll('.transcript-entry');
  for (let i = entries.length - 1; i >= 0; i--) {
    const text = entries[i].textContent || '';
    // Match first 50 chars of content
    if (content && text.includes(content.substring(0, 50))) {
      entries[i].classList.add('active');
      entries[i].scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      break;
    }
  }
}

function roleClass(role) {
  if (role === 'defense_attorney') return 'defense';
  return role || '';
}

function renderArgument(e) {
  let impact = '';
  if (e.juryImpact && e.juryImpact.length > 0) {
    const impacts = e.juryImpact
      .filter(i => Math.abs(i.delta) > 0.02)
      .map(i => `J${i.seat} ${i.delta > 0 ? '+' : ''}${i.delta.toFixed(2)}`)
      .join(', ');
    if (impacts) {
      impact = `<span class="transcript-impact">Jury: ${impacts}</span>`;
    }
  }
  return `<span class="transcript-speaker ${roleClass(e.speakerRole)}">${e.speakerName}:</span> "${e.content}"${impact}`;
}

function renderObjection(e) {
  const ruling = e.ruling === 'sustained'
    ? '<b style="color:#27ae60">SUSTAINED</b>'
    : '<b style="color:#e74c3c">OVERRULED</b>';
  return `<span class="transcript-speaker ${roleClass(e.byRole)}">${e.by}:</span> "Objection! ${e.grounds}." — ${ruling}`;
}

function renderJuryShift(e) {
  const dir = e.newConviction > e.oldConviction ? '&#9650;' : '&#9660;';
  const color = e.newConviction > e.oldConviction ? '#e74c3c' : '#3498db';
  return `<span class="transcript-impact" style="color:${color}">${dir} Juror #${e.seat}: ${e.oldConviction.toFixed(2)} → ${e.newConviction.toFixed(2)} (${e.cause})</span>`;
}

function renderWitnessCalled(e) {
  return `<span class="transcript-speaker witness">${e.calledBy} calls ${e.name}</span> (${e.witnessType}) to the stand.`;
}

function renderTestimony(e) {
  const emotion = e.emotionalImpact > 0.5 ? ' <span class="transcript-impact">[emotional]</span>' : '';
  return `<span class="transcript-speaker witness">${e.witnessName}:</span> "${e.content}"${emotion}`;
}

function renderDeliberation(e) {
  return `<span class="transcript-speaker juror">Juror #${e.seat} (${e.speakerName}):</span> "${e.content}"`;
}

function renderVote(e) {
  const color = e.verdict === 'guilty' ? '#e74c3c' : '#3498db';
  return `<span class="transcript-speaker juror">Juror #${e.seat} (${e.name}):</span> <b style="color:${color}">${e.verdict.toUpperCase()}</b>`;
}
