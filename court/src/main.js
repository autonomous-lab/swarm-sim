// AI Tribunal — Three.js courtroom simulation frontend
import * as THREE from 'three';
import { buildCourtroom } from './scene/scene.js';
import { populateCourt } from './scene/characters.js';
import { CameraController } from './scene/camera.js';
import { AnimationManager } from './scene/animations.js';
import { initLabelRenderer, renderLabels, createJurorLabel, createRoleLabel, updateJurorLabel, showSpeechBubble, hideSpeechBubble } from './ui/labels.js';
import { initHUD } from './ui/hud.js';
import { initTranscript } from './ui/transcript.js';
import { initSidebar } from './ui/sidebar.js';
import { initVerdict } from './ui/verdict.js';
import { initLauncher } from './ui/launcher.js';
import { initTTS, speak, narratorSpeak, isTTSEnabled, setOnStartPlaying, disableTTS } from './ui/tts.js';
import { store } from './state/store.js';
import { connectWebSocket } from './state/websocket.js';

// ─── Init Three.js ───

const container = document.getElementById('canvas-container');

const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = THREE.PCFSoftShadowMap;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 0.9;
container.appendChild(renderer.domElement);

const scene = new THREE.Scene();
scene.background = new THREE.Color(0x0d0d0f);

const camera = new THREE.PerspectiveCamera(45, window.innerWidth / window.innerHeight, 0.1, 100);

// ─── Build courtroom ───

const courtroom = buildCourtroom(scene);
const characters = populateCourt(scene, courtroom.positions);

// ─── Camera ───

const cameraCtrl = new CameraController(camera, renderer);

// ─── Labels (CSS2DRenderer) ───

const labelRenderer = initLabelRenderer(container);

// Create juror labels
for (let i = 0; i < characters.jurors.length; i++) {
  createJurorLabel(characters.jurors[i], i + 1);
}

// Create role labels
createRoleLabel(characters.judge, 'Judge', 'judge',
  '<div class="label-stat">Impartial</div>');
createRoleLabel(characters.prosecutor, 'Prosecutor', 'prosecutor',
  '<div class="label-stat">Persuasion: 0%</div>');
createRoleLabel(characters.defense, 'Defense', 'defense_attorney',
  '<div class="label-stat">Persuasion: 0%</div>');
createRoleLabel(characters.witness, 'Witness', 'witness',
  '<div class="label-stat">On the stand</div>');

// ─── Animations ───

const animManager = new AnimationManager(characters);

// Update role labels when participant names arrive
store.on('participantNames', (names) => {
  const updateLabel = (character, name) => {
    if (!name || !character) return;
    const d = character.userData.labelDiv;
    if (d) { const n = d.querySelector('.label-name'); if (n) n.textContent = name; }
  };
  updateLabel(characters.judge, names.judge);
  updateLabel(characters.prosecutor, names.prosecutor);
  updateLabel(characters.defense, names.defense);
  if (names.witness) {
    updateLabel(characters.witness, names.witness);
    characters.witness.visible = true;
  }
});

// ─── UI ───

initHUD();
initTranscript();
initSidebar();
initVerdict();
initLauncher();
initTTS();

// ─── Store → Scene bindings ───

store.on('jurors', (jurorsMap) => {
  animManager.updateJurorColors(jurorsMap);
  for (const [seat, data] of jurorsMap) {
    const idx = seat - 1;
    if (idx >= 0 && idx < characters.jurors.length) {
      updateJurorLabel(characters.jurors[idx], data);
    }
  }
});

// All characters for clearing bubbles
const allCharacters = [
  characters.judge, characters.prosecutor, characters.defense, characters.witness,
  ...characters.jurors
];

// Shared function: show bubble + camera for a given entry
function showEntryVisuals(entry) {
  const role = entry.speakerRole;
  let speaker = null;
  if (role === 'judge') speaker = characters.judge;
  else if (role === 'prosecutor') speaker = characters.prosecutor;
  else if (role === 'defense_attorney' || role === 'defense') speaker = characters.defense;
  else if (role === 'witness') speaker = characters.witness;
  else if (role === 'juror' && entry.seat) {
    const idx = entry.seat - 1;
    if (idx >= 0 && idx < characters.jurors.length) speaker = characters.jurors[idx];
  }

  if (speaker && entry.content) {
    for (const ch of allCharacters) {
      if (ch !== speaker) hideSpeechBubble(ch);
    }
    showSpeechBubble(speaker, entry.content);

    // Camera stays on overview — user controls camera manually

    if (role === 'witness') {
      characters.witness.visible = true;
      const wDiv = characters.witness.userData.labelDiv;
      if (wDiv && entry.speakerName) {
        const nameEl = wDiv.querySelector('.label-name');
        if (nameEl) nameEl.textContent = entry.speakerName;
      }
    }
  }
}

// TTS callback: when an audio STARTS playing, show bubble + scroll transcript
setOnStartPlaying((entry) => {
  showEntryVisuals(entry);
  if (entry.content) {
    // Scroll transcript to matching entry
    const feedEl = document.getElementById('transcript-feed');
    if (feedEl) {
      const prev = feedEl.querySelector('.transcript-entry.active');
      if (prev) prev.classList.remove('active');
      const entries = feedEl.querySelectorAll('.transcript-entry');
      for (let i = entries.length - 1; i >= 0; i--) {
        if (entries[i].textContent.includes(entry.content.substring(0, 50))) {
          entries[i].classList.add('active');
          entries[i].scrollIntoView({ behavior: 'smooth', block: 'nearest' });
          break;
        }
      }
    }
  }
});

store.on('transcript', (entry) => {
  const ttsOn = store.get('realtimeMode') && isTTSEnabled();
  if (ttsOn) {
    // TTS mode: queue audio. Bubble shown via onStartPlaying.
    // But ALWAYS show in transcript immediately (don't wait for TTS).
    speak(entry.content, entry.speakerRole, entry.speakerName, entry);
  } else {
    // Non-TTS: show bubble immediately
    showEntryVisuals(entry);
  }

  // Flash juror labels on impact
  if (entry.juryImpact) {
    for (const impact of entry.juryImpact) {
      const idx = impact.seat - 1;
      if (idx >= 0 && idx < characters.jurors.length) {
        const juror = characters.jurors[idx];
        const div = juror.userData.labelDiv;
        if (div) {
          const cls = impact.delta > 0 ? 'flash-guilty' : 'flash-innocent';
          div.classList.add(cls);
          setTimeout(() => div.classList.remove(cls), 1500);
        }
      }
    }
  }

  // Trigger animations
  switch (entry.type) {
    case 'argument':
      animManager.onArgument(entry.speakerRole, entry.juryImpact || []);
      break;
    case 'objection':
      animManager.onObjection(entry.byRole);
      break;
    case 'jury_shift':
      animManager.onJuryShift(entry.seat, entry.newConviction, entry.newConviction > entry.oldConviction);
      break;
    case 'witness_called':
      animManager.onWitnessCalled(entry.name);
      break;
    case 'testimony':
      animManager.onTestimony();
      break;
    case 'deliberation':
      animManager.onDeliberation(entry.seat);
      break;
  }
});

store.on('verdict', (data) => {
  if (data) {
    animManager.onVerdict(data.result);
    const vText = data.result === 'guilty'
      ? `The jury has reached a verdict. Guilty. ${data.guilty} to ${data.notGuilty}.`
      : `The jury has reached a verdict. Not guilty. ${data.notGuilty} to ${data.guilty}.`;
    narratorSpeak(vText);
  }
});

// Narrator for phase changes
let lastPhase = '';
store.on('phase', (phase) => {
  if (phase && phase !== lastPhase) {
    lastPhase = phase;
    const narrations = {
      'Opening Statements': 'The trial begins. Both sides will now present their opening statements to the jury.',
      'Prosecution Case': 'The prosecution now presents its case, calling witnesses and presenting evidence.',
      'Defense Case': 'The defense now presents its case, challenging the prosecution and calling its own witnesses.',
      'Rebuttal': 'The prosecution has the opportunity to rebut the defense arguments.',
      'Closing Arguments': 'Both sides now deliver their closing arguments to the jury.',
      'Jury Deliberation': 'The jury retires to deliberate. The fate of the defendant is now in their hands.',
    };
    const text = narrations[phase];
    if (text) {
      narratorSpeak(text);
    }
  }
});

// ─── Camera presets ───

document.querySelectorAll('#camera-presets button').forEach(btn => {
  btn.addEventListener('click', () => {
    const view = btn.dataset.view;
    cameraCtrl.setPreset(view);
    document.querySelectorAll('#camera-presets button').forEach(b => b.classList.remove('active'));
    btn.classList.add('active');
  });
});

// Keyboard shortcuts
document.addEventListener('keydown', (e) => {
  switch (e.key) {
    case '1': cameraCtrl.setPreset('overview'); break;
    case '2': cameraCtrl.setPreset('jury'); break;
    case '3': cameraCtrl.setPreset('witness'); break;
    case '4': cameraCtrl.setPreset('judge'); break;
    case '5': cameraCtrl.setPreset('cinematic'); break;
    case 'p': case 'P':
      fetch('/api/simulation/pause', { method: 'POST' });
      break;
    case 'r': case 'R':
      fetch('/api/simulation/resume', { method: 'POST' });
      break;
  }
});

// Pause/Resume buttons
const btnPause = document.getElementById('btn-pause');
const btnResume = document.getElementById('btn-resume');

btnPause.addEventListener('click', () => {
  fetch('/api/simulation/pause', { method: 'POST' }).then(() => {
    btnPause.classList.add('hidden');
    btnResume.classList.remove('hidden');
  });
});
btnResume.addEventListener('click', () => {
  console.log('[UI] Resume clicked, TTS enabled:', isTTSEnabled());
  // Disable TTS if active (otherwise it will re-pause immediately)
  try { disableTTS(); console.log('[UI] TTS disabled'); } catch(e) { console.warn('[UI] disableTTS failed:', e); }
  // Also force the store flag
  store.set('realtimeMode', false);
  // Update TTS button visually
  const ttsBtn = document.getElementById('btn-realtime');
  if (ttsBtn) { ttsBtn.classList.remove('rt-on'); ttsBtn.classList.add('rt-off'); }
  // Resume simulation
  fetch('/api/simulation/resume', { method: 'POST' }).then(() => {
    console.log('[UI] Resume sent');
    btnResume.classList.add('hidden');
    btnPause.classList.remove('hidden');
  });
});

// New Trial button
document.getElementById('btn-new-trial').addEventListener('click', () => {
  // Stop current trial
  fetch('/api/simulation/stop', { method: 'POST' });
  try { disableTTS(); } catch {}
  // Show launcher
  document.getElementById('launcher-overlay').classList.remove('hidden');
});

// Show/hide witness based on phase
store.on('phase', (phase) => {
  const p = (phase || '').toLowerCase();
  const witnessPhases = ['prosecution case', 'defense case', 'prosecution_case', 'defense_case'];
  if (witnessPhases.some(wp => p.includes(wp.replace(/ /g, '')) || p.includes(wp))) {
    characters.witness.visible = true;
  }
});

// Sync button state with server status
store.on('status', (status) => {
  if (status === 'paused') {
    btnPause.classList.add('hidden');
    btnResume.classList.remove('hidden');
  } else {
    btnResume.classList.add('hidden');
    btnPause.classList.remove('hidden');
  }
});

// ─── Resize handler ───

window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(window.innerWidth, window.innerHeight);
});

// ─── WebSocket ───

const wsProtocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
connectWebSocket(`${wsProtocol}//${location.host}/ws`);

// ─── Init demo jurors for visual preview ───

for (let i = 1; i <= 12; i++) {
  store.updateJuror(i, {
    conviction: 0,
    confidence: 0.2,
    name: `Juror #${i}`,
    trustProsecution: 0.5,
    trustDefense: 0.5,
    keyMoments: [],
    convictionHistory: [[0, 0, 0.2]],
  });
}

// ─── Render loop ───

const clock = new THREE.Clock();

function animate() {
  requestAnimationFrame(animate);

  const delta = clock.getDelta();

  cameraCtrl.update(delta);
  animManager.update(delta);

  renderer.render(scene, camera);
  renderLabels(camera, scene);
}

animate();

console.log('[AI Judgment] Courtroom initialized');
