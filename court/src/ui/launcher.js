// Launcher UI — case selection, presets, trial launch
import { store } from '../state/store.js';

const PRESET_CASES = [
  {
    title: "Tech Fraud",
    prompt: "The People v. Marcus Chen, former CTO of QuantumLeap AI, accused of defrauding investors of $47 million by fabricating AI benchmark results. Key evidence: Slack messages saying 'just make the numbers work', whistleblower testimony, audit showing 340% performance inflation."
  },
  {
    title: "Medical Malpractice",
    prompt: "The People v. Dr. Elena Vasquez, neurosurgeon, charged with involuntary manslaughter after a patient died during an experimental brain surgery. The prosecution argues she ignored standard protocols. The defense claims she was attempting a life-saving procedure with the patient's informed consent."
  },
  {
    title: "Corporate Espionage",
    prompt: "The People v. James Whitfield, senior engineer at NovaTech, accused of stealing proprietary chip designs worth $200M and selling them to a Chinese competitor. Evidence includes encrypted file transfers, suspicious travel patterns, and a $2M deposit in an offshore account."
  },
  {
    title: "Environmental Crime",
    prompt: "The People v. Pinnacle Chemical Corp, charged with illegally dumping toxic waste into the Cedar River, causing a cancer cluster in the town of Millbrook. 47 residents diagnosed. The company claims the contamination predates their operations."
  },
  {
    title: "Self-Defense or Murder",
    prompt: "The People v. Sarah Mitchell, a domestic abuse survivor who shot and killed her husband David Mitchell. She claims self-defense after years of documented abuse. The prosecution argues she had time to leave and the shooting was premeditated."
  },
  {
    title: "AI Liability",
    prompt: "The People v. AutoDrive Inc, whose self-driving car killed a pedestrian in San Francisco. The prosecution argues the company knowingly deployed software with a known sensor blind spot. The defense claims the pedestrian jaywalked into an unavoidable situation."
  },
  {
    title: "Insider Trading",
    prompt: "The People v. Richard Huang, hedge fund manager, accused of insider trading that netted $83 million in profits. Key evidence: phone records with a pharmaceutical CEO days before a failed drug trial was made public, and suspicious trading patterns across 12 accounts."
  },
  {
    title: "Police Misconduct",
    prompt: "The People v. Officer Derek Rawlings, charged with excessive force and civil rights violations after beating an unarmed suspect during an arrest. Body cam footage is partially corrupted. Three witnesses give conflicting accounts."
  },
  {
    title: "Art Forgery Empire",
    prompt: "The People v. Isabella Romano, art dealer accused of running a decade-long forgery operation that sold $150M worth of fake masterpieces to museums and collectors worldwide. She claims she was deceived by her suppliers and had no knowledge the works were forgeries."
  },
  {
    title: "Crypto Ponzi Scheme",
    prompt: "The People v. Tyler Graves, founder of LunaYield crypto platform, charged with operating a $600M Ponzi scheme disguised as a DeFi yield protocol. 14,000 investors lost their savings. Graves claims the protocol failed due to market conditions, not fraud."
  },
];

export function initLauncher() {
  const overlay = document.getElementById('launcher-overlay');
  const caseInput = document.getElementById('case-input');
  const btnLaunch = document.getElementById('btn-launch');
  const btnRandomize = document.getElementById('btn-randomize');
  const presetList = document.getElementById('preset-list');
  const realtimeToggle = document.getElementById('realtime-toggle');
  const pastTrialsList = document.getElementById('past-trials-list');

  // Check URL for trial replay: /court/?trial=<id>
  const params = new URLSearchParams(window.location.search);
  const replayId = params.get('trial');
  if (replayId) {
    overlay.classList.add('hidden');
    loadAndReplayTrial(replayId);
  }

  // Load past trials
  loadPastTrials(pastTrialsList);

  // Populate preset buttons
  for (const preset of PRESET_CASES) {
    const btn = document.createElement('button');
    btn.className = 'preset-btn';
    btn.textContent = preset.title;
    btn.addEventListener('click', () => {
      caseInput.value = preset.prompt;
    });
    presetList.appendChild(btn);
  }

  // Randomize button
  btnRandomize.addEventListener('click', () => {
    const random = PRESET_CASES[Math.floor(Math.random() * PRESET_CASES.length)];
    caseInput.value = random.prompt;
  });

  // Launch button
  btnLaunch.addEventListener('click', async () => {
    const scenario = caseInput.value.trim();
    if (!scenario) return;

    btnLaunch.disabled = true;
    btnLaunch.textContent = 'Launching...';

    // Store real-time preference
    store.set('realtimeMode', realtimeToggle.checked);

    try {
      const res = await fetch('/api/simulation/launch', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          scenario_prompt: scenario,
          mode: 'trial',
          target_agent_count: 20,
        }),
      });

      if (res.ok) {
        overlay.classList.add('hidden');

        // Reset ALL state from previous trial
        const verdictOverlay = document.getElementById('verdict-overlay');
        verdictOverlay.classList.add('hidden');
        verdictOverlay.classList.remove('visible');
        store.set('verdict', null);
        store.set('round', 0);
        store.set('phase', '');
        store.set('phaseLabel', '');
        store.set('momentum', 0);
        store.set('objectionsSustained', 0);
        store.set('objectionsOverruled', 0);
        store.set('apiCost', 0);
        store.set('guiltyCount', 0);
        store.set('undecidedCount', 12);
        store.set('innocentCount', 0);
        store._notify('jurySplit', { guilty: 0, undecided: 12, innocent: 0 });
        // Clear transcript
        store.state.transcript = [];
        if (store._transcriptKeys) store._transcriptKeys.clear();
        const feedEl = document.getElementById('transcript-feed');
        if (feedEl) feedEl.innerHTML = '';
        // Reset jurors
        store.state.jurors = new Map();
        for (let i = 1; i <= 12; i++) {
          store.updateJuror(i, { conviction: 0, confidence: 0.2, name: 'Juror #' + i });
        }

        // Show loading screen
        const loadingOverlay = document.getElementById('loading-overlay');
        loadingOverlay.classList.remove('hidden');
        document.getElementById('loading-title').textContent = 'Preparing the courtroom...';
        document.getElementById('loading-detail').textContent = 'Generating court participants and case details';
        store.set('status', 'preparing');

        // Poll until agents are ready
        const pollReady = setInterval(async () => {
          try {
            const s = await (await fetch('/api/status')).json();
            if (s.total_agents > 0) {
              document.getElementById('loading-title').textContent = 'Trial starting...';
              document.getElementById('loading-detail').textContent = `${s.total_agents} participants ready`;
            }
            if (s.status === 'running') {
              clearInterval(pollReady);
              loadingOverlay.classList.add('hidden');
            }
          } catch {}
        }, 1500);
      } else {
        const err = await res.text();
        alert('Launch failed: ' + err);
        btnLaunch.disabled = false;
        btnLaunch.textContent = 'Start Trial';
      }
    } catch (e) {
      alert('Launch failed: ' + e.message);
      btnLaunch.disabled = false;
      btnLaunch.textContent = 'Start Trial';
    }
  });

  // Hide launcher when running, show when finished
  store.on('status', (status) => {
    if (status === 'running' || status === 'paused' || status === 'preparing') {
      overlay.classList.add('hidden');
    } else if (status === 'finished' || status === 'idle') {
      // Re-enable launch button for next trial
      btnLaunch.disabled = false;
      btnLaunch.textContent = 'Start Trial';
    }
  });

  // Show launcher again after verdict
  store.on('verdict', (data) => {
    if (data) {
      setTimeout(() => {
        overlay.classList.remove('hidden');
        btnLaunch.disabled = false;
        btnLaunch.textContent = 'Start New Trial';
        loadPastTrials(pastTrialsList); // refresh list
      }, 8000);
    }
  });

  // Check on load
  fetch('/api/status').then(r => r.json()).then(d => {
    if (d.status === 'running' || d.status === 'paused') {
      overlay.classList.add('hidden');
    }
  }).catch(() => {});
}

// ─── Past trials ───

async function loadPastTrials(container) {
  try {
    const res = await fetch('/api/trials');
    if (!res.ok) return;
    const trials = await res.json();

    container.innerHTML = '';
    if (trials.length === 0) {
      container.innerHTML = '<p style="color:#555;font-size:12px">No past trials yet.</p>';
      return;
    }

    for (const trial of trials) {
      const card = document.createElement('div');
      card.className = 'past-trial-card';

      const verdictText = trial.verdict
        ? (trial.verdict.verdict === 'guilty' ? 'GUILTY' : 'NOT GUILTY')
        : 'IN PROGRESS';
      const verdictClass = trial.verdict
        ? (trial.verdict.verdict === 'guilty' ? 'guilty' : 'not-guilty')
        : '';
      const votes = trial.verdict
        ? `${trial.verdict.guilty_votes}-${trial.verdict.not_guilty_votes}`
        : '';
      const date = trial.timestamp.split('T')[0];

      card.innerHTML = `
        <div>
          <div class="past-trial-title">${trial.case_title}</div>
          <div class="past-trial-meta">${date} | ${trial.total_rounds} rounds | ${trial.juror_count} jurors</div>
        </div>
        <div class="past-trial-verdict ${verdictClass}">${verdictText} ${votes}</div>
      `;

      card.addEventListener('click', () => {
        // Navigate to replay URL
        window.location.href = `/court/?trial=${trial.id}`;
      });

      container.appendChild(card);
    }
  } catch (e) {
    // Silent fail
  }
}

// ─── Trial replay ───

async function loadAndReplayTrial(id) {
  try {
    const res = await fetch(`/api/trial-replay/${id}`);
    if (!res.ok) {
      alert('Trial not found');
      window.location.href = '/court/';
      return;
    }
    const trial = await res.json();

    // Reset ALL state before replay
    const verdictOverlay = document.getElementById('verdict-overlay');
    verdictOverlay.classList.add('hidden');
    verdictOverlay.classList.remove('visible');
    store.set('verdict', null);
    store.set('round', 0);
    store.set('phase', '');
    store.set('phaseLabel', '');
    store.set('momentum', 0);
    store.set('objectionsSustained', 0);
    store.set('objectionsOverruled', 0);
    store.set('apiCost', 0);
    store.set('guiltyCount', 0);
    store.set('undecidedCount', 12);
    store.set('innocentCount', 0);
    store._notify('jurySplit', { guilty: 0, undecided: 12, innocent: 0 });
    store.state.transcript = [];
    if (store._transcriptKeys) store._transcriptKeys.clear();
    const feedEl = document.getElementById('transcript-feed');
    if (feedEl) feedEl.innerHTML = '';
    store.state.jurors = new Map();

    store.set('status', 'replay');
    store.set('totalRounds', trial.total_rounds);

    // Set up jury
    for (const j of trial.jurors) {
      store.updateJuror(j.seat, {
        conviction: j.final_conviction,
        confidence: 0.5,
        name: j.name,
      });
    }

    // Replay transcript entries with delays
    let lastRound = 0;
    for (let i = 0; i < trial.transcript.length; i++) {
      const entry = trial.transcript[i];

      // Phase label
      let phase = 'Opening Statements';
      if (entry.round > 2 && entry.round <= 10) phase = 'Prosecution Case';
      else if (entry.round > 10 && entry.round <= 18) phase = 'Defense Case';
      else if (entry.round > 18 && entry.round <= 20) phase = 'Rebuttal';
      else if (entry.round > 20 && entry.round <= 22) phase = 'Closing Arguments';
      else if (entry.round > 22) phase = 'Jury Deliberation';

      if (entry.round !== lastRound) {
        lastRound = entry.round;
        store.set('round', entry.round);
        store.set('phaseLabel', phase);
      }

      // Add to transcript (triggers bubbles + animations)
      store.addTranscript({
        type: entry.speaker_role === 'juror' ? 'deliberation' : 'argument',
        round: entry.round,
        speakerId: entry.speaker_id,
        speakerName: entry.speaker_name,
        speakerRole: entry.speaker_role,
        content: entry.content,
        juryImpact: (entry.jury_impact || []).map(([seat, delta]) => ({
          seat, delta, new_conviction: delta,
        })),
      });

      // Update jury conviction from history
      for (const j of trial.jurors) {
        const histEntry = j.conviction_history.find(h => h[0] === entry.round);
        if (histEntry) {
          store.updateJuror(j.seat, {
            conviction: histEntry[1],
            confidence: histEntry[2],
            name: j.name,
          });
        }
      }

      // Delay between entries (shorter in replay)
      await new Promise(r => setTimeout(r, 800));
    }

    // Show verdict
    if (trial.verdict) {
      store.set('verdict', {
        result: trial.verdict.verdict === 'guilty' ? 'guilty' : 'not_guilty',
        guilty: trial.verdict.guilty_votes,
        notGuilty: trial.verdict.not_guilty_votes,
        unanimous: trial.verdict.unanimous,
      });
    }
  } catch (e) {
    console.error('Replay error:', e);
  }
}
