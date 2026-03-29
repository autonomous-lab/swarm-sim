// CSS2D labels above character heads
import { CSS2DRenderer, CSS2DObject } from 'three/addons/renderers/CSS2DRenderer.js';
import { store } from '../state/store.js';
import { convictionToCss } from '../utils/colors.js';

let labelRenderer = null;

export function initLabelRenderer(container) {
  labelRenderer = new CSS2DRenderer();
  labelRenderer.setSize(window.innerWidth, window.innerHeight);
  labelRenderer.domElement.style.position = 'absolute';
  labelRenderer.domElement.style.top = '0';
  labelRenderer.domElement.style.pointerEvents = 'none';
  container.appendChild(labelRenderer.domElement);

  window.addEventListener('resize', () => {
    labelRenderer.setSize(window.innerWidth, window.innerHeight);
  });

  return labelRenderer;
}

export function renderLabels(camera, scene) {
  if (labelRenderer) {
    labelRenderer.render(scene, camera);
  }
}

/**
 * Create a floating label for a juror.
 */
export function createJurorLabel(character, seat) {
  const div = document.createElement('div');
  div.className = 'agent-label';
  div.innerHTML = `
    <div class="label-name">Juror #${seat}</div>
    <div class="label-role">Undecided</div>
    <div class="label-gauge">
      <div class="label-gauge-fill" style="width:50%;background:#555560;"></div>
    </div>
    <div class="label-stat">Conf: 20% | Att: 100%</div>
  `;
  div.style.pointerEvents = 'auto';

  const label = new CSS2DObject(div);
  label.position.set(0, 2.2, 0);
  character.add(label);

  // Store reference for updates
  character.userData.labelDiv = div;
  character.userData.label = label;

  // Click handler
  div.addEventListener('click', () => {
    store.set('selectedAgentId', character.userData.agentId || `juror-${seat}`);
  });

  return label;
}

/**
 * Create a label for judge/attorney.
 */
export function createRoleLabel(character, name, role, extraHtml = '') {
  const roleColors = {
    judge: '#d4a843',
    prosecutor: '#e74c3c',
    defense_attorney: '#3498db',
    witness: '#cccccc',
  };

  const div = document.createElement('div');
  div.className = 'agent-label';
  div.innerHTML = `
    <div class="label-name" style="color:${roleColors[role] || '#fff'}">${name}</div>
    <div class="label-role">${role.replace(/_/g, ' ').toUpperCase()}</div>
    ${extraHtml}
  `;
  div.style.pointerEvents = 'auto';

  const label = new CSS2DObject(div);
  label.position.set(0, 2.2, 0);
  character.add(label);

  character.userData.labelDiv = div;
  character.userData.label = label;

  div.addEventListener('click', () => {
    store.set('selectedAgentId', character.userData.agentId || role);
  });

  return label;
}

/**
 * Show a speech bubble above a character.
 * Stays visible until replaced by the next bubble (no auto-hide).
 */
export function showSpeechBubble(character, text) {
  // Remove existing bubble if any
  hideSpeechBubble(character);

  const div = document.createElement('div');
  div.className = 'speech-bubble';
  div.textContent = text.length > 150 ? text.slice(0, 147) + '...' : text;
  div.style.pointerEvents = 'none';

  const bubble = new CSS2DObject(div);
  bubble.position.set(0, 2.8, 0);
  character.add(bubble);
  character.userData.speechBubble = bubble;
  character.userData.speechDiv = div;

  // Animate in
  requestAnimationFrame(() => div.classList.add('visible'));
}

/**
 * Hide speech bubble from a character.
 */
export function hideSpeechBubble(character) {
  if (character.userData.speechBubble) {
    character.remove(character.userData.speechBubble);
    character.userData.speechBubble = null;
    character.userData.speechDiv = null;
  }
}

/**
 * Update a juror's label — regenerates full innerHTML every time
 * to avoid stale DOM references.
 */
export function updateJurorLabel(character, jurorData) {
  const div = character.userData.labelDiv;
  if (!div) return;

  const conviction = jurorData.conviction ?? 0;
  const confidence = jurorData.confidence ?? 0.2;
  const seat = character.userData.seat || 0;
  const name = jurorData.name || `Juror #${seat}`;

  let stanceLabel = 'Undecided';
  let stanceColor = '#888890';
  if (conviction > 0.6) { stanceLabel = 'GUILTY'; stanceColor = '#e74c3c'; }
  else if (conviction > 0.2) { stanceLabel = 'Leaning Guilty'; stanceColor = '#e74c3c'; }
  else if (conviction < -0.6) { stanceLabel = 'NOT GUILTY'; stanceColor = '#3498db'; }
  else if (conviction < -0.2) { stanceLabel = 'Leaning Innocent'; stanceColor = '#3498db'; }

  const gaugeWidth = ((conviction + 1) / 2 * 100).toFixed(0);
  const gaugeColor = convictionToCss(conviction);
  const confPct = (confidence * 100).toFixed(0);
  const convStr = conviction >= 0 ? `+${conviction.toFixed(2)}` : conviction.toFixed(2);

  div.innerHTML = `
    <div class="label-name">${name}</div>
    <div class="label-role" style="color:${stanceColor};font-weight:600">${stanceLabel}</div>
    <div class="label-gauge">
      <div class="label-gauge-fill" style="width:${gaugeWidth}%;background-color:${gaugeColor}"></div>
    </div>
    <div class="label-stat">${convStr} | Conf: ${confPct}%</div>
  `;
}
