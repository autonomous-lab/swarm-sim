// Color utilities for conviction-to-color mapping
import * as THREE from 'three';

export const PALETTE = {
  bg: 0x0d0d0f,
  surface: 0x1a1a1e,
  judge: 0xd4a843,
  prosecutor: 0xc0392b,
  defense: 0x2c5f8a,
  witness: 0xcccccc,
  jurorNeutral: 0x555560,
  guilty: 0xe74c3c,
  innocent: 0x3498db,
  wood: 0x3d2b1f,
  woodLight: 0x5c3d2e,
  marble: 0xd9d0c7,
  leather: 0x2a1f14,
  ambient: 0x1a1520,
  warmLight: 0xffe4c4,
  coolLight: 0x8899aa,
};

/**
 * Map conviction (-1 to 1) to a color.
 * -1 = full innocent (blue), 0 = neutral (gray), 1 = full guilty (red)
 */
export function convictionToColor(conviction) {
  const guilty = new THREE.Color(PALETTE.guilty);
  const neutral = new THREE.Color(PALETTE.jurorNeutral);
  const innocent = new THREE.Color(PALETTE.innocent);

  if (conviction > 0) {
    return neutral.clone().lerp(guilty, Math.min(conviction, 1));
  } else {
    return neutral.clone().lerp(innocent, Math.min(-conviction, 1));
  }
}

/**
 * CSS color from conviction.
 */
export function convictionToCss(conviction) {
  const c = convictionToColor(conviction);
  return `#${c.getHexString()}`;
}

/**
 * Get role color.
 */
export function roleColor(role) {
  switch (role) {
    case 'judge': return PALETTE.judge;
    case 'prosecutor': return PALETTE.prosecutor;
    case 'defense_attorney': return PALETTE.defense;
    case 'witness': return PALETTE.witness;
    case 'juror': return PALETTE.jurorNeutral;
    default: return PALETTE.surface;
  }
}
