// HUD updates — connects store changes to DOM elements
import { store } from '../state/store.js';

export function initHUD() {
  const els = {
    phaseLabel: document.getElementById('phase-label'),
    roundLabel: document.getElementById('round-label'),
    progressFill: document.getElementById('progress-fill'),
    splitGuilty: document.getElementById('split-guilty'),
    splitUndecided: document.getElementById('split-undecided'),
    splitInnocent: document.getElementById('split-innocent'),
    guiltyCount: document.getElementById('guilty-count'),
    undecidedCount: document.getElementById('undecided-count'),
    innocentCount: document.getElementById('innocent-count'),
    momentumIndicator: document.getElementById('momentum-indicator'),
    objSustained: document.getElementById('obj-sustained'),
    objOverruled: document.getElementById('obj-overruled'),
    apiCost: document.getElementById('api-cost'),
  };

  // Phase changes
  store.on('phaseLabel', (label) => {
    els.phaseLabel.textContent = label;
  });

  store.on('round', (round) => {
    const total = store.get('totalRounds');
    els.roundLabel.textContent = `Round ${round}/${total}`;
    const pct = (round / total * 100).toFixed(0);
    els.progressFill.style.width = `${pct}%`;
  });

  // Jury split
  store.on('jurySplit', ({ guilty, undecided, innocent }) => {
    const total = guilty + undecided + innocent || 12;
    els.splitGuilty.style.width = `${guilty / total * 100}%`;
    els.splitUndecided.style.width = `${undecided / total * 100}%`;
    els.splitInnocent.style.width = `${innocent / total * 100}%`;
    els.guiltyCount.textContent = guilty;
    els.undecidedCount.textContent = undecided;
    els.innocentCount.textContent = innocent;
  });

  // Momentum
  store.on('momentum', (m) => {
    // -1 to 1 mapped to 0% to 100%
    const pct = ((m + 1) / 2 * 100).toFixed(1);
    els.momentumIndicator.style.left = `${pct}%`;
  });

  // Objections
  store.on('objectionsSustained', (v) => { els.objSustained.textContent = v; });
  store.on('objectionsOverruled', (v) => { els.objOverruled.textContent = v; });

  // Cost
  store.on('apiCost', (v) => { els.apiCost.textContent = v.toFixed(2); });
}
