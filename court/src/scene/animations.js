// Animation manager for courtroom events
import { animateSpeaking, startSpeaking, stopSpeaking, standUp, flashJuror, updateJurorColor } from './characters.js';

export class AnimationManager {
  constructor(characters) {
    this.characters = characters;
    this.activeSpeaker = null;
    this.pendingAnimations = [];
  }

  update(deltaTime) {
    // Animate speaking for active speaker
    if (this.activeSpeaker) {
      animateSpeaking(this.activeSpeaker, deltaTime);
    }

    // Animate all jurors speaking during deliberation
    for (const juror of this.characters.jurors) {
      if (juror.userData.speaking) {
        animateSpeaking(juror, deltaTime);
      }
    }
  }

  // ─── Events ───

  onArgument(speakerRole, juryImpacts) {
    // Stop previous speaker
    if (this.activeSpeaker) {
      stopSpeaking(this.activeSpeaker);
    }

    // Start new speaker
    const character = this._getCharacterByRole(speakerRole);
    if (character) {
      startSpeaking(character);
      this.activeSpeaker = character;

      // Auto-stop after 3s
      setTimeout(() => {
        if (this.activeSpeaker === character) {
          stopSpeaking(character);
          this.activeSpeaker = null;
        }
      }, 3000);
    }

    // Flash jurors who shifted
    for (const impact of juryImpacts) {
      const jurorIdx = impact.seat - 1;
      if (jurorIdx >= 0 && jurorIdx < this.characters.jurors.length) {
        const juror = this.characters.jurors[jurorIdx];
        flashJuror(juror, impact.delta > 0);
        updateJurorColor(juror, impact.new_conviction);
      }
    }
  }

  onObjection(byRole) {
    const character = this._getCharacterByRole(byRole);
    if (character) {
      standUp(character);
    }
  }

  onJuryShift(seat, newConviction, isGuiltyShift) {
    const jurorIdx = seat - 1;
    if (jurorIdx >= 0 && jurorIdx < this.characters.jurors.length) {
      const juror = this.characters.jurors[jurorIdx];
      flashJuror(juror, isGuiltyShift);
      updateJurorColor(juror, newConviction);
    }
  }

  onWitnessCalled(name) {
    this.characters.witness.visible = true;
    this.characters.witness.userData.name = name;
  }

  onTestimony() {
    startSpeaking(this.characters.witness);
    setTimeout(() => stopSpeaking(this.characters.witness), 3000);
  }

  onDeliberation(seat) {
    // Flash the speaking juror
    const jurorIdx = seat - 1;
    if (jurorIdx >= 0 && jurorIdx < this.characters.jurors.length) {
      const juror = this.characters.jurors[jurorIdx];
      startSpeaking(juror);
      setTimeout(() => stopSpeaking(juror), 2500);
    }
  }

  onVerdict(result) {
    // All jurors stand
    for (const juror of this.characters.jurors) {
      standUp(juror);
    }
  }

  // Update all juror colors from store state
  updateJurorColors(jurorsMap) {
    for (const [seat, data] of jurorsMap) {
      const idx = seat - 1;
      if (idx >= 0 && idx < this.characters.jurors.length) {
        updateJurorColor(this.characters.jurors[idx], data.conviction || 0);
      }
    }
  }

  _getCharacterByRole(role) {
    switch (role) {
      case 'judge': return this.characters.judge;
      case 'prosecutor': return this.characters.prosecutor;
      case 'defense_attorney':
      case 'defense': return this.characters.defense;
      case 'witness': return this.characters.witness;
      default: return null;
    }
  }
}
