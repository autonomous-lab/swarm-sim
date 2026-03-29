// TTS — Gemini Live API via backend WS proxy
// Streams PCM chunks and plays them immediately via AudioContext.
// Next speaker waits until the last chunk's onended fires.
import { store } from '../state/store.js';

const MALE_VOICES = ['Charon', 'Fenrir', 'Orus', 'Puck', 'Enceladus', 'Iapetus', 'Umbriel', 'Algenib'];
const FEMALE_VOICES = ['Kore', 'Leda', 'Aoede', 'Zephyr', 'Callirrhoe', 'Autonoe', 'Despina', 'Erinome'];
const FEMALE_NAMES = new Set([
  'sarah','emily','jessica','jennifer','amanda','ashley','mary','patricia','linda',
  'elizabeth','sophia','isabella','olivia','emma','ava','mia','grace','priya','aisha',
  'maria','carmen','elena','anna','lisa','karen','nancy','betty','sandra','donna',
  'carol','ruth','sharon','michelle','laura','kimberly','deborah','dorothy','helen',
  'samantha','katherine','christine','stephanie','rebecca','rachel','andrea','susan',
  'victoria','natalia','alicia','diana','eva','rosa','lucia','sofia','mei','yuki','anya',
]);

function guessGender(name) {
  if (!name) return 'male';
  const first = name.toLowerCase().split(/[\s.]+/).find(p => p.length > 2) || '';
  if (FEMALE_NAMES.has(first)) return 'female';
  if (name.toLowerCase().startsWith('ms.') || name.toLowerCase().startsWith('mrs.')) return 'female';
  return 'male';
}

const voiceCache = new Map();
let maleIdx = 0, femaleIdx = 0;
function getVoice(role, name) {
  if (role === 'narrator') return 'Sadachbia';
  const key = name || role;
  if (voiceCache.has(key)) return voiceCache.get(key);
  const gender = guessGender(name);
  const voice = gender === 'female'
    ? FEMALE_VOICES[femaleIdx++ % FEMALE_VOICES.length]
    : MALE_VOICES[maleIdx++ % MALE_VOICES.length];
  voiceCache.set(key, voice);
  return voice;
}

const SAMPLE_RATE = 24000;
let enabled = false;
let speechQueue = [];
let isPlaying = false;
let simulationPaused = false;
let audioCtx = null;
let onStartPlaying = null;

export function initTTS() {
  const btn = document.getElementById('btn-realtime');
  btn.addEventListener('click', () => {
    enabled = !enabled;
    btn.classList.toggle('rt-on', enabled);
    btn.classList.toggle('rt-off', !enabled);
    store.set('realtimeMode', enabled);
    if (enabled && !audioCtx) audioCtx = new AudioContext({ sampleRate: SAMPLE_RATE });
    if (!enabled) {
      speechQueue = [];
      isPlaying = false;
      if (simulationPaused) {
        fetch('/api/simulation/resume', { method: 'POST' });
        simulationPaused = false;
      }
    }
  });
  store.on('realtimeMode', (val) => {
    enabled = val;
    btn.classList.toggle('rt-on', enabled);
    btn.classList.toggle('rt-off', !enabled);
  });
}

export function setOnStartPlaying(cb) { onStartPlaying = cb; }

export function speak(text, role, speakerName, entry) {
  if (!enabled || !text || text.length < 10) return;
  speechQueue.push({
    text: text.length > 400 ? text.slice(0, 397) + '...' : text,
    voice: getVoice(role, speakerName),
    entry,
  });
  if (!simulationPaused) {
    fetch('/api/simulation/pause', { method: 'POST' });
    simulationPaused = true;
  }
  if (!isPlaying) playNext();
}

export function narratorSpeak(text) {
  speak(text, 'narrator', 'Narrator', {
    speakerRole: 'narrator', speakerName: 'Narrator', content: text,
  });
}

async function playNext() {
  if (speechQueue.length === 0) {
    isPlaying = false;
    if (simulationPaused && enabled) {
      await sleep(500);
      fetch('/api/simulation/resume', { method: 'POST' });
      simulationPaused = false;
    }
    return;
  }

  isPlaying = true;
  const item = speechQueue.shift();

  if (onStartPlaying && item.entry) onStartPlaying(item.entry);

  if (!audioCtx) audioCtx = new AudioContext({ sampleRate: SAMPLE_RATE });
  if (audioCtx.state === 'suspended') await audioCtx.resume();

  try {
    await streamAndPlay(item.text, item.voice);
  } catch (e) {
    console.warn('[TTS] Error:', e);
  }

  await sleep(300);
  playNext();
}

/**
 * Stream TTS: play chunks as they arrive (instant start),
 * resolve only when the LAST chunk has finished playing.
 */
function streamAndPlay(text, voice) {
  return new Promise((resolve) => {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${proto}//${location.host}/ws/tts`);
    ws.binaryType = 'arraybuffer';

    let scheduledEnd = audioCtx.currentTime;
    let chunkCount = 0;
    let wsDone = false;
    let resolved = false;

    ws.onopen = () => {
      ws.send(JSON.stringify({ text, voice }));
    };

    ws.onmessage = (event) => {
      if (typeof event.data === 'string') {
        ws.close();
        return;
      }

      const pcmBytes = new Uint8Array(event.data);
      if (pcmBytes.length < 20) return;

      const samples = new Float32Array(pcmBytes.length / 2);
      const view = new DataView(event.data);
      for (let i = 0; i < samples.length; i++) {
        samples[i] = view.getInt16(i * 2, true) / 32768;
      }

      const buf = audioCtx.createBuffer(1, samples.length, SAMPLE_RATE);
      buf.getChannelData(0).set(samples);
      const src = audioCtx.createBufferSource();
      src.buffer = buf;
      src.connect(audioCtx.destination);

      // Always schedule AFTER the previous chunk — never overlap
      const startTime = Math.max(scheduledEnd, audioCtx.currentTime);
      src.start(startTime);
      scheduledEnd = startTime + buf.duration;
      chunkCount++;
    };

    const waitForAudioAndResolve = () => {
      if (resolved) return;
      resolved = true;

      if (chunkCount === 0) {
        resolve();
        return;
      }

      // Wait until all scheduled audio has finished playing
      // Use a poll instead of onended (onended is unreliable for chained buffers)
      const check = () => {
        if (audioCtx.currentTime >= scheduledEnd - 0.05) {
          resolve();
        } else {
          setTimeout(check, 100);
        }
      };
      check();
    };

    ws.onclose = () => {
      wsDone = true;
      waitForAudioAndResolve();
    };

    ws.onerror = () => {
      wsDone = true;
      waitForAudioAndResolve();
    };

    // Hard timeout
    setTimeout(() => {
      try { ws.close(); } catch {}
      if (!resolved) { resolved = true; resolve(); }
    }, 25000);
  });
}

function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }
export function isTTSEnabled() { return enabled; }
export function disableTTS() {
  enabled = false;
  speechQueue = [];
  isPlaying = false;
  simulationPaused = false;
  const btn = document.getElementById('btn-realtime');
  if (btn) { btn.classList.remove('rt-on'); btn.classList.add('rt-off'); }
  store.set('realtimeMode', false);
}
export function skipCurrent() {}
