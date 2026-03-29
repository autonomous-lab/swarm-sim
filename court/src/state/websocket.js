// WebSocket client — native trial events, no polling
import { store } from './store.js';

let ws = null;
let reconnectTimer = null;

export function connectWebSocket(url) {
  if (ws) ws.close();

  ws = new WebSocket(url);

  ws.onopen = () => {
    console.log('[WS] Connected');
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    // Fetch initial state once on connect
    fetchInitialState();
  };

  ws.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data);
      handleMessage(msg);
    } catch (e) {
      console.warn('[WS] Parse error:', e);
    }
  };

  ws.onclose = () => {
    console.log('[WS] Disconnected, reconnecting in 2s...');
    reconnectTimer = setTimeout(() => connectWebSocket(url), 2000);
  };

  ws.onerror = (err) => {
    console.error('[WS] Error:', err);
  };
}

async function fetchInitialState() {
  try {
    // Trial status
    const res = await fetch('/api/trial/status');
    if (res.ok) {
      const s = await res.json();
      applyTrialStatus(s);
    }
    // Jury
    const juryRes = await fetch('/api/trial/jury');
    if (juryRes.ok) {
      const jurors = await juryRes.json();
      for (const j of jurors) {
        store.updateJuror(j.seat, {
          conviction: j.conviction,
          confidence: j.confidence,
          name: j.name,
          trustProsecution: j.trust_prosecution,
          trustDefense: j.trust_defense,
          keyMoments: j.key_moments,
          convictionHistory: j.conviction_history,
        });
      }
    }
    // Load existing transcript
    const trRes = await fetch('/api/trial/transcript');
    if (trRes.ok) {
      const entries = await trRes.json();
      for (const e of entries) {
        store.addTranscript(mapTranscriptEntry(e));
      }
    }
    // Main status for cost
    const mainRes = await fetch('/api/status');
    if (mainRes.ok) {
      const main = await mainRes.json();
      store.set('apiCost', main.estimated_cost || 0);
      store.set('status', main.status);
    }

    // Fetch agent names for labels
    const agentsRes = await fetch('/api/agents');
    if (agentsRes.ok) {
      const agents = await agentsRes.json();
      const roleNames = {};
      const witnesses = [];
      for (const a of agents) {
        if (a.tier === 'tier1') roleNames.judge = a.name;
        if (a.tier === 'tier2') {
          // Check transcript to identify who is prosecutor vs defense vs witness
          // For now, collect all tier2 names
          if (!roleNames.prosecutor) roleNames.prosecutor = a.name;
          else if (!roleNames.defense) roleNames.defense = a.name;
          else witnesses.push(a.name);
        }
      }
      // Also check transcript for witness names
      const trRes2 = await fetch('/api/trial/transcript');
      if (trRes2.ok) {
        const entries = await trRes2.json();
        for (const e of entries) {
          if (e.speaker_role === 'witness' && !roleNames.witness) {
            roleNames.witness = e.speaker_name;
          }
        }
      }
      if (!roleNames.witness && witnesses.length > 0) {
        roleNames.witness = witnesses[0];
      }
      store.set('participantNames', roleNames);
    }
  } catch (e) {
    console.warn('[WS] Initial fetch failed:', e);
  }
}

function applyTrialStatus(s) {
  store.update({
    phase: s.phase,
    phaseLabel: s.phase,
    round: s.round,
    totalRounds: s.total_rounds,
    momentum: s.momentum,
  });
  if (s.jury_split) {
    const g = s.jury_split.guilty || 0;
    const u = s.jury_split.undecided || 0;
    const i = s.jury_split.innocent || 0;
    store.update({ guiltyCount: g, undecidedCount: u, innocentCount: i });
    store._notify('jurySplit', { guilty: g, undecided: u, innocent: i });
  }
  if (s.objections) {
    store.set('objectionsSustained', s.objections.sustained || 0);
    store.set('objectionsOverruled', s.objections.overruled || 0);
  }
  // Only apply verdict if a trial is actively running (not on initial page load)
  if (s.verdict && store.get('status') === 'running') {
    store.set('verdict', {
      result: s.verdict.verdict === 'guilty' ? 'guilty' : 'not_guilty',
      guilty: s.verdict.guilty_votes,
      notGuilty: s.verdict.not_guilty_votes,
      unanimous: s.verdict.unanimous,
    });
  }
}

function mapTranscriptEntry(e) {
  return {
    type: e.speaker_role === 'juror' ? 'deliberation' : 'argument',
    round: e.round,
    speakerId: e.speaker_id,
    speakerName: e.speaker_name,
    speakerRole: e.speaker_role,
    content: e.content,
    juryImpact: (e.jury_impact || []).map(([seat, delta]) => ({
      seat, delta, new_conviction: delta,
    })),
    seat: e.speaker_role === 'juror' ? parseInt(e.speaker_name.replace(/\D/g, '')) || 0 : undefined,
  };
}

function handleMessage(msg) {
  switch (msg.type) {
    // ─── Trial-specific native events ───
    case 'trial_argument':
      store.set('round', msg.round);
      store.addTranscript({
        type: 'argument',
        round: msg.round,
        speakerId: msg.speaker_id,
        speakerName: msg.speaker_name,
        speakerRole: msg.speaker_role,
        content: msg.content,
        juryImpact: (msg.jury_impact || []).map(([seat, delta, newConv]) => ({
          seat, delta, new_conviction: newConv,
        })),
      });
      break;

    case 'trial_jury_update':
      for (const j of (msg.jurors || [])) {
        store.updateJuror(j.seat, {
          conviction: j.conviction,
          confidence: j.confidence,
          name: j.name,
          convictionLabel: j.conviction_label,
          trustProsecution: j.trust_prosecution,
          trustDefense: j.trust_defense,
        });
      }
      break;

    case 'trial_phase_change':
      store.update({
        phase: msg.phase,
        phaseLabel: msg.phase,
        round: msg.round,
      });
      break;

    case 'trial_objection':
      const sustained = msg.ruling === 'sustained';
      store.set(
        sustained ? 'objectionsSustained' : 'objectionsOverruled',
        store.get(sustained ? 'objectionsSustained' : 'objectionsOverruled') + 1
      );
      store.addTranscript({
        type: 'objection',
        round: msg.round,
        by: msg.by_name,
        byRole: msg.by_name, // simplified
        grounds: msg.grounds,
        ruling: msg.ruling,
      });
      break;

    case 'trial_verdict':
      store.set('verdict', {
        result: msg.result,
        guilty: msg.guilty,
        notGuilty: msg.not_guilty,
        unanimous: msg.unanimous,
      });
      break;

    // ─── Standard events ───
    case 'round_end':
      store.set('round', msg.round);
      store.set('apiCost', msg.estimated_cost || 0);
      // Refresh trial status on round end
      fetch('/api/trial/status').then(r => r.ok ? r.json() : null).then(s => {
        if (s) applyTrialStatus(s);
      }).catch(() => {});
      break;

    case 'status_update':
      store.set('status', msg.status);
      store.set('round', msg.current_round);
      store.set('totalRounds', msg.total_rounds);
      break;

    case 'simulation_end':
      // Final status fetch
      fetch('/api/trial/status').then(r => r.ok ? r.json() : null).then(s => {
        if (s) applyTrialStatus(s);
      }).catch(() => {});
      break;

    default:
      break;
  }
}
