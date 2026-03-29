// Camera management — presets, orbit controls, cinematic transitions
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { easeInOutQuad } from '../utils/math.js';

const PRESETS = {
  overview: {
    position: new THREE.Vector3(0, 16, 16),
    target: new THREE.Vector3(0, 0, -2),
  },
  jury: {
    position: new THREE.Vector3(0, 4, -1),
    target: new THREE.Vector3(0, 1.5, 4),
  },
  witness: {
    position: new THREE.Vector3(-2, 3, -5),
    target: new THREE.Vector3(-6, 1.5, -8),
  },
  judge: {
    position: new THREE.Vector3(0, 6, -6),
    target: new THREE.Vector3(0, 1.5, -10),
  },
  prosecution: {
    position: new THREE.Vector3(-2, 4, 0),
    target: new THREE.Vector3(-5, 1, -3.5),
  },
  defense: {
    position: new THREE.Vector3(2, 4, 0),
    target: new THREE.Vector3(5, 1, -3.5),
  },
  cinematic: {
    position: new THREE.Vector3(10, 5, 0),
    target: new THREE.Vector3(0, 1, -3),
    orbit: true,
  },
};

export class CameraController {
  constructor(camera, renderer) {
    this.camera = camera;
    this.controls = new OrbitControls(camera, renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.minDistance = 3;
    this.controls.maxDistance = 30;
    this.controls.maxPolarAngle = Math.PI / 2.1;

    this.transitioning = false;
    this.transitionStart = 0;
    this.transitionDuration = 1.5;
    this.fromPos = new THREE.Vector3();
    this.toPos = new THREE.Vector3();
    this.fromTarget = new THREE.Vector3();
    this.toTarget = new THREE.Vector3();

    this.cinematicMode = false;
    this.cinematicAngle = 0;

    this.currentPreset = 'overview';

    // Set initial position
    this.setPreset('overview', false);
  }

  setPreset(name, animate = true) {
    const preset = PRESETS[name];
    if (!preset) return;

    this.currentPreset = name;
    this.cinematicMode = !!preset.orbit;

    if (animate) {
      this.transitioning = true;
      this.transitionStart = performance.now();
      this.fromPos.copy(this.camera.position);
      this.fromTarget.copy(this.controls.target);
      this.toPos.copy(preset.position);
      this.toTarget.copy(preset.target);
    } else {
      this.camera.position.copy(preset.position);
      this.controls.target.copy(preset.target);
      this.controls.update();
    }
  }

  update(deltaTime) {
    if (this.transitioning) {
      const elapsed = (performance.now() - this.transitionStart) / 1000;
      const t = Math.min(elapsed / this.transitionDuration, 1);
      const eased = easeInOutQuad(t);

      this.camera.position.lerpVectors(this.fromPos, this.toPos, eased);
      this.controls.target.lerpVectors(this.fromTarget, this.toTarget, eased);

      if (t >= 1) {
        this.transitioning = false;
      }
    }

    if (this.cinematicMode && !this.transitioning) {
      this.cinematicAngle += deltaTime * 0.15;
      const radius = 14;
      const height = 6;
      this.camera.position.set(
        Math.cos(this.cinematicAngle) * radius,
        height,
        Math.sin(this.cinematicAngle) * radius
      );
      this.controls.target.set(0, 1, -3);
    }

    this.controls.update();
  }
}
