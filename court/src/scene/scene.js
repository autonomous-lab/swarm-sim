// Courtroom 3D scene — geometry, lights, materials
import * as THREE from 'three';
import { PALETTE } from '../utils/colors.js';

export function buildCourtroom(scene) {
  // Materials
  const woodMat = new THREE.MeshStandardMaterial({ color: PALETTE.wood, roughness: 0.8, metalness: 0.1 });
  const woodLightMat = new THREE.MeshStandardMaterial({ color: PALETTE.woodLight, roughness: 0.7, metalness: 0.1 });
  const marbleMat = new THREE.MeshStandardMaterial({ color: PALETTE.marble, roughness: 0.3, metalness: 0.05 });
  const leatherMat = new THREE.MeshStandardMaterial({ color: PALETTE.leather, roughness: 0.9, metalness: 0.0 });
  const wallMat = new THREE.MeshStandardMaterial({ color: 0x2a2520, roughness: 0.9, metalness: 0.0 });
  const darkMat = new THREE.MeshStandardMaterial({ color: 0x151510, roughness: 0.95, metalness: 0.0 });

  // ─── Floor ───
  const floor = new THREE.Mesh(
    new THREE.PlaneGeometry(30, 25),
    marbleMat
  );
  floor.rotation.x = -Math.PI / 2;
  floor.position.y = 0;
  floor.receiveShadow = true;
  scene.add(floor);

  // ─── Walls ───
  // Walls removed — open courtroom for free camera movement

  // ─── Judge's bench (elevated platform) ───
  const benchPlatform = new THREE.Mesh(new THREE.BoxGeometry(8, 1.2, 3), woodMat);
  benchPlatform.position.set(0, 0.6, -10);
  benchPlatform.castShadow = true;
  benchPlatform.receiveShadow = true;
  scene.add(benchPlatform);

  // Bench desk
  const benchDesk = new THREE.Mesh(new THREE.BoxGeometry(7, 1, 0.8), woodLightMat);
  benchDesk.position.set(0, 1.7, -9.2);
  benchDesk.castShadow = true;
  scene.add(benchDesk);

  // Judge nameplate
  const nameplateGeo = new THREE.BoxGeometry(1.5, 0.3, 0.15);
  const nameplateMat = new THREE.MeshStandardMaterial({ color: PALETTE.judge, roughness: 0.3, metalness: 0.7 });
  const nameplate = new THREE.Mesh(nameplateGeo, nameplateMat);
  nameplate.position.set(0, 2.3, -8.85);
  scene.add(nameplate);

  // ─── Witness stand (left of judge) ───
  const witnessBox = new THREE.Mesh(new THREE.BoxGeometry(2.5, 0.8, 2.5), woodMat);
  witnessBox.position.set(-6, 0.4, -8);
  witnessBox.castShadow = true;
  scene.add(witnessBox);

  const witnessRail = new THREE.Mesh(new THREE.BoxGeometry(2.5, 0.6, 0.2), woodLightMat);
  witnessRail.position.set(-6, 1.1, -6.85);
  scene.add(witnessRail);

  // ─── Attorney tables ───
  // Prosecution (left)
  const prosTable = new THREE.Mesh(new THREE.BoxGeometry(4, 0.8, 2), woodLightMat);
  prosTable.position.set(-5, 0.4, -3);
  prosTable.castShadow = true;
  scene.add(prosTable);

  // Defense (right)
  const defTable = new THREE.Mesh(new THREE.BoxGeometry(4, 0.8, 2), woodLightMat);
  defTable.position.set(5, 0.4, -3);
  defTable.castShadow = true;
  scene.add(defTable);

  // ─── Jury box (right side, 2 rows of 6) ───
  const juryPlatform = new THREE.Mesh(new THREE.BoxGeometry(10, 0.4, 5), woodMat);
  juryPlatform.position.set(0, 0.2, 4);
  juryPlatform.receiveShadow = true;
  scene.add(juryPlatform);

  // Back row elevated
  const juryBackRow = new THREE.Mesh(new THREE.BoxGeometry(10, 0.4, 2), woodMat);
  juryBackRow.position.set(0, 0.6, 5.5);
  scene.add(juryBackRow);

  // Jury rail
  const juryRail = new THREE.Mesh(new THREE.BoxGeometry(10.5, 0.5, 0.15), woodLightMat);
  juryRail.position.set(0, 0.75, 1.65);
  scene.add(juryRail);

  // Gallery railing removed — open layout

  // Center aisle removed — cleaner look

  // ─── Lighting ───

  // Ambient
  const ambient = new THREE.AmbientLight(PALETTE.ambient, 0.4);
  scene.add(ambient);

  // Main overhead (warm, dramatic)
  const mainLight = new THREE.DirectionalLight(PALETTE.warmLight, 1.2);
  mainLight.position.set(5, 12, -5);
  mainLight.castShadow = true;
  mainLight.shadow.mapSize.width = 2048;
  mainLight.shadow.mapSize.height = 2048;
  mainLight.shadow.camera.near = 0.5;
  mainLight.shadow.camera.far = 30;
  mainLight.shadow.camera.left = -15;
  mainLight.shadow.camera.right = 15;
  mainLight.shadow.camera.top = 15;
  mainLight.shadow.camera.bottom = -15;
  scene.add(mainLight);

  // Fill light (cool, from the side)
  const fillLight = new THREE.DirectionalLight(PALETTE.coolLight, 0.3);
  fillLight.position.set(-8, 6, 3);
  scene.add(fillLight);

  // Spotlight on witness stand
  const witnessSpot = new THREE.SpotLight(0xffffff, 0.8, 15, Math.PI / 8, 0.5);
  witnessSpot.position.set(-6, 8, -6);
  witnessSpot.target.position.set(-6, 1, -8);
  scene.add(witnessSpot);
  scene.add(witnessSpot.target);

  // Spotlight on judge
  const judgeSpot = new THREE.SpotLight(PALETTE.warmLight, 0.5, 12, Math.PI / 6, 0.5);
  judgeSpot.position.set(0, 8, -8);
  judgeSpot.target.position.set(0, 1.5, -10);
  scene.add(judgeSpot);
  scene.add(judgeSpot.target);

  // No fog — keeps background clean when rotating camera

  return {
    // Return positions for character placement
    positions: {
      judge: { x: 0, y: 1.2, z: -10 },
      prosecutor: { x: -5, y: 0, z: -3.8 },
      defense: { x: 5, y: 0, z: -3.8 },
      witness: { x: -6, y: 0.8, z: -8 },
      // Jury: 2 rows of 6
      jury: [
        // Front row (seats 1-6)
        { x: -3.5, y: 0.4, z: 3 },
        { x: -2.0, y: 0.4, z: 3 },
        { x: -0.5, y: 0.4, z: 3 },
        { x: 1.0, y: 0.4, z: 3 },
        { x: 2.5, y: 0.4, z: 3 },
        { x: 4.0, y: 0.4, z: 3 },
        // Back row (seats 7-12)
        { x: -3.5, y: 0.8, z: 5.5 },
        { x: -2.0, y: 0.8, z: 5.5 },
        { x: -0.5, y: 0.8, z: 5.5 },
        { x: 1.0, y: 0.8, z: 5.5 },
        { x: 2.5, y: 0.8, z: 5.5 },
        { x: 4.0, y: 0.8, z: 5.5 },
      ],
    },
  };
}
