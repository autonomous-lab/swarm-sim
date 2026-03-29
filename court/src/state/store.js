// Reactive state store for the tribunal UI

class Store {
  constructor() {
    this.state = {
      // Trial metadata
      phase: 'opening',
      phaseLabel: 'Opening Statements',
      round: 0,
      totalRounds: 27,

      // Jury state: Map<seatNumber, jurorState>
      jurors: new Map(),

      // Agents: Map<id, agentInfo>
      agents: new Map(),

      // Jury split
      guiltyCount: 0,
      undecidedCount: 12,
      innocentCount: 0,

      // Momentum: -1.0 (defense) to 1.0 (prosecution)
      momentum: 0,

      // Objections
      objectionsSustained: 0,
      objectionsOverruled: 0,

      // Cost
      apiCost: 0,

      // Transcript entries
      transcript: [],

      // Verdict
      verdict: null,

      // Selected agent for sidebar
      selectedAgentId: null,

      // Simulation status
      status: 'idle',
    };

    this._listeners = new Map();
  }

  get(key) {
    return this.state[key];
  }

  set(key, value) {
    this.state[key] = value;
    this._notify(key, value);
  }

  update(partial) {
    for (const [key, value] of Object.entries(partial)) {
      this.state[key] = value;
      this._notify(key, value);
    }
  }

  on(key, callback) {
    if (!this._listeners.has(key)) {
      this._listeners.set(key, []);
    }
    this._listeners.get(key).push(callback);
  }

  _notify(key, value) {
    const listeners = this._listeners.get(key) || [];
    for (const cb of listeners) {
      cb(value);
    }
  }

  // ─── Jury helpers ───

  updateJuror(seat, updates) {
    const juror = this.state.jurors.get(seat) || {};
    this.state.jurors.set(seat, { ...juror, ...updates });
    this._recalcJurySplit();
    this._notify('jurors', this.state.jurors);
  }

  _recalcJurySplit() {
    let guilty = 0, undecided = 0, innocent = 0;
    for (const j of this.state.jurors.values()) {
      if (j.conviction > 0.2) guilty++;
      else if (j.conviction < -0.2) innocent++;
      else undecided++;
    }
    this.state.guiltyCount = guilty;
    this.state.undecidedCount = undecided;
    this.state.innocentCount = innocent;
    this._notify('jurySplit', { guilty, undecided, innocent });
  }

  // ─── Transcript ───

  addTranscript(entry) {
    // Dedup: skip if we already have an entry with same round + role + first 50 chars
    const key = `${entry.round}:${entry.speakerRole}:${(entry.content || '').substring(0, 50)}`;
    if (this._transcriptKeys && this._transcriptKeys.has(key)) return;
    if (!this._transcriptKeys) this._transcriptKeys = new Set();
    this._transcriptKeys.add(key);

    this.state.transcript.push(entry);
    this._notify('transcript', entry);
  }
}

export const store = new Store();
