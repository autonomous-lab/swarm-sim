# AI Judgment — Tribunal Simulation Engine

**Fork de swarm-sim.** Même moteur Rust (tiered batching, cognitive modeling, WebSocket), mais le frontend devient une salle de tribunal 3D en Three.js avec HUD temps réel.

---

## Concept

Un procès simulé par agents IA. L'utilisateur donne une affaire (texte libre, article, document PDF), le moteur extrait les parties prenantes, génère les profils (juge, avocats, témoins, jurés) et lance le procès. Chaque round = une phase procédurale. Le rendu 3D montre la salle d'audience avec les agents qui parlent, réagissent, et évoluent en temps réel.

**Killer feature :** On voit le jury changer d'avis live. Les jauges au-dessus de leurs têtes bougent à chaque argument. Le verdict se construit sous nos yeux.

---

## Mapping Tiers → Rôles

| Tier | Rôle | Batch | Modèle | Justification |
|------|-------|-------|--------|---------------|
| **Tier 1** | Juge | 1 (individuel) | Best (GPT-4o, Claude) | Doit raisonner sur la procédure, trancher les objections, guider le jury. Needs best reasoning. |
| **Tier 2** | Avocats (Procureur + Défense) + Témoins | 1-2 | Mid (GPT-4o-mini, Gemini Flash) | Stratégie argumentative, contre-interrogatoires, réponses adaptatives. |
| **Tier 3** | Jury (12 jurés) | 6-12 | Cheap (Qwen, DeepSeek) | Réactions internes, pas de prise de parole publique. Conviction + émotions. Batchable. |

### Flow causal par round

```
Juge ouvre/dirige → Avocat actif plaide/interroge → Témoin répond (si interrogatoire)
    → Jury réagit internement (beliefs shift, fatigue, attention)
    → Autre avocat objecte/contre-interroge → Juge tranche
```

---

## Phases du procès (mapping rounds)

Le procès suit une structure procédurale réaliste. Les rounds sont groupés en phases :

| Phase | Rounds | Description |
|-------|--------|-------------|
| **Opening** | 1-2 | Déclarations d'ouverture. Procureur puis Défense exposent leur thèse. Jury forme ses premières impressions. |
| **Prosecution Case** | 3-N | Procureur appelle ses témoins. Interrogatoire direct + contre-interrogatoire par la défense. |
| **Defense Case** | N+1-M | Défense appelle ses témoins. Même format inversé. |
| **Rebuttal** | M+1-M+2 | Procureur peut rappeler des témoins pour contrer la défense. |
| **Closing** | M+3-M+4 | Plaidoiries finales. Dernier push persuasif. Gros impact sur le jury. |
| **Deliberation** | M+5-fin | Jury délibère entre eux (intra-tier communication). Débat, persuasion, pression sociale. Vote. |

Le nombre de rounds par phase est configurable. Default : 2 + 8 + 8 + 2 + 2 + 5 = **27 rounds**.

---

## Agents — Profils détaillés

### Juge (Tier 1)

- **Rôle :** Dirige la procédure, tranche les objections, donne les instructions au jury
- **Cognitive state :** Impartialité (bias tracker), patience (diminue avec les objections frivoles)
- **Actions :** `sustain_objection`, `overrule_objection`, `instruct_jury`, `call_order`, `admit_evidence`, `strike_from_record`
- **Personnalité configurable :** strict/lenient, by-the-book/pragmatic

### Avocats (Tier 2)

- **Procureur :** Agressif, builds narrative of guilt, pose des questions piège
- **Défense :** Protecteur, sème le doute, attaque la crédibilité des témoins
- **Cognitive state :** Stratégie restante (arguments préparés), adaptabilité (réagit aux surprises), confidence
- **Actions :** `present_argument`, `call_witness`, `cross_examine`, `object`, `present_evidence`, `closing_statement`
- **Mémoire :** Track quels arguments ont marché (jury shift) et double down dessus

### Témoins (Tier 2)

- **Générés par extraction d'entités** depuis le document source
- **Types :** Expert (crédible, technique), Eyewitness (émotionnel, faillible), Character witness (personnel)
- **Cognitive state :** Nervosité (augmente sous pression), cohérence (peut se contredire si nerveux)
- **Actions :** `testify`, `answer_question`, `hesitate`, `break_down`

### Jurés (Tier 3)

- **12 profils démographiques variés** : âge, profession, background, biais initiaux
- **Cognitive state (le coeur du système) :**
  - `conviction` : float -1.0 (innocent) à 1.0 (coupable) — évolue chaque round
  - `confidence` : 0.0-1.0 — certitude dans sa conviction actuelle
  - `fatigue` : 0.0-1.0 — attention qui diminue (arguments tardifs moins impactants)
  - `emotional_state` : influenced par les témoignages émotionnels
  - `trust_map` : confiance envers chaque avocat (basée sur la cohérence de leurs arguments)
  - `key_moments` : liste des arguments qui ont le plus shifté leur conviction
- **Pendant le procès :** Pas de prise de parole, seulement des réactions internes
- **Pendant la délibération :** Prennent la parole, débattent, tentent de convaincre les autres jurés
- **Biais cognitifs simulés :** anchoring (première impression forte), recency (derniers arguments sur-pondérés), authority (experts > eyewitness), sympathy (témoignages émotionnels)

---

## Frontend Three.js — La Salle d'Audience

### Scène 3D

**Style visuel :** Low-poly stylisé, palette sombre (bois foncé, cuir, marbre). Éclairage dramatique avec ombres portées. Pas de réalisme — on vise un rendu type "Monument Valley meets courtroom drama".

**Layout de la salle :**

```
                    ┌─────────────────────┐
                    │      JUGE           │  ← Estrade surélevée
                    │   (chaise haute)    │
                    └─────────────────────┘
                              │
        ┌─────────┐           │          ┌─────────┐
        │ TÉMOIN  │           │          │ GREFFIER│
        │ (barre) │           │          │         │
        └─────────┘           │          └─────────┘
                              │
   ┌──────────┐                          ┌──────────┐
   │PROCUREUR │         (allée)          │ DÉFENSE  │
   │ (table)  │                          │ (table)  │
   └──────────┘                          └──────────┘
                              │
   ┌──────────────────────────────────────────────┐
   │               JURY (2 rangées de 6)          │
   │  [J1] [J2] [J3] [J4] [J5] [J6]              │
   │  [J7] [J8] [J9] [J10] [J11] [J12]           │
   └──────────────────────────────────────────────┘
                              │
                    ┌─────────────────┐
                    │    GALERIE      │  ← (optionnel, public)
                    └─────────────────┘
```

**Personnages :** Silhouettes géométriques (cylindre + sphère pour la tête). Couleurs par rôle :
- Juge : noir + or
- Procureur : rouge bordeaux
- Défense : bleu marine
- Témoins : blanc/gris
- Jurés : gris neutre → **teinte dynamique** (rouge = guilty, bleu = innocent, intensité = conviction)

**Animations simples :**
- Agent qui parle : légère oscillation verticale + bulle de texte
- Juré qui réagit : petit flash de couleur (particules)
- Objection : l'avocat se "lève" (scale Y temporaire)
- Juge qui tranche : coup de marteau (gavel animation)

### Camera

- **Vue par défaut :** Isométrique plongeante (vue d'ensemble)
- **Presets (boutons ou raccourcis clavier) :**
  - `1` — Vue d'ensemble
  - `2` — Zoom jury (face aux jurés, on voit toutes les jauges)
  - `3` — Zoom témoin (face à la barre)
  - `4` — Vue du juge (plongeante depuis l'estrade)
  - `5` — Vue cinématique (caméra qui orbite lentement)
- **OrbitControls** pour navigation libre (drag rotate, scroll zoom)

### HUD (HTML overlay, pas dans la scène 3D)

**Barre supérieure :**

```
┌────────────────────────────────────────────────────────────────────┐
│ ⚖️ PHASE: Prosecution Case  │  Round 7/27  │  ██████░░░░ 26%     │
│                                                                    │
│ JURY SPLIT:  ████████░░░░░░░░░░░░  GUILTY 4 │ UNDECIDED 5 │ NOT 3 │
│                                                                    │
│ MOMENTUM: ←←← Defense  ■■■■■■■■░░  Prosecution →→→                │
│                                                                    │
│ OBJECTIONS: Sustained 3 │ Overruled 5  │  API: $0.42 │ 12.4K tok  │
└────────────────────────────────────────────────────────────────────┘
```

**Au-dessus des têtes des jurés (world-space, Three.js CSS2DRenderer) :**

```
         ┌──────────────┐
         │ Sarah, 42    │
         │ Teacher      │
         │              │
         │  ◉───────○   │  ← Jauge conviction (guilty ◉ ←→ ○ innocent)
         │  Conf: 72%   │  ← Confidence dans sa position
         │  👁 Att: 85%  │  ← Attention (diminue avec fatigue)
         └──────────────┘
              [avatar]
```

Chaque juré a sa mini-card flottante. Quand un argument fait bouger la jauge, **flash visuel** + la jauge slide de manière animée.

**Au-dessus du juge :**

```
         ┌──────────────┐
         │ Judge Miller  │
         │ Impartial: 94%│  ← Bias tracker
         │ Patience: ██░ │  ← Diminue avec objections frivoles
         └──────────────┘
```

**Au-dessus des avocats :**

```
         ┌──────────────────┐
         │ PROSECUTION       │
         │ Persuasion: 38%   │  ← % du jury qui penche guilty
         │ Args left: 4/7    │  ← Arguments préparés restants
         │ Win rate: 3/8 obj │  ← Objections acceptées
         └──────────────────┘
```

### Panneau latéral (HTML, slide-in au click)

Click sur n'importe quel agent → panneau détaillé :

**Pour un juré :**
- Profil complet (âge, profession, background)
- Graphe d'évolution de conviction (sparkline round par round)
- Biais cognitifs actifs
- "Key moments" — les 3 arguments qui l'ont le plus impacté (avec citation)
- Trust scores envers chaque avocat

**Pour un avocat :**
- Stratégie actuelle
- Arguments utilisés vs restants
- Impact tracker : quel argument a bougé combien de jurés
- Historique des objections

**Pour le juge :**
- Historique des décisions (sustained/overruled)
- Instructions données au jury
- Bias meter evolution

### Feed central (bas de l'écran, collapsible)

Transcript du procès en temps réel, style chat :

```
[Round 7 — Prosecution Case — Cross-examination]

PROSECUTOR: "Dr. Williams, you claim the defendant was nowhere near the
scene. But how do you explain THIS security footage?"
    → Jury impact: J2 +0.15, J7 +0.22, J11 +0.08

DEFENSE: "Objection! This evidence was obtained without proper warrant."

JUDGE: "Overruled. The footage was from a public camera. Continue."
    → Jury reaction: J4 -0.05 (frustrated with ruling)

WITNESS: "I... I can explain. The timestamp shows—"
    → J9 attention dropped to 45% (fatigue)
```

---

## Rust Backend — Modifications

### Nouveau modèle d'agent (`agent.rs`)

```rust
enum CourtRole {
    Judge,
    Prosecutor,
    DefenseAttorney,
    Witness { witness_type: WitnessType, called_by: Party },
    Juror { seat_number: u8 },
}

enum WitnessType {
    Expert,
    Eyewitness,
    Character,
}

enum Party {
    Prosecution,
    Defense,
}

// Juror-specific state
struct JurorState {
    conviction: f32,        // -1.0 (innocent) to 1.0 (guilty)
    confidence: f32,        // 0.0-1.0
    emotional_state: f32,   // -1.0 to 1.0
    trust_prosecution: f32,
    trust_defense: f32,
    key_moments: Vec<KeyMoment>,
    cognitive_biases: Vec<CognitiveBias>,
    vote: Option<Verdict>,  // Set during deliberation
}

struct KeyMoment {
    round: u32,
    source_agent: Uuid,
    content_summary: String,
    conviction_delta: f32,
}

enum CognitiveBias {
    Anchoring { strength: f32 },
    RecencyBias { strength: f32 },
    AuthorityBias { strength: f32 },
    SympathyBias { strength: f32 },
    ConfirmationBias { strength: f32 },
}
```

### Nouvelles actions (`world.rs`)

```rust
enum ActionType {
    // Existing (keep for compatibility)
    CreatePost, Reply, Like, ...

    // Court-specific
    PresentArgument,
    CallWitness,
    CrossExamine,
    Object { grounds: ObjectionGrounds },
    SustainObjection,
    OverruleObjection,
    InstructJury,
    Testify,
    AnswerQuestion,
    Hesitate,
    ClosingStatement,

    // Deliberation
    DeliberateArgue,
    DeliberateAgree,
    DeliberateChallenge,
    CastVote { verdict: Verdict },
}

enum ObjectionGrounds {
    Hearsay,
    Relevance,
    LeadingQuestion,
    Speculation,
    AskedAndAnswered,
    Badgering,
}

enum Verdict {
    Guilty,
    NotGuilty,
}
```

### Phases dans `engine.rs`

```rust
enum TrialPhase {
    Opening,
    ProsecutionCase,
    DefenseCase,
    Rebuttal,
    Closing,
    Deliberation,
}

impl TrialPhase {
    fn active_speakers(&self) -> Vec<CourtRole> { ... }
    fn jury_can_speak(&self) -> bool {
        matches!(self, TrialPhase::Deliberation)
    }
}
```

### Nouveaux endpoints API (`server.rs`)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/trial/status` | Phase actuelle, round, jury split |
| `GET` | `/api/trial/jury` | État de tous les jurés (conviction, confidence, fatigue) |
| `GET` | `/api/trial/jury/:seat` | Détail d'un juré (evolution, key moments, biases) |
| `GET` | `/api/trial/transcript` | Transcript complet du procès |
| `GET` | `/api/trial/objections` | Historique des objections + rulings |
| `GET` | `/api/trial/evidence` | Pièces à conviction présentées |
| `GET` | `/api/trial/verdict` | Verdict final (si délibération terminée) |
| `GET` | `/api/trial/momentum` | Momentum timeline (qui domine round par round) |
| `POST` | `/api/trial/god-eye` | Injection événement (nouveau témoin surprise, pièce à conviction, scandal) |

### WebSocket events

```json
{"type": "phase_change",    "phase": "defense_case", "round": 11}
{"type": "argument",        "speaker": "prosecutor", "content": "...", "jury_impact": [...]}
{"type": "objection",       "by": "defense", "grounds": "hearsay", "ruling": "sustained"}
{"type": "jury_shift",      "juror_seat": 7, "old_conviction": 0.1, "new_conviction": 0.35, "cause": "..."}
{"type": "witness_called",  "name": "Dr. Williams", "type": "expert", "called_by": "prosecution"}
{"type": "testimony",       "witness": "...", "content": "...", "emotional_impact": 0.7}
{"type": "deliberation",    "speaker_seat": 3, "argument": "...", "reactions": [...]}
{"type": "vote_cast",       "seat": 5, "verdict": "guilty"}
{"type": "verdict",         "result": "guilty", "vote": {"guilty": 10, "not_guilty": 2}, "unanimous": false}
```

---

## God's Eye — Événements Tribunal

```toml
[[events]]
id = "surprise-witness"
inject_at_round = 12
event_type = "surprise_witness"
content = "A new witness has come forward — the defendant's former business partner."

[[events]]
id = "evidence-leak"
event_type = "evidence_bombshell"
content = "Leaked emails show the defendant knew about the safety violations 6 months prior."

[[events]]
id = "juror-pressure"
event_type = "jury_pressure"
content = "juror:seat_4 receives anonymous threats. Stress +0.5, attention -0.3"

[[events]]
id = "media-circus"
event_type = "media_attention"
content = "The case goes viral on social media. Public opinion overwhelmingly against the defendant."
```

---

## Structure du projet

```
swarm-sim/
├── src/                          # Rust backend (existant + modifs)
│   ├── main.rs
│   ├── config.rs
│   ├── agent.rs                  # + CourtRole, JurorState, CognitiveBias
│   ├── world.rs                  # + court-specific ActionTypes
│   ├── engine.rs                 # + TrialPhase, phase-based round logic
│   ├── trial.rs                  # NEW — trial-specific logic, jury impact calc
│   ├── llm.rs                    # + court-specific prompts
│   ├── metrics.rs                # + trial metrics (momentum, persuasion, bias)
│   ├── server.rs                 # + /api/trial/* endpoints
│   └── ...
├── web/                          # Ancien frontend (garder comme fallback)
└── court/                        # NEW — Three.js tribunal frontend
    ├── index.html                # Shell HTML
    ├── src/
    │   ├── main.js               # Entry point, init Three.js + WebSocket
    │   ├── scene/
    │   │   ├── courtroom.js      # Salle d'audience (géométrie, lumières, matériaux)
    │   │   ├── characters.js     # Agents 3D (low-poly silhouettes, couleurs dynamiques)
    │   │   ├── animations.js     # Parole, objection, gavel, jury reactions
    │   │   └── camera.js         # Presets, OrbitControls, transitions cinématiques
    │   ├── ui/
    │   │   ├── hud.js            # Barre supérieure (phase, jury split, momentum)
    │   │   ├── labels.js         # CSS2DRenderer — jauges au-dessus des têtes
    │   │   ├── sidebar.js        # Panneau agent detail (click)
    │   │   ├── transcript.js     # Feed procès en bas
    │   │   └── verdict.js        # Séquence verdict finale
    │   ├── state/
    │   │   ├── store.js          # State management (agents, jury, phase)
    │   │   └── websocket.js      # WebSocket client, event dispatch
    │   └── utils/
    │       ├── colors.js         # Palette, conviction-to-color mapping
    │       └── math.js           # Interpolation, easing functions
    │
    ├── assets/
    │   └── gavel.glb             # Marteau du juge (seul modèle 3D importé)
    ├── style.css                 # Dark theme, HUD layout, panels
    └── vite.config.js            # Build config
```

---

## Verdict — Séquence finale

Quand la délibération se termine :

1. **Caméra zoom** lentement sur le jury
2. Le foreman (juré #1) se lève — **pause dramatique** (2s)
3. Annonce du verdict — texte en gros au centre de l'écran
4. **Réaction visuelle :**
   - Guilty → teinte rouge sur toute la scène, son grave
   - Not Guilty → teinte bleue, lumière qui s'adoucit
5. **Stats finales :** overlay avec le breakdown complet :
   - Vote final (X-Y)
   - Turning points du procès (les 3 moments qui ont le plus bougé le jury)
   - MVP avocat (lequel a eu le plus d'impact)
   - Juré le plus influençable vs le plus ancré
   - Graphe conviction de chaque juré round par round

---

## Stack technique

- **Backend :** Rust (existant) + Axum + tokio + WebSocket
- **Frontend 3D :** Three.js (vanilla, pas de React — performance)
- **UI overlay :** HTML/CSS natif (pas de framework, comme l'existant)
- **Labels 3D :** CSS2DRenderer (Three.js)
- **Build :** Vite (pour le hot reload et le bundling Three.js)
- **Modèles 3D :** Géométrie procédurale (BoxGeometry, CylinderGeometry, SphereGeometry) — pas de fichiers .glb sauf le gavel

---

## Priorité d'implémentation

1. **Phase 1 — Backend court logic** : CourtRole, TrialPhase, JurorState, nouveaux ActionTypes, prompts LLM spécialisés tribunal. Tester en CLI.
2. **Phase 2 — API trial endpoints** : /api/trial/*, WebSocket events tribunal.
3. **Phase 3 — Scène 3D statique** : Salle d'audience, personnages placés, caméra, éclairage. Pas encore de data.
4. **Phase 4 — HUD + labels** : Jury split bar, jauges au-dessus des têtes, panneau latéral.
5. **Phase 5 — Live data** : Brancher WebSocket → scène. Couleurs dynamiques, animations, transcript.
6. **Phase 6 — Verdict sequence** : Séquence cinématique de fin.
7. **Phase 7 — Polish** : Particules, transitions caméra, sound design (optionnel), mobile responsive.
