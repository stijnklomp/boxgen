// The 3D preview: the real `body.stl` / `lid.stl` build artifacts in a
// three.js scene with orbit and zoom. No geometry is generated in the
// browser: the meshes are the files FreeCAD wrote (ADR-0002).
//
// three.js is vendored (MIT) under `/assets/vendor/`; the bare `three`
// specifier resolves through the import map in `index.html`.

import * as THREE from "three";
import { STLLoader } from "./vendor/STLLoader.js";

const BODY_COLOR = 0x4f7fbf;
const LID_COLOR = 0xd08a3e;
const BACKGROUND = 0x22262b;

/**
 * Mounts the viewer on `container`.
 *
 * `bodyUrl` is required; `lidUrl` is optional (a body-only build). Returns a
 * handle with the visibility toggles and `dispose`.
 */
export function mountViewer(container, { bodyUrl, lidUrl = null }) {
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(window.devicePixelRatio || 1);
  container.append(renderer.domElement);

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(BACKGROUND);

  const camera = new THREE.PerspectiveCamera(45, 1, 1, 10000);
  const group = new THREE.Group();
  // The build is z-up (origin at the centre of the outer footprint, z = 0 at
  // the bottom); three.js is y-up.
  group.rotation.x = -Math.PI / 2;
  scene.add(group);

  scene.add(new THREE.HemisphereLight(0xffffff, 0x555566, 2.4));
  const key = new THREE.DirectionalLight(0xffffff, 2.2);
  key.position.set(300, 500, 400);
  scene.add(key);
  const fill = new THREE.DirectionalLight(0xffffff, 0.7);
  fill.position.set(-400, -250, -350);
  scene.add(fill);

  const target = new THREE.Vector3(0, 0, 0);
  let theta = 0.85;
  let phi = 1.0;
  let radius = 300;

  const updateCamera = () => {
    const sinPhi = Math.sin(phi);
    camera.position.set(
      target.x + radius * sinPhi * Math.sin(theta),
      target.y + radius * Math.cos(phi),
      target.z + radius * sinPhi * Math.cos(theta),
    );
    camera.lookAt(target);
  };

  let body = null;
  let lid = null;
  const loader = new STLLoader();

  const load = (url, color) =>
    loader.loadAsync(url).then((geometry) => {
      geometry.computeVertexNormals();
      const material = new THREE.MeshStandardMaterial({
        color,
        metalness: 0.05,
        roughness: 0.55,
        side: THREE.DoubleSide,
      });
      const mesh = new THREE.Mesh(geometry, material);
      group.add(mesh);
      return mesh;
    });

  Promise.all([load(bodyUrl, BODY_COLOR), lidUrl ? load(lidUrl, LID_COLOR) : Promise.resolve(null)])
    .then(([bodyMesh, lidMesh]) => {
      body = bodyMesh;
      lid = lidMesh;
      frame();
      updateCamera();
      renderer.setAnimationLoop(() => renderer.render(scene, camera));
    })
    .catch((error) => {
      renderer.setAnimationLoop(null);
      renderer.domElement.remove();
      const message = document.createElement("p");
      message.className = "viewer-message";
      message.textContent = `The meshes could not be loaded: ${error}`;
      container.append(message);
    });

  /** Frames the loaded model and sets the orbit radius. */
  function frame() {
    const box = new THREE.Box3().setFromObject(group);
    if (box.isEmpty()) return;
    box.getCenter(target);
    const size = box.getSize(new THREE.Vector3());
    radius = Math.max(size.x, size.y, size.z, 50) * 1.9;
  }

  // ---- orbit and zoom ----

  const canvas = renderer.domElement;
  let dragging = false;
  let lastX = 0;
  let lastY = 0;

  canvas.addEventListener("pointerdown", (event) => {
    dragging = true;
    lastX = event.clientX;
    lastY = event.clientY;
    canvas.setPointerCapture(event.pointerId);
  });
  canvas.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    const dx = event.clientX - lastX;
    const dy = event.clientY - lastY;
    lastX = event.clientX;
    lastY = event.clientY;
    theta -= dx * 0.008;
    phi = Math.min(Math.max(phi - dy * 0.008, 0.05), Math.PI - 0.05);
    updateCamera();
  });
  const stop = () => {
    dragging = false;
  };
  canvas.addEventListener("pointerup", stop);
  canvas.addEventListener("pointercancel", stop);
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      radius = Math.min(Math.max(radius * (1 + Math.sign(event.deltaY) * 0.12), 40), 4000);
      updateCamera();
    },
    { passive: false },
  );

  // ---- size ----

  const resize = () => {
    const width = Math.max(container.clientWidth, 1);
    const height = Math.max(container.clientHeight, 1);
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  };
  const observer = new ResizeObserver(resize);
  observer.observe(container);
  resize();
  updateCamera();

  return {
    setBodyVisible(visible) {
      if (body) body.visible = visible;
    },
    setLidVisible(visible) {
      if (lid) lid.visible = visible;
    },
    dispose() {
      observer.disconnect();
      renderer.setAnimationLoop(null);
      renderer.dispose();
      canvas.remove();
    },
  };
}
