// Verdict sequence — cinematic reveal
import { store } from '../state/store.js';

export function initVerdict() {
  store.on('verdict', (data) => {
    if (!data) return;
    // Don't show verdict if launcher is visible (page just loaded or between trials)
    const launcher = document.getElementById('launcher-overlay');
    if (launcher && !launcher.classList.contains('hidden')) return;
    showVerdict(data);
  });
}

function showVerdict(data) {
  const overlay = document.getElementById('verdict-overlay');
  const textEl = document.getElementById('verdict-text');
  const voteEl = document.getElementById('verdict-vote');
  const statsEl = document.getElementById('verdict-stats');

  // Set content
  const isGuilty = data.result === 'guilty';
  textEl.textContent = isGuilty ? 'GUILTY' : 'NOT GUILTY';
  textEl.className = isGuilty ? 'guilty' : 'not-guilty';

  voteEl.textContent = `${data.guilty} — ${data.notGuilty}`;

  let statsHtml = '';
  if (data.unanimous) {
    statsHtml += '<div>Unanimous verdict</div>';
  }

  // Find turning points from transcript
  const transcript = store.get('transcript');
  const shifts = transcript
    .filter(t => t.type === 'jury_shift')
    .sort((a, b) => Math.abs(b.newConviction - b.oldConviction) - Math.abs(a.newConviction - a.oldConviction))
    .slice(0, 3);

  if (shifts.length > 0) {
    statsHtml += '<div style="margin-top:16px;font-size:13px">Key Turning Points:</div>';
    for (const s of shifts) {
      const delta = (s.newConviction - s.oldConviction).toFixed(2);
      statsHtml += `<div>R${s.round}: Juror #${s.seat} shifted ${delta > 0 ? '+' : ''}${delta}</div>`;
    }
  }

  statsEl.innerHTML = statsHtml;

  // Show with fade-in
  overlay.classList.remove('hidden');
  requestAnimationFrame(() => {
    overlay.classList.add('visible');
  });

  // Click to dismiss after 5s
  setTimeout(() => {
    overlay.addEventListener('click', () => {
      overlay.classList.remove('visible');
      setTimeout(() => overlay.classList.add('hidden'), 1000);
    }, { once: true });
  }, 5000);
}
