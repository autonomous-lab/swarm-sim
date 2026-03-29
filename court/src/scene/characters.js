// Character models — low-poly stylized silhouettes
import * as THREE from 'three';
import { PALETTE, convictionToColor, roleColor } from '../utils/colors.js';

const BODY_HEIGHT = 1.4;
const HEAD_RADIUS = 0.25;

/**
 * Create a stylized character (cylinder body + sphere head).
 * Returns a THREE.Group with { body, head, role, agentId } userData.
 */
export function createCharacter(role, options = {}) {
  const group = new THREE.Group();
  const color = roleColor(role);

  // Body (tapered cylinder)
  const bodyGeo = new THREE.CylinderGeometry(0.2, 0.3, BODY_HEIGHT, 8);
  const bodyMat = new THREE.MeshStandardMaterial({
    color,
    roughness: 0.6,
    metalness: 0.1,
  });
  const body = new THREE.Mesh(bodyGeo, bodyMat);
  body.position.y = BODY_HEIGHT / 2;
  body.castShadow = true;
  group.add(body);

  // Head (sphere)
  const headGeo = new THREE.SphereGeometry(HEAD_RADIUS, 12, 8);
  const headMat = new THREE.MeshStandardMaterial({
    color: 0xe8d5c0, // skin tone
    roughness: 0.7,
    metalness: 0.0,
  });
  const head = new THREE.Mesh(headGeo, headMat);
  head.position.y = BODY_HEIGHT + HEAD_RADIUS * 0.8;
  head.castShadow = true;
  group.add(head);

  // Role-specific accessories
  if (role === 'judge') {
    // Gavel indication: small dark rectangle on bench
    const gavel = new THREE.Mesh(
      new THREE.BoxGeometry(0.15, 0.08, 0.08),
      new THREE.MeshStandardMaterial({ color: 0x1a0f05, roughness: 0.5, metalness: 0.3 })
    );
    gavel.position.set(0.4, BODY_HEIGHT + 0.1, 0);
    group.add(gavel);
  }

  // Store references
  group.userData = {
    role,
    agentId: options.agentId || null,
    seat: options.seat || null,
    name: options.name || '',
    bodyMesh: body,
    headMesh: head,
    baseColor: color,
    speaking: false,
    speakAnim: 0,
  };

  return group;
}

/**
 * Update juror body color based on conviction.
 */
export function updateJurorColor(character, conviction) {
  const color = convictionToColor(conviction);
  character.userData.bodyMesh.material.color.copy(color);
}

/**
 * Speaking animation — slight vertical oscillation.
 */
export function animateSpeaking(character, deltaTime) {
  if (!character.userData.speaking) return;

  character.userData.speakAnim += deltaTime * 6;
  const offset = Math.sin(character.userData.speakAnim) * 0.03;
  character.userData.headMesh.position.y = BODY_HEIGHT + HEAD_RADIUS * 0.8 + offset;
}

/**
 * Start speaking animation.
 */
export function startSpeaking(character) {
  character.userData.speaking = true;
  character.userData.speakAnim = 0;
}

/**
 * Stop speaking animation.
 */
export function stopSpeaking(character) {
  character.userData.speaking = false;
  character.userData.headMesh.position.y = BODY_HEIGHT + HEAD_RADIUS * 0.8;
}

/**
 * "Stand up" animation for objections (scale Y temporarily).
 */
export function standUp(character, callback) {
  const original = character.scale.y;
  character.scale.y = 1.15;
  setTimeout(() => {
    character.scale.y = original;
    if (callback) callback();
  }, 800);
}

/**
 * Flash effect on juror when conviction shifts.
 */
export function flashJuror(character, isGuiltyShift) {
  const mat = character.userData.bodyMesh.material;
  const flashColor = isGuiltyShift ? new THREE.Color(PALETTE.guilty) : new THREE.Color(PALETTE.innocent);
  const original = mat.emissive.clone();

  mat.emissive.copy(flashColor);
  mat.emissiveIntensity = 0.8;

  setTimeout(() => {
    mat.emissive.copy(original);
    mat.emissiveIntensity = 0;
  }, 500);
}

/**
 * Create all court characters and place them in the scene.
 */
export function populateCourt(scene, positions) {
  const characters = {
    judge: null,
    prosecutor: null,
    defense: null,
    witness: null,
    jurors: [],
  };

  // Judge
  const judge = createCharacter('judge', { name: 'Judge' });
  judge.position.set(positions.judge.x, positions.judge.y, positions.judge.z);
  scene.add(judge);
  characters.judge = judge;

  // Prosecutor
  const prosecutor = createCharacter('prosecutor', { name: 'Prosecutor' });
  prosecutor.position.set(positions.prosecutor.x, positions.prosecutor.y, positions.prosecutor.z);
  scene.add(prosecutor);
  characters.prosecutor = prosecutor;

  // Defense
  const defense = createCharacter('defense_attorney', { name: 'Defense' });
  defense.position.set(positions.defense.x, positions.defense.y, positions.defense.z);
  scene.add(defense);
  characters.defense = defense;

  // Witness (initially hidden)
  const witness = createCharacter('witness', { name: 'Witness' });
  witness.position.set(positions.witness.x, positions.witness.y, positions.witness.z);
  witness.visible = false;
  scene.add(witness);
  characters.witness = witness;

  // Jurors (12)
  for (let i = 0; i < 12; i++) {
    const pos = positions.jury[i];
    const juror = createCharacter('juror', { seat: i + 1, name: `Juror ${i + 1}` });
    juror.position.set(pos.x, pos.y, pos.z);
    scene.add(juror);
    characters.jurors.push(juror);
  }

  return characters;
}
