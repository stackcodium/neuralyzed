// src/renderer/rust-iso-atlas-renderer.ts
var LOGICAL_W = 2304;
var LOGICAL_H = 1248;
var TILE_W = 96;
var TILE_H = 48;
var VIEW_W = 23;
var VIEW_H = 23;
var ATLAS_COLUMNS = 32;
var ATLAS_CELL = 128;
var TILE_CELLS = { "#": 1, "+": 2, "'": 3, ">": 4, "<": 5, _: 6, "*": 7, "%": 8, "^": 9, "~": 10 };
var CLASS_INDEX = { a: 0, r: 1, v: 2, t: 3, m: 4 };
var PLAYER_BASE = 58;
var MOB_BASE = 258;
var PLAYER_DIRECTION_STRIDE = 10;
var PLAYER_CLASS_STRIDE = 40;
var PLAYER_STATE_OFFSETS = [0, 1, 6, 7, 8, 9];
var FRIENDLY_PLAYER_STATE_OFFSETS = [0, 1, 7, 8, 0, 0, 0, 9];
var MOB_DIRECTION_STRIDE = 12;
var MOB_KIND_STRIDE = 48;
var MOB_STATE_OFFSETS = [0, 1, 6, 7, 8, 9, 10, 11];
var FLOOR_TILES = new Set([10]);
var ACTOR_TILES = new Set([1, 2, 6, 7]);
var IMMOBILIZED_MOB_STATES = new Set([4, 5, 6]);
function zKey(x, y, band, order) {
  return band * 1e9 + (x + y) * 1e5 + y * 1000 + order;
}
function projectCell(view, cell) {
  if (!view)
    return null;
  const x = cell % view.width, y = Math.floor(cell / view.width);
  return {
    x: view.ox + (LOGICAL_W / 2 + (x - view.cameraX - (y - view.cameraY)) * TILE_W / 2) * view.sceneScale,
    y: view.oy + (72 + (x - view.cameraX + (y - view.cameraY)) * TILE_H / 2) * view.sceneScale,
    scale: view.sceneScale
  };
}
async function createRustIsoRenderer(canvas, preference) {
  const embedded = globalThis.__MIB_RUST_EMBEDDED_ASSETS__;
  const [image, meta] = await Promise.all([
    loadImage(embedded?.atlasUrl ?? "./dist/rust-wasm/iso-atlas.png?v=20260721.1"),
    embedded ? Promise.resolve(embedded.atlasMeta) : fetch("./dist/rust-wasm/iso-atlas-meta.json?v=20260721.1", { cache: "no-store" }).then((response) => response.json())
  ]);
  if (preference === "webgl2") {
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: false, depth: false, preserveDrawingBuffer: false });
    if (gl && gl.getParameter(gl.MAX_TEXTURE_SIZE) >= Math.max(meta.width, meta.height))
      return new WebGlAtlasRenderer(canvas, gl, image, meta);
  }
  return new CanvasAtlasRenderer(canvas, image, meta);
}
function loadImage(src) {
  return new Promise((resolve, reject) => {
    const image = new Image;
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(`failed to load ${src}`));
    image.src = src;
  });
}
function cameraTarget(snapshot, transitionMs, enteringFloor = false) {
  const progress = enteringFloor ? 1 : Math.max(0, Math.min(1, transitionMs / 90));
  const fromX = snapshot.player.fromCell % snapshot.width, fromY = Math.floor(snapshot.player.fromCell / snapshot.width);
  const toX = snapshot.player.cell % snapshot.width, toY = Math.floor(snapshot.player.cell / snapshot.width);
  const playerX = fromX + (toX - fromX) * progress, playerY = fromY + (toY - fromY) * progress;
  const targetX = Math.max(0, Math.min(snapshot.width - VIEW_W, playerX - Math.floor(VIEW_W / 2)));
  const targetY = Math.max(0, Math.min(snapshot.height - VIEW_H, playerY - Math.floor(VIEW_H / 2)));
  let minX = snapshot.width, maxX = 0, minY = snapshot.height, maxY = 0;
  for (let cell = 0;cell < snapshot.seen.length; cell++)
    if (snapshot.seen[cell] === "1") {
      const x = cell % snapshot.width, y = Math.floor(cell / snapshot.width);
      if (x >= targetX && x < targetX + VIEW_W && y >= targetY && y < targetY + VIEW_H) {
        minX = Math.min(minX, x);
        maxX = Math.max(maxX, x);
        minY = Math.min(minY, y);
        maxY = Math.max(maxY, y);
      }
    }
  const spanX = maxX >= minX ? maxX - minX + 1 : VIEW_W, spanY = maxY >= minY ? maxY - minY + 1 : VIEW_H;
  const targetZoom = Math.max(1.12, Math.min(2.25, Math.min(VIEW_W / (spanX + 1), VIEW_H / (spanY + 1))));
  return { x: targetX, y: targetY, zoom: targetZoom };
}
function cameraOrigin(state, snapshot, transitionMs) {
  const enteringFloor = !state.initialized || state.floor !== snapshot.floor;
  const target = cameraTarget(snapshot, transitionMs, enteringFloor);
  const progress = Math.max(0, Math.min(1, transitionMs / 90));
  const presentedAt = snapshot.frame + progress;
  if (enteringFloor || snapshot.player.teleported) {
    state.x = target.x;
    state.y = target.y;
    state.zoom = target.zoom;
    state.floor = snapshot.floor;
    state.entryFrame = enteringFloor ? snapshot.frame : -1;
    state.initialized = true;
  } else {
    const elapsedFrames = Math.min(1, Math.max(0, presentedAt - state.lastAt));
    const follow = 1 - Math.exp(-elapsedFrames / 0.52);
    state.x += (target.x - state.x) * follow;
    state.y += (target.y - state.y) * follow;
    state.zoom += (target.zoom - state.zoom) * follow;
  }
  state.lastAt = presentedAt;
  return state;
}
function presentedPlayerPose(snapshot, transitionMs) {
  if (transitionMs < 90 || snapshot.player.state !== 1)
    return { state: snapshot.player.state, direction: snapshot.player.direction };
  const dead = snapshot.mobs.filter((mob) => mob.state === 7).sort((left, right) => {
    const distance = (cell) => {
      const x = cell % snapshot.width, y = Math.floor(cell / snapshot.width);
      const px2 = snapshot.player.cell % snapshot.width, py2 = Math.floor(snapshot.player.cell / snapshot.width);
      return Math.max(Math.abs(x - px2), Math.abs(y - py2));
    };
    return distance(left.cell) - distance(right.cell);
  })[0];
  if (!dead)
    return { state: snapshot.player.state, direction: snapshot.player.direction };
  const px = snapshot.player.cell % snapshot.width, py = Math.floor(snapshot.player.cell / snapshot.width);
  const tx = dead.cell % snapshot.width, ty = Math.floor(dead.cell / snapshot.width);
  const dx = tx - px, dy = ty - py;
  const direction = Math.abs(dx) >= Math.abs(dy) ? dx >= 0 ? 1 : 3 : dy >= 0 ? 2 : 0;
  return { state: 0, direction };
}
function presentedMobAtlasCell(snapshot, mob, animationFrame = 0) {
  if (mob.friendly) {
    const backupClass = (CLASS_INDEX[snapshot.class] + 1) % Object.keys(CLASS_INDEX).length;
    return PLAYER_BASE + backupClass * PLAYER_CLASS_STRIDE + mob.direction * PLAYER_DIRECTION_STRIDE + FRIENDLY_PLAYER_STATE_OFFSETS[mob.state] + animationFrame;
  }
  return MOB_BASE + mob.kind * MOB_KIND_STRIDE + mob.direction * MOB_DIRECTION_STRIDE + MOB_STATE_OFFSETS[mob.state] + animationFrame;
}
function shouldRenderMob(snapshot, mob) {
  if (snapshot.visible[mob.cell] === "1")
    return true;
  if (!mob.spotted || snapshot.seen[mob.cell] !== "1" || !IMMOBILIZED_MOB_STATES.has(mob.state))
    return false;
  const x = mob.cell % snapshot.width, y = Math.floor(mob.cell / snapshot.width);
  const px = snapshot.player.cell % snapshot.width, py = Math.floor(snapshot.player.cell / snapshot.width);
  return Math.max(Math.abs(x - px), Math.abs(y - py)) <= 9;
}
function commands(snapshot, meta, transitionMs, clockMs, camera) {
  const transitionProgress = Math.max(0, Math.min(1, transitionMs / 90));
  const px = snapshot.player.cell % snapshot.width;
  const py = Math.floor(snapshot.player.cell / snapshot.width);
  const x0 = Math.floor(camera.x);
  const y0 = Math.floor(camera.y);
  const x1 = Math.min(snapshot.width, x0 + VIEW_W + (camera.x % 1 ? 1 : 0));
  const y1 = Math.min(snapshot.height, y0 + VIEW_H + (camera.y % 1 ? 1 : 0));
  const result = [];
  const project = (x, y) => ({
    x: LOGICAL_W / 2 + (x - camera.x - (y - camera.y)) * TILE_W / 2,
    y: 72 + (x - camera.x + (y - camera.y)) * TILE_H / 2
  });
  const animatedPoint = (cell, fromCell, state) => {
    const tx = cell % snapshot.width, ty = Math.floor(cell / snapshot.width);
    if (camera.entryFrame === snapshot.frame || state !== 1 || fromCell === cell)
      return { ...project(tx, ty), gridX: tx, gridY: ty };
    const fx = fromCell % snapshot.width, fy = Math.floor(fromCell / snapshot.width);
    const progress = Math.min(1, transitionMs / 90);
    const gridX = fx + (tx - fx) * progress, gridY = fy + (ty - fy) * progress;
    return { ...project(gridX, gridY), gridX, gridY };
  };
  const animationFrame = (state) => state === 1 ? Math.floor(clockMs / 100) % 5 : 0;
  const walkAnchor = (firstCell, state) => {
    if (state !== 1)
      return;
    const anchors = meta.anchors.slice(firstCell, firstCell + 5);
    return [anchors.reduce((sum, anchor) => sum + anchor[0], 0) / anchors.length, anchors.reduce((sum, anchor) => sum + anchor[1], 0) / anchors.length];
  };
  for (let y = y0;y < y1; y++)
    for (let x = x0;x < x1; x++) {
      const index = y * snapshot.width + x;
      if (snapshot.seen[index] !== "1")
        continue;
      const point = project(x, y);
      result.push({ cell: 0, ...point, scale: 1, z: zKey(x, y, 0, 0) });
      const tile = TILE_CELLS[snapshot.map[index]];
      if (tile !== undefined) {
        const band = FLOOR_TILES.has(tile) ? 0 : ACTOR_TILES.has(tile) ? 3 : 1;
        const order = band === 3 ? 30 : band === 1 ? 10 : 0;
        result.push({ cell: tile, ...point, scale: 1, z: zKey(x, y, band, order) });
      }
    }
  for (const item of snapshot.items) {
    if (snapshot.seen[item.cell] !== "1")
      continue;
    const x = item.cell % snapshot.width, y = Math.floor(item.cell / snapshot.width);
    if (x < x0 || x >= x1 || y < y0 || y >= y1)
      continue;
    result.push({ cell: 26 + item.gear, ...project(x, y), scale: 0.7, z: zKey(x, y, 3, 20) });
  }
  for (const mob of snapshot.mobs) {
    if (!shouldRenderMob(snapshot, mob))
      continue;
    const x = mob.cell % snapshot.width, y = Math.floor(mob.cell / snapshot.width);
    if (x < x0 || x >= x1 || y < y0 || y >= y1)
      continue;
    const motionFrame = animationFrame(mob.state);
    const atlasCell = presentedMobAtlasCell(snapshot, mob, motionFrame);
    const position = animatedPoint(mob.cell, mob.fromCell, mob.state);
    const deathFade = mob.state === 7 ? Math.max(0, Math.min(1, (transitionProgress - 0.76) / 0.24)) : 0;
    const appearFade = mob.appeared ? Math.max(0.08, Math.min(1, transitionProgress / 0.42)) : 1;
    const opacity = mob.state === 7 ? 1 - deathFade : appearFade;
    result.push({ cell: atlasCell, x: position.x, y: position.y, scale: mob.boss ? 1.2 : mob.friendly ? 0.92 : 1, opacity, z: zKey(position.gridX, position.gridY, 3, 30), anchor: walkAnchor(atlasCell - motionFrame, mob.state) });
  }
  const playerPose = presentedPlayerPose(snapshot, transitionMs);
  const playerPosition = animatedPoint(snapshot.player.cell, snapshot.player.fromCell, playerPose.state);
  const teleportProgress = Math.max(0, Math.min(1, transitionMs / (snapshot.player.teleportPhase === "out" ? 520 : 620)));
  const playerOpacity = snapshot.player.teleportPhase === "in" ? Math.min(1, teleportProgress * 1.45) : 1;
  const playerCell = PLAYER_BASE + CLASS_INDEX[snapshot.class] * PLAYER_CLASS_STRIDE + playerPose.direction * PLAYER_DIRECTION_STRIDE + PLAYER_STATE_OFFSETS[playerPose.state] + animationFrame(playerPose.state);
  result.push({
    cell: playerCell,
    x: playerPosition.x,
    y: playerPosition.y,
    scale: 1,
    opacity: playerOpacity,
    z: zKey(playerPosition.gridX, playerPosition.gridY, 3, 30) + 5,
    anchor: walkAnchor(PLAYER_BASE + CLASS_INDEX[snapshot.class] * PLAYER_CLASS_STRIDE + playerPose.direction * PLAYER_DIRECTION_STRIDE + PLAYER_STATE_OFFSETS[playerPose.state], playerPose.state),
    muzzle: meta.muzzles?.[playerCell] ?? undefined
  });
  return result.sort((a, b) => a.z - b.z || a.y - b.y || a.x - b.x).filter((draw) => meta.anchors[draw.cell]);
}
function resize(canvas) {
  const dpr = Math.min(devicePixelRatio || 1, 1.5);
  const width = Math.max(1, Math.round(canvas.clientWidth * dpr));
  const height = Math.max(1, Math.round(canvas.clientHeight * dpr));
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  return { width, height, scale: Math.min(width / LOGICAL_W, height / LOGICAL_H) };
}

class CanvasAtlasRenderer {
  canvas;
  image;
  meta;
  kind = "canvas2d";
  ctx;
  camera = { x: 0, y: 0, zoom: 1.12, floor: 0, entryFrame: -1, initialized: false, lastAt: performance.now() };
  view = null;
  playerMuzzle = null;
  constructor(canvas, image, meta) {
    this.canvas = canvas;
    this.image = image;
    this.meta = meta;
    this.ctx = canvas.getContext("2d", { alpha: false });
  }
  render(snapshot, transitionMs = 1000, clockMs = transitionMs) {
    const started = performance.now();
    const size = resize(this.canvas);
    const ctx = this.ctx;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = "#020504";
    ctx.fillRect(0, 0, size.width, size.height);
    const camera = cameraOrigin(this.camera, snapshot, transitionMs);
    const sceneScale = size.scale * camera.zoom;
    const ox = (size.width - LOGICAL_W * sceneScale) / 2;
    const oy = (size.height - LOGICAL_H * sceneScale) / 2;
    this.view = { ox, oy, sceneScale, cameraX: camera.x, cameraY: camera.y, width: snapshot.width };
    this.playerMuzzle = null;
    for (const draw of commands(snapshot, this.meta, transitionMs, clockMs, camera)) {
      const anchor = draw.anchor ?? this.meta.anchors[draw.cell];
      const sx = draw.cell % ATLAS_COLUMNS * ATLAS_CELL;
      const sy = Math.floor(draw.cell / ATLAS_COLUMNS) * ATLAS_CELL;
      const scale = draw.scale * sceneScale;
      ctx.globalAlpha = draw.opacity ?? 1;
      const x0 = ox + draw.x * sceneScale - anchor[0] * scale;
      const y0 = oy + draw.y * sceneScale - anchor[1] * scale;
      if (draw.muzzle)
        this.playerMuzzle = { x: x0 + draw.muzzle[0] * scale, y: y0 + draw.muzzle[1] * scale, scale: sceneScale };
      ctx.drawImage(this.image, sx, sy, ATLAS_CELL, ATLAS_CELL, x0, y0, ATLAS_CELL * scale, ATLAS_CELL * scale);
    }
    ctx.globalAlpha = 1;
    return performance.now() - started;
  }
  projectCell(cell) {
    return projectCell(this.view, cell);
  }
  projectPlayerMuzzle() {
    return this.playerMuzzle;
  }
  destroy() {}
}

class WebGlAtlasRenderer {
  canvas;
  gl;
  meta;
  kind = "webgl2";
  program;
  buffer;
  position;
  camera = { x: 0, y: 0, zoom: 1.12, floor: 0, entryFrame: -1, initialized: false, lastAt: performance.now() };
  view = null;
  playerMuzzle = null;
  constructor(canvas, gl, image, meta) {
    this.canvas = canvas;
    this.gl = gl;
    this.meta = meta;
    const vertex = compile(gl, gl.VERTEX_SHADER, `#version 300 es
      in vec2 a_position; in vec2 a_uv; in float a_opacity; out vec2 v_uv; out float v_opacity; uniform vec2 u_size;
      void main(){ vec2 p=a_position/u_size*2.0-1.0; gl_Position=vec4(p.x,-p.y,0,1); v_uv=a_uv; v_opacity=a_opacity; }`);
    const fragment = compile(gl, gl.FRAGMENT_SHADER, `#version 300 es
      precision mediump float; in vec2 v_uv; in float v_opacity; out vec4 color; uniform sampler2D u_atlas;
      void main(){ color=texture(u_atlas,v_uv)*v_opacity; if(color.a<0.02) discard; }`);
    this.program = gl.createProgram();
    gl.attachShader(this.program, vertex);
    gl.attachShader(this.program, fragment);
    gl.linkProgram(this.program);
    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS))
      throw new Error(gl.getProgramInfoLog(this.program) || "WebGL link failed");
    this.position = gl.getAttribLocation(this.program, "a_position");
    const uv = gl.getAttribLocation(this.program, "a_uv");
    this.buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.enableVertexAttribArray(this.position);
    gl.vertexAttribPointer(this.position, 2, gl.FLOAT, false, 20, 0);
    gl.enableVertexAttribArray(uv);
    gl.vertexAttribPointer(uv, 2, gl.FLOAT, false, 20, 8);
    const opacity = gl.getAttribLocation(this.program, "a_opacity");
    gl.enableVertexAttribArray(opacity);
    gl.vertexAttribPointer(opacity, 1, gl.FLOAT, false, 20, 16);
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  }
  render(snapshot, transitionMs = 1000, clockMs = transitionMs) {
    const started = performance.now();
    const size = resize(this.canvas);
    const gl = this.gl;
    gl.viewport(0, 0, size.width, size.height);
    gl.clearColor(0.008, 0.02, 0.016, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const camera = cameraOrigin(this.camera, snapshot, transitionMs);
    const sceneScale = size.scale * camera.zoom;
    const ox = (size.width - LOGICAL_W * sceneScale) / 2;
    const oy = (size.height - LOGICAL_H * sceneScale) / 2;
    this.view = { ox, oy, sceneScale, cameraX: camera.x, cameraY: camera.y, width: snapshot.width };
    this.playerMuzzle = null;
    const data = [];
    for (const draw of commands(snapshot, this.meta, transitionMs, clockMs, camera)) {
      const anchor = draw.anchor ?? this.meta.anchors[draw.cell];
      const scale = draw.scale * sceneScale;
      const x0 = ox + draw.x * sceneScale - anchor[0] * scale, y0 = oy + draw.y * sceneScale - anchor[1] * scale;
      if (draw.muzzle)
        this.playerMuzzle = { x: x0 + draw.muzzle[0] * scale, y: y0 + draw.muzzle[1] * scale, scale: sceneScale };
      const x1 = x0 + ATLAS_CELL * scale, y1 = y0 + ATLAS_CELL * scale;
      const u0 = draw.cell % ATLAS_COLUMNS * ATLAS_CELL / this.meta.width;
      const v0 = Math.floor(draw.cell / ATLAS_COLUMNS) * ATLAS_CELL / this.meta.height;
      const u1 = u0 + ATLAS_CELL / this.meta.width, v1 = v0 + ATLAS_CELL / this.meta.height;
      const opacity = draw.opacity ?? 1;
      data.push(x0, y0, u0, v0, opacity, x1, y0, u1, v0, opacity, x0, y1, u0, v1, opacity, x0, y1, u0, v1, opacity, x1, y0, u1, v0, opacity, x1, y1, u1, v1, opacity);
    }
    gl.useProgram(this.program);
    gl.uniform2f(gl.getUniformLocation(this.program, "u_size"), size.width, size.height);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.DYNAMIC_DRAW);
    gl.drawArrays(gl.TRIANGLES, 0, data.length / 5);
    return performance.now() - started;
  }
  projectCell(cell) {
    return projectCell(this.view, cell);
  }
  projectPlayerMuzzle() {
    return this.playerMuzzle;
  }
  destroy() {
    this.gl.deleteBuffer(this.buffer);
    this.gl.deleteProgram(this.program);
  }
}
function compile(gl, kind, source) {
  const shader = gl.createShader(kind);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS))
    throw new Error(gl.getShaderInfoLog(shader) || "WebGL compile failed");
  return shader;
}

// src/renderer/rust-iso-effects.ts
var THROW_EFFECT_MS = 240;
var TRACER_EFFECT_MS = 90;
var TELEPORT_OUT_MS = 520;
var TELEPORT_IN_MS = 620;
var DURATIONS = {
  damage: 420,
  "player-damage": 420,
  heal: 520,
  kill: 560,
  tracer: TRACER_EFFECT_MS,
  throw: THROW_EFFECT_MS,
  "teleport-out": TELEPORT_OUT_MS,
  "teleport-in": TELEPORT_IN_MS,
  miss: 340,
  reward: 420,
  level: 560,
  status: 520,
  contact: 280,
  victory: 1900,
  defeat: 1900
};
var EFFECT_DESKTOP_SCENE_SCALE = 0.5330729166666667;
function rustIsoEffectTimelineMs(effects) {
  return effects.reduce((longest, effect) => Math.max(longest, (effect.delay ?? 0) + DURATIONS[effect.kind]), 0);
}
function rustIsoEffectTimeScale(effects, windowMs) {
  const naturalTimeline = rustIsoEffectTimelineMs(effects);
  const terminal = effects.some((effect) => effect.kind === "victory" || effect.kind === "defeat");
  return !terminal && windowMs && naturalTimeline > windowMs ? windowMs / naturalTimeline : 1;
}
function deriveRustIsoEffects(before, next) {
  if (!before)
    return [];
  const sameFloor = before.floor === next.floor;
  const effects = [];
  const add = (effect) => effects.push({ ...effect, delay: effect.delay ?? effects.filter((row) => row.cell === effect.cell).length * 90 });
  if (sameFloor && next.player.teleported && before.player.cell !== next.player.cell) {
    add({ kind: "teleport-out", cell: before.player.cell, delay: 0 });
    add({ kind: "teleport-in", cell: next.player.cell, delay: 0 });
  }
  if (before.player.hp > next.player.hp)
    add({ kind: "player-damage", cell: next.player.cell, magnitude: before.player.hp - next.player.hp });
  else if (before.player.hp < next.player.hp && /^(eat|use):/.test(next.action)) {
    add({ kind: "heal", cell: next.player.cell, magnitude: next.player.hp - before.player.hp });
  }
  if (sameFloor) {
    for (const oldMob of before.mobs) {
      const mob = next.mobs.find((candidate) => candidate.uid === oldMob.uid);
      if (!mob || oldMob.hp > 0 && mob.hp <= 0) {
        add({ kind: "damage", cell: oldMob.cell, magnitude: Math.max(1, oldMob.hp) });
        add({ kind: "kill", cell: oldMob.cell, delay: 100 });
      } else if (oldMob.hp > mob.hp)
        add({ kind: "damage", cell: mob.cell, magnitude: oldMob.hp - mob.hp });
    }
    for (const mob of next.mobs)
      if (mob.appeared)
        add({ kind: "contact", cell: mob.cell });
  }
  const target = actionTarget(next);
  if (target !== null && next.action.startsWith("fire:"))
    add({ kind: "tracer", cell: next.player.cell, targetCell: target, delay: 0 });
  if (target !== null && next.action.startsWith("throw:"))
    add({ kind: "throw", cell: next.player.cell, targetCell: target, delay: 0 });
  if (target !== null && next.logs.some((line) => line.text.startsWith("Shot missed")))
    add({ kind: "miss", cell: target });
  for (const line of next.logs) {
    if (line.text.startsWith("Picked up ") || line.text === "MIB supplies received.")
      add({ kind: "reward", cell: next.player.cell, color: "#73c8d6" });
    else if (/^\+\d+ credits\.$/.test(line.text))
      add({ kind: "reward", cell: next.player.cell, color: "#e4c15d" });
    else if (/^Level \d+\./.test(line.text))
      add({ kind: "level", cell: next.player.cell });
    else if (line.text.endsWith(" active."))
      add({ kind: "status", cell: next.player.cell, color: line.cls === "good" ? "#9adf91" : "#ff756f" });
  }
  if (next.won && !before.won)
    add({ kind: "victory", cell: next.player.cell, delay: 0 });
  else if (next.dead && !before.dead)
    add({ kind: "defeat", cell: next.player.cell, delay: 0 });
  return effects;
}
function splitRangedEffects(effects) {
  return {
    travel: effects.filter((effect) => effect.kind === "tracer" || effect.kind === "throw"),
    impact: effects.filter((effect) => effect.kind !== "tracer" && effect.kind !== "throw")
  };
}
function splitTeleportEffects(effects) {
  return {
    departure: effects.filter((effect) => effect.kind === "teleport-out"),
    arrival: effects.filter((effect) => effect.kind !== "teleport-out")
  };
}
function stageTeleportTransition(before, next) {
  if (!before || before.floor !== next.floor || !next.player.teleported || before.player.cell === next.player.cell)
    return null;
  const departure = {
    ...before,
    frame: next.frame,
    frameCount: next.frameCount,
    action: next.action,
    logs: [],
    policy: next.policy,
    player: {
      ...before.player,
      fromCell: before.player.cell,
      state: 0,
      teleported: false,
      teleportPhase: "out"
    }
  };
  const arrival = {
    ...next,
    player: {
      ...next.player,
      fromCell: next.player.cell,
      state: 0,
      teleportPhase: "in"
    }
  };
  return { departure, arrival };
}
function stageRangedImpactTransition(before, next) {
  if (!before || before.floor !== next.floor || !/^(fire|throw):/.test(next.action))
    return null;
  const mobs = next.mobs.map((mob) => {
    const previous = before.mobs.find((candidate) => candidate.uid === mob.uid);
    if (!previous || previous.hp <= mob.hp && previous.frozen === mob.frozen)
      return mob;
    return {
      ...mob,
      hp: previous.hp,
      fromCell: previous.fromCell,
      state: previous.state,
      direction: previous.direction,
      asleep: previous.asleep,
      pacified: previous.pacified,
      frozen: previous.frozen
    };
  });
  return { ...next, won: before.won, dead: before.dead, mobs };
}
function actionTarget(snapshot) {
  const fields = snapshot.action.split(":");
  const value = snapshot.action.startsWith("fire:") ? fields[1] : snapshot.action.startsWith("throw:") ? fields[2] : undefined;
  if (!value)
    return null;
  const [x, y] = value.split(",").map(Number);
  return Number.isInteger(x) && Number.isInteger(y) && x >= 0 && y >= 0 && x < snapshot.width && y < snapshot.height ? y * snapshot.width + x : null;
}

class RustIsoEffectLayer {
  canvas;
  ctx;
  active = [];
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
  }
  clear() {
    this.active = [];
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
  }
  clearCells(cells) {
    if (!cells.length)
      return;
    const removed = new Set(cells);
    this.active = this.active.filter((effect) => !removed.has(effect.cell) && (effect.targetCell === undefined || !removed.has(effect.targetCell)));
  }
  hasActiveAtCell(cell) {
    return this.active.some((effect) => effect.cell === cell || effect.targetCell === cell);
  }
  play(effects, now, windowMs) {
    const scale = rustIsoEffectTimeScale(effects, windowMs);
    for (const effect of effects) {
      if (effect.kind === "damage" || effect.kind === "kill") {
        this.active = this.active.filter((active) => active.kind !== "miss" || active.cell !== effect.cell);
      }
      this.active.push({
        ...effect,
        startedAt: now + (effect.delay ?? 0) * scale,
        duration: DURATIONS[effect.kind] * scale
      });
    }
    if (this.active.length > 40)
      this.active.splice(0, this.active.length - 40);
  }
  render(renderer, now) {
    const dpr = Math.min(devicePixelRatio || 1, 1.5);
    const width = Math.max(1, Math.round(this.canvas.clientWidth * dpr));
    const height = Math.max(1, Math.round(this.canvas.clientHeight * dpr));
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
    const ctx = this.ctx;
    ctx.clearRect(0, 0, width, height);
    this.active = this.active.filter((effect) => now < effect.startedAt + effect.duration);
    const ordered = [...this.active].sort((a, b) => effectPriority(a.kind) - effectPriority(b.kind));
    for (const effect of ordered) {
      if (now < effect.startedAt)
        continue;
      const muzzle = effect.kind === "tracer" ? renderer.projectPlayerMuzzle() : null;
      const point = muzzle ?? renderer.projectCell(effect.cell);
      if (!point)
        continue;
      const t = Math.min(1, (now - effect.startedAt) / effect.duration);
      drawEffect(ctx, effect, point, effect.targetCell === undefined ? null : renderer.projectCell(effect.targetCell), t, width, height, Boolean(muzzle));
    }
  }
}
function drawEffect(ctx, effect, point, target, t, width, height, exactSource = false) {
  const scale = point.scale / EFFECT_DESKTOP_SCENE_SCALE;
  ctx.save();
  const alpha = t < 0.68 ? 1 : 1 - (t - 0.68) / 0.32;
  ctx.globalAlpha = Math.max(0, alpha);
  if (effect.kind === "victory")
    drawTerminalEffect(ctx, point, t, scale, width, height, true);
  else if (effect.kind === "defeat")
    drawTerminalEffect(ctx, point, t, scale, width, height, false);
  else if (effect.kind === "tracer" && target)
    drawTracer(ctx, point, target, t, scale, false, exactSource);
  else if (effect.kind === "throw" && target)
    drawTracer(ctx, point, target, t, scale, true);
  else if (effect.kind === "teleport-out" || effect.kind === "teleport-in")
    drawTeleport(ctx, point, t, scale, effect.kind === "teleport-in");
  else if (effect.kind === "contact")
    drawRings(ctx, point, t, scale, "#73c8d6", 3);
  else {
    const color = effect.color ?? (effect.kind === "heal" ? "#9adf91" : effect.kind === "kill" || effect.kind === "level" ? "#e4c15d" : effect.kind === "reward" || effect.kind === "status" ? "#73c8d6" : "#ff655f");
    if (effect.kind === "player-damage")
      drawVignette(ctx, t, width, height);
    if (["damage", "player-damage", "kill"].includes(effect.kind))
      drawBurst(ctx, point, t, scale, color, effect.magnitude ?? 0);
    else if (effect.kind === "miss")
      drawMiss(ctx, point, t, scale, color);
    else
      drawRings(ctx, point, t, scale, color, effect.kind === "level" ? 3 : 2);
  }
  ctx.restore();
}
function effectPriority(kind) {
  return kind === "victory" || kind === "defeat" ? 12 : kind === "player-damage" ? 10 : kind === "kill" || kind === "level" ? 8 : kind === "damage" || kind === "heal" ? 6 : 2;
}
function drawTerminalEffect(ctx, point, t, scale, width, height, victory) {
  const primary = victory ? "#e4c15d" : "#ff655f", secondary = victory ? "#73c8d6" : "#a22d36";
  const opening = Math.min(1, t / 0.42), fade = t < 0.78 ? 1 : 1 - (t - 0.78) / 0.22;
  const wash = ctx.createRadialGradient(point.x, point.y - 36 * scale, 0, point.x, point.y - 36 * scale, Math.max(width, height) * 0.72);
  wash.addColorStop(0, victory ? `rgba(228,193,93,${0.24 * opening * fade})` : `rgba(255,70,62,${0.2 * opening * fade})`);
  wash.addColorStop(0.44, victory ? `rgba(48,160,154,${0.1 * opening * fade})` : `rgba(75,4,12,${0.18 * opening * fade})`);
  wash.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = wash;
  ctx.fillRect(0, 0, width, height);
  if (!victory)
    drawVignette(ctx, Math.min(0.38, t * 0.38), width, height);
  ctx.globalAlpha *= fade;
  for (let ring = 0;ring < 4; ring++) {
    const phase = Math.max(0, Math.min(1, opening - ring * 0.14));
    if (!phase)
      continue;
    ctx.strokeStyle = ring % 2 ? secondary : primary;
    ctx.lineWidth = Math.max(1, (4 - phase * 3) * scale);
    ctx.globalAlpha = fade * (1 - phase * 0.72);
    ctx.beginPath();
    ctx.ellipse(point.x, point.y - 25 * scale, (18 + phase * 115) * scale, (7 + phase * 42) * scale, 0, 0, Math.PI * 2);
    ctx.stroke();
  }
  const particles = victory ? 24 : 18;
  for (let index = 0;index < particles; index++) {
    const angle = index / particles * Math.PI * 2 + (victory ? -Math.PI / 2 : Math.PI / 2);
    const speed = (34 + index % 6 * 9) * scale, travel = Math.sin(Math.min(1, t * 1.7) * Math.PI / 2) * speed;
    const drift = victory ? -t * (28 + index % 4 * 8) * scale : t * (18 + index % 5 * 7) * scale;
    const x = point.x + Math.cos(angle) * travel, y = point.y - 34 * scale + Math.sin(angle) * travel + drift;
    ctx.fillStyle = index % 3 ? primary : secondary;
    ctx.globalAlpha = fade * (0.5 + index % 4 * 0.12);
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle + t * 4);
    const size = (3 + index % 3) * scale;
    if (victory)
      ctx.fillRect(-size / 2, -size, size, size * 2);
    else {
      ctx.beginPath();
      ctx.moveTo(0, -size * 1.5);
      ctx.lineTo(size, size);
      ctx.lineTo(-size, size);
      ctx.closePath();
      ctx.fill();
    }
    ctx.restore();
  }
}
function drawBurst(ctx, point, t, scale, color, magnitude) {
  const pulse = Math.min(1, t / 0.32), radius = (10 + Math.min(16, magnitude) * 0.8 + 34 * pulse) * scale;
  ctx.strokeStyle = color;
  ctx.lineWidth = Math.max(1, (4 - pulse * 3) * scale);
  ctx.globalAlpha *= 1 - pulse * 0.55;
  ctx.beginPath();
  ctx.arc(point.x, point.y - 35 * scale, radius, 0, Math.PI * 2);
  ctx.stroke();
  for (let index = 0;index < 10; index++) {
    const angle = index / 10 * Math.PI * 2, inner = radius * 0.45, outer = radius * (1.05 + index % 3 * 0.16);
    ctx.beginPath();
    ctx.moveTo(point.x + Math.cos(angle) * inner, point.y - 35 * scale + Math.sin(angle) * inner);
    ctx.lineTo(point.x + Math.cos(angle) * outer, point.y - 35 * scale + Math.sin(angle) * outer);
    ctx.stroke();
  }
  const pips = Math.max(0, Math.min(20, Math.round(magnitude)));
  ctx.fillStyle = color;
  ctx.globalAlpha = Math.max(0.15, 1 - t);
  for (let index = 0;index < pips; index++) {
    const angle = index / Math.max(1, pips) * Math.PI * 2 - Math.PI / 2;
    const orbit = (22 + index % 2 * 8 + pulse * 22) * scale;
    const x = point.x + Math.cos(angle) * orbit, y = point.y - 35 * scale + Math.sin(angle) * orbit;
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle + Math.PI / 4);
    ctx.fillRect(-2.5 * scale, -2.5 * scale, 5 * scale, 5 * scale);
    ctx.restore();
  }
}
function drawMiss(ctx, point, t, scale, color) {
  const pulse = Math.min(1, t / 0.4), radius = (12 + 28 * pulse) * scale, y = point.y - 28 * scale;
  ctx.strokeStyle = color;
  ctx.lineWidth = Math.max(1, (3 - pulse * 2) * scale);
  ctx.globalAlpha *= 1 - pulse * 0.55;
  ctx.beginPath();
  ctx.arc(point.x, y, radius, 0, Math.PI * 2);
  ctx.stroke();
  const arm = radius * 0.55;
  ctx.beginPath();
  ctx.moveTo(point.x - arm, y - arm);
  ctx.lineTo(point.x + arm, y + arm);
  ctx.moveTo(point.x + arm, y - arm);
  ctx.lineTo(point.x - arm, y + arm);
  ctx.stroke();
  ctx.globalAlpha = Math.max(0, 1 - t * 1.4);
  ctx.fillStyle = "rgba(255,190,184,.82)";
  ctx.textAlign = "center";
  ctx.textBaseline = "bottom";
  ctx.font = `700 ${Math.max(7, 11 * scale)}px "Courier New",monospace`;
  ctx.fillText("MISS", point.x, y - radius - 4 * scale);
}
function drawRings(ctx, point, t, scale, color, count) {
  ctx.strokeStyle = color;
  ctx.lineWidth = Math.max(1, 2 * scale);
  for (let index = 0;index < count; index++) {
    const phase = Math.max(0, Math.min(1, t * 1.5 - index * 0.13));
    ctx.globalAlpha *= Math.max(0.15, 1 - phase);
    ctx.beginPath();
    ctx.ellipse(point.x, point.y - 14 * scale, (12 + phase * 42) * scale, (5 + phase * 17) * scale, 0, 0, Math.PI * 2);
    ctx.stroke();
  }
}
function drawTracer(ctx, from, to, t, scale, arc, exactSource = false) {
  const progress = Math.min(1, t), x = from.x + (to.x - from.x) * progress;
  const sourceY = exactSource ? from.y : from.y - 38 * scale;
  const targetY = to.y - 38 * scale;
  const baseY = sourceY + (targetY - sourceY) * progress, y = arc ? baseY - Math.sin(progress * Math.PI) * 75 * scale : baseY;
  ctx.strokeStyle = arc ? "#e4c15d" : "#73e6ff";
  ctx.lineWidth = Math.max(1, (arc ? 3 : 2) * scale);
  ctx.shadowColor = ctx.strokeStyle;
  ctx.shadowBlur = 10 * scale;
  ctx.beginPath();
  ctx.moveTo(from.x, sourceY);
  if (arc)
    ctx.quadraticCurveTo((from.x + to.x) / 2, Math.min(from.y, to.y) - 100 * scale, x, y);
  else
    ctx.lineTo(x, y);
  ctx.stroke();
  ctx.shadowBlur = 0;
  ctx.fillStyle = ctx.strokeStyle;
  ctx.beginPath();
  ctx.arc(x, y, Math.max(1.5, 4 * scale), 0, Math.PI * 2);
  ctx.fill();
}
function drawTeleport(ctx, point, t, scale, arriving) {
  const phase = arriving ? t : 1 - t;
  const centerY = point.y - 38 * scale;
  const glow = 0.35 + Math.sin(Math.min(1, t) * Math.PI) * 0.65;
  ctx.save();
  ctx.globalCompositeOperation = "screen";
  ctx.shadowColor = "#73e6ff";
  ctx.shadowBlur = (12 + glow * 22) * scale;
  const column = ctx.createLinearGradient(point.x, point.y - 105 * scale, point.x, point.y + 4 * scale);
  column.addColorStop(0, "rgba(115,230,255,0)");
  column.addColorStop(0.35, `rgba(115,230,255,${0.12 + glow * 0.2})`);
  column.addColorStop(0.72, `rgba(174,118,255,${0.1 + glow * 0.2})`);
  column.addColorStop(1, "rgba(174,118,255,0)");
  ctx.fillStyle = column;
  ctx.fillRect(point.x - (8 + 14 * glow) * scale, point.y - 108 * scale, (16 + 28 * glow) * scale, 112 * scale);
  for (let ring = 0;ring < 4; ring++) {
    const stagger = Math.max(0, Math.min(1, t * 1.35 - ring * 0.1));
    const radius = arriving ? 48 - stagger * 34 : 14 + stagger * 38;
    ctx.strokeStyle = ring % 2 ? "#b98aff" : "#73e6ff";
    ctx.lineWidth = Math.max(1, (3 - stagger * 1.8) * scale);
    ctx.globalAlpha = Math.max(0.08, (1 - stagger * 0.72) * glow);
    ctx.beginPath();
    ctx.ellipse(point.x, point.y - 8 * scale, radius * scale, (5 + radius * 0.28) * scale, 0, 0, Math.PI * 2);
    ctx.stroke();
  }
  ctx.globalAlpha = 0.35 + glow * 0.65;
  for (let index = 0;index < 18; index++) {
    const angle = index / 18 * Math.PI * 2 + t * (arriving ? -2.5 : 3.5);
    const orbit = (10 + index % 5 * 7 + (1 - phase) * 25) * scale;
    const lift = (index % 6 * 15 - 42) * scale;
    const x = point.x + Math.cos(angle) * orbit;
    const y = centerY + lift + (arriving ? -1 : 1) * (1 - phase) * 28 * scale;
    const size = (1.5 + index % 3) * scale;
    ctx.fillStyle = index % 3 ? "#73e6ff" : "#c397ff";
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle);
    ctx.fillRect(-size / 2, -size * 2.2, size, size * 4.4);
    ctx.restore();
  }
  ctx.restore();
}
function drawVignette(ctx, t, width, height) {
  const strength = Math.max(0, 1 - t * 2.4) * 0.48;
  const gradient = ctx.createRadialGradient(width / 2, height / 2, Math.min(width, height) * 0.18, width / 2, height / 2, Math.max(width, height) * 0.68);
  gradient.addColorStop(0, "rgba(130,0,0,0)");
  gradient.addColorStop(1, `rgba(235,45,35,${strength})`);
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, width, height);
}

// src/runtime/log-layout.ts
function storedLogLayout(value) {
  return value === "below" ? "below" : "side";
}
function nextLogLayout(value) {
  return value === "side" ? "below" : "side";
}
function logLayoutButtonState(layout) {
  return { label: layout === "side" ? "Side" : "Below", pressed: layout === "side" };
}

// src/shared/hud.ts
function renderHudView(view) {
  const hpPct = percent(view.hp, view.maxHp);
  return [
    `<div class="hud-vital" title="Health ${view.hp}/${view.maxHp}"><span class="hud-icon hud-icon-hp" aria-hidden="true"></span><div class="hud-bar" aria-label="Health ${hpPct}%"><span style="width:${hpPct}%"></span></div><strong>${view.hp}/${view.maxHp}</strong></div>`,
    chip("floor", view.floor, view.floorTitle),
    chip("agent", view.agent, view.agentTitle),
    chip("weapon", compactWeaponName(view.weapon), view.weaponTitle),
    chip("damage", view.damage, view.damageTitle),
    chip("range", view.range, view.rangeTitle),
    chip("ammo", view.ammo, view.ammoTitle),
    chip(view.armorWarning ? "warn" : "armor", view.armor, view.armorTitle),
    meter("xp", view.xpPercent, view.xpTitle, view.level),
    chip("credits", view.credits, "Credits"),
    chip(view.nutritionWarning ? "warn" : "food", view.nutrition, view.nutritionTitle),
    view.skillPoints ? chip("skill", view.skillPoints, "Skill points available") : "",
    view.effects ? chip("warn", view.effects, "Active effects") : ""
  ].filter(Boolean).join("");
}
function renderPregameHud(options = {}) {
  const label = options.mode === "ended" ? options.title ?? "Mission ended" : "Select agent";
  return [
    `<div class="hud-vital hud-vital-empty"><span class="hud-icon hud-icon-hp" aria-hidden="true"></span><div class="hud-bar" aria-label="Health preview"><span style="width:100%"></span></div><strong>Ready</strong></div>`,
    chip("agent", label, "Choose a field profile"),
    chip("floor", "F1", "HQ Evidence Lockdown"),
    chip("damage", "-", "Damage updates when a weapon is wielded"),
    chip("range", "-", "Range updates when a weapon is wielded"),
    chip("ammo", "-", "Ammo depends on starting gear"),
    chip("weapon", "Gear", "Starting gear depends on selected agent")
  ].join("");
}
function chip(kind, value, title) {
  return `<span class="hud-chip hud-${kind}" title="${esc(title)}"><span class="hud-icon hud-icon-${kind}" aria-hidden="true"></span><strong>${esc(value)}</strong></span>`;
}
function meter(kind, pct, title, value) {
  return `<span class="hud-chip hud-${kind}" title="${esc(title)}"><span class="hud-icon hud-icon-${kind}" aria-hidden="true"></span><span class="hud-mini"><span style="width:${pct}%"></span></span><strong>${esc(value)}</strong></span>`;
}
function compactWeaponName(name) {
  const known = { "noisy cricket": "Cricket", "standard pistol": "Pistol", "prototype zapper": "Zapper", "series 4 de-atomizer": "S4 De-Atom.", "reverberating carbonizer w/ mutate capacity": "Rev. Carbon.", "tri-barrel plasma gun": "Tri-Plasma", "bone spur": "Spur", "stun baton": "Baton", "arquillian saber": "Saber", "sugar-water cannon": "Sugar Gun" };
  const compact = known[name.toLowerCase()] ?? name.replace(/^prototype\s+/i, "").replace(/^standard\s+/i, "").split(/\s+/).slice(0, 2).join(" ");
  return compact.length > 12 ? `${compact.slice(0, 11).trimEnd()}.` : compact;
}
function percent(value, max) {
  return max <= 0 ? 0 : Math.max(0, Math.min(100, Math.round(value / max * 100)));
}
function esc(value) {
  return value.replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch]);
}

// src/shared/inventory-primary-action.ts
function inventoryPrimaryAction(kind) {
  if (kind === "weapon")
    return "wield";
  if (kind === "armor")
    return "wear";
  if (kind === "food" || kind === "pill")
    return "eat";
  if (kind === "tool")
    return "use";
  if (kind === "thrown")
    return "aim-throw";
  return null;
}

// src/isometric-rust-wasm-browser.ts
var canvasHost = document.querySelector("#canvasHost");
var stats = document.querySelector("#perfStats");
var status = document.querySelector("#status");
var history = document.querySelector("#history");
var seedInput = document.querySelector("#seed");
var classSelect = document.querySelector("#class");
var rendererSelect = document.querySelector("#renderer");
var fpsInput = document.querySelector("#fps");
var fpsValue = document.querySelector("#fpsValue");
var fpsDefaultButton = document.querySelector("#fpsDefault");
var plannerCoresInput = document.querySelector("#plannerCores");
var plannerCoresValue = document.querySelector("#plannerCoresValue");
var plannerCoresMax = document.querySelector("#plannerCoresMax");
var plannerStrengthSelect = document.querySelector("#plannerStrength");
var plannerStrengthValue = document.querySelector("#plannerStrengthValue");
var plannerStrengthHint = document.querySelector("#plannerStrengthHint");
var startButton = document.querySelector("#start");
var stepButton = document.querySelector("#step");
var autoplayButton = document.querySelector("#autoplay");
var resetButton = document.querySelector("#reset");
var newRunButton = document.querySelector("#newRun");
var endingNewButton = document.querySelector("#endingNew");
var hud = document.querySelector("#stats");
var ending = document.querySelector("#ending");
var endingTitle = document.querySelector("#endingTitle");
var endingBody = document.querySelector("#endingBody");
var classPicker = document.querySelector("#classPicker");
var settingsModal = document.querySelector("#settingsModal");
var settingsButton = document.querySelector("#settingsButton");
var settingsClose = document.querySelector("#settingsClose");
function commandButton(icon, label) {
  return `<span class="btn-icon icon-${icon}"></span><span>${label}</span>`;
}
var layoutLogButton = document.querySelector("#layoutLog");
var helpButton = document.querySelector("#helpButton");
var helpModal = document.querySelector("#helpModal");
var helpClose = document.querySelector("#helpClose");
var inventoryModal = document.querySelector("#inventoryModal");
var inventoryList = document.querySelector("#inventoryList");
var inventoryHint = document.querySelector("#inventoryHint");
var shopModal = document.querySelector("#shopModal");
var shopList = document.querySelector("#shopList");
var planningModal = document.querySelector("#planningModal");
var inventorySelection = 0;
var shopSelection = 0;
var targetMode = null;
var recommendationId = 0;
var recommendationWaiters = new Map;
var DEFAULT_AUTOPLAY_FPS = 6;
var MAX_MANUAL_FPS = 6;
var REPORTED_LOGICAL_CPUS = Math.max(1, navigator.hardwareConcurrency || 2);
var PLANNER_CORE_LIMIT = Math.max(1, Math.floor(REPORTED_LOGICAL_CPUS / 2));
var DEFAULT_PLANNER_CORES = Math.min(4, PLANNER_CORE_LIMIT);
var REFERENCE_BASELINE_MS = 84;
var CALIBRATION_TURN_CAP = 600;
var CALIBRATION_BUDGET_MS = 850;
var STRONGEST_MIN_RELATIVE_SPEED = 0.7;
var BALANCED_MIN_RELATIVE_SPEED = 0.35;
var DEFAULT_PLANNER_STRENGTH = "strongest";
var MOB_APPEAR_MS = 280;
var MOB_DEATH_MS = 760;
var ENDING_PRELUDE_MS = 1250;
var resumeAfterSettings = false;
var actionLog = [];
function applyLogLayout(layout) {
  const button = logLayoutButtonState(layout);
  document.body.dataset.logLayout = layout;
  layoutLogButton.setAttribute("aria-pressed", String(button.pressed));
  layoutLogButton.innerHTML = commandButton("below", button.label);
}
if (new URLSearchParams(location.search).has("e2e")) {
  Object.defineProperty(window, "__MIB_RUST_E2E__", { value: {
    snapshot: () => simulationSnapshot,
    presented: () => snapshot,
    pose: () => snapshot ? presentedPlayerPose(snapshot, (presentationPausedAt ?? performance.now()) - presentationStarted) : null,
    effectAt: (cell) => effectLayer?.hasActiveAtCell(cell) ?? false,
    log: () => actionLog.map((entry) => ({ ...entry })),
    idle: () => !pending,
    planning: () => planningTelemetry && { ...planningTelemetry, candidateMs: [...planningTelemetry.candidateMs] },
    project: (cell) => renderer?.projectCell(cell) ?? null,
    mobRenderable: (uid) => simulationSnapshot?.mobs.some((mob) => mob.uid === uid && shouldRenderMob(simulationSnapshot, mob)) ?? false,
    seek: (frame) => worker.postMessage({ type: "seek", frame }),
    act: (signature) => worker.postMessage({ type: "action", signature }),
    previewTeleport: () => previewTeleportForAudit(),
    recommend: () => new Promise((resolve) => {
      const requestId = ++recommendationId;
      recommendationWaiters.set(requestId, resolve);
      worker.postMessage({ type: "recommend", requestId });
    })
  }, configurable: true });
}
function appendActionLog(text, cls) {
  const last = actionLog[actionLog.length - 1];
  if (last?.text === text && last.cls === cls)
    last.repeat += 1;
  else
    actionLog.push({ text, cls, repeat: 1 });
  actionLog = actionLog.slice(-120);
  renderActionLog();
  status.hidden = true;
}
function renderActionLog() {
  const visibleLines = document.body.dataset.logLayout === "side" && window.innerWidth > 860 ? 16 : 4;
  history.replaceChildren(...actionLog.slice(-visibleLines).map((entry) => {
    const row = document.createElement("div");
    row.textContent = entry.repeat > 1 ? `${entry.text} ×${entry.repeat}` : entry.text;
    if (entry.cls)
      row.className = entry.cls;
    return row;
  }));
}
var embeddedWorkerUrl = globalThis.__MIB_RUST_WORKER_URL__;
function createSimulationWorker() {
  return embeddedWorkerUrl ? new Worker(embeddedWorkerUrl) : new Worker("./dist/isometric-rust-worker.js?v=20260721.1", { type: "module" });
}
var worker = createSimulationWorker();
var renderer = null;
var effectLayer = null;
var effectsBySnapshot = new WeakMap;
var snapshot = null;
var simulationSnapshot = null;
var visualQueue = [];
var visualTimer = null;
var visualEndsAt = 0;
var visualRemaining = 0;
var presentationPausedAt = null;
var autoplay = false;
var pending = false;
var planning = false;
var manualRequestPending = false;
var lastManualMovementAt = -Infinity;
var presentationDurations = new WeakMap;
var projectilePreviews = new WeakSet;
var teleportPreviews = new WeakSet;
var loggedSnapshots = new WeakSet;
var initMs = 0;
var wasmMs = 0;
var snapshotMs = 0;
var renderMs = 0;
var renderedFrames = 0;
var totalRenderMs = 0;
var presentationStarted = 0;
var lastStatsAt = 0;
var endingFrame = -1;
var endingRevealAt = 0;
var planningTelemetry = null;
var planningBeganAt = 0;
function resetEndingSequence() {
  endingFrame = -1;
  endingRevealAt = 0;
  ending.classList.remove("visible", "complete", "failed");
}
function prepareEndingSequence(next, now) {
  if (!next.won && !next.dead) {
    resetEndingSequence();
    return;
  }
  if (endingFrame === next.frame)
    return;
  endingFrame = next.frame;
  endingRevealAt = now + ENDING_PRELUDE_MS;
  ending.classList.remove("visible");
  ending.classList.toggle("complete", next.won);
  ending.classList.toggle("failed", next.dead);
}
function playbackFps() {
  return Math.max(1, Math.min(60, Number(fpsInput.value) || DEFAULT_AUTOPLAY_FPS));
}
function syncFpsSetting(value = fpsInput.value) {
  const fps = Math.max(1, Math.min(60, Number(value) || DEFAULT_AUTOPLAY_FPS));
  fpsInput.value = String(fps);
  fpsValue.value = `${fps} FPS`;
  fpsInput.setAttribute("aria-valuetext", `${fps} frames per second`);
}
function syncPlannerCoreSetting(value = plannerCoresInput.value) {
  const cores = Math.max(1, Math.min(PLANNER_CORE_LIMIT, Number(value) || DEFAULT_PLANNER_CORES));
  plannerCoresInput.max = String(PLANNER_CORE_LIMIT);
  plannerCoresInput.value = String(cores);
  plannerCoresValue.value = `${cores} ${cores === 1 ? "core" : "cores"}`;
  plannerCoresMax.textContent = `${PLANNER_CORE_LIMIT} ${PLANNER_CORE_LIMIT === 1 ? "core" : "cores"} max`;
  plannerCoresInput.setAttribute("aria-valuetext", `${cores} physical CPU ${cores === 1 ? "core" : "cores"}`);
}
function plannerCoreCount() {
  return Math.max(1, Math.min(PLANNER_CORE_LIMIT, Number(plannerCoresInput.value) || DEFAULT_PLANNER_CORES));
}
function syncPlannerStrengthSetting(value = plannerStrengthSelect.value) {
  const setting = value === "adaptive" || value === "baseline" || value === "balanced" || value === "strongest" ? value : DEFAULT_PLANNER_STRENGTH;
  plannerStrengthSelect.value = setting;
  const cached = adaptivePlannerCalibration();
  plannerStrengthValue.value = setting === "adaptive" ? cached ? `Adaptive → ${plannerLevelName(cached.level)}` : "Adaptive" : plannerLevelName(setting);
  plannerStrengthHint.firstElementChild.textContent = setting === "adaptive" ? cached ? `Benchmarked: ${plannerLevelName(cached.level)}` : "Benchmarked automatically" : setting === "baseline" ? "No tree search" : setting === "balanced" ? "Narrower tree search" : "Complete current search";
}
function plannerLevelName(level) {
  return level === "baseline" ? "Quick" : level === "balanced" ? "Tactical" : "Strategic";
}
function adaptivePlannerCalibration() {
  try {
    const value = JSON.parse(localStorage.getItem("mib_rust_planner_calibration_v2") || "null");
    if (value?.logicalCpus !== REPORTED_LOGICAL_CPUS || !["baseline", "balanced", "strongest"].includes(value?.level))
      return null;
    return { level: value.level, baselineMs: Number(value.baselineMs) };
  } catch {
    return null;
  }
}
function calibratedPlannerLevel(baselineMs) {
  const relativeSpeed = REFERENCE_BASELINE_MS / Math.max(1, baselineMs);
  if (relativeSpeed >= STRONGEST_MIN_RELATIVE_SPEED)
    return "strongest";
  if (relativeSpeed >= BALANCED_MIN_RELATIVE_SPEED)
    return "balanced";
  return "baseline";
}
function storePlannerCalibration(level, baselineMs) {
  localStorage.setItem("mib_rust_planner_calibration_v2", JSON.stringify({ logicalCpus: REPORTED_LOGICAL_CPUS, level, baselineMs }));
}
function selectedPlannerLevel() {
  const setting = plannerStrengthSelect.value;
  return setting === "adaptive" ? adaptivePlannerCalibration()?.level ?? null : setting;
}
function frameDuration() {
  const fps = playbackFps();
  return fps ? 1000 / fps : 0;
}
function manualMovementRepeatDuration() {
  return 1000 / Math.min(playbackFps(), MAX_MANUAL_FPS);
}
function framesPerWalkPose() {
  return 0.5;
}
function enhanceHudEquipmentIcon(selector, gear, kind) {
  if (gear < 0)
    return;
  const icon = hud.querySelector(selector);
  if (!icon)
    return;
  const cell = 26 + gear, x = cell % 32 * 32, y = Math.floor(cell / 32) * 32;
  const chip2 = icon.closest(".hud-chip");
  if (kind === "weapon")
    chip2?.classList.add("hud-asset-backed");
  else
    chip2?.classList.add("hud-armor-equipped");
  icon.className = "hud-atlas-icon hud-item-icon-wrap";
  applyEmbeddedHudAtlas(icon);
  icon.style.backgroundPosition = `-${x}px -${y}px`;
}
function applyEmbeddedHudAtlas(root) {
  const atlasUrl = globalThis.__MIB_RUST_EMBEDDED_ASSETS__?.atlasUrl;
  if (!atlasUrl)
    return;
  const icons = root instanceof HTMLElement && root.classList.contains("hud-atlas-icon") ? [root] : [...root.querySelectorAll(".hud-atlas-icon")];
  for (const icon of icons)
    icon.style.backgroundImage = `url("${atlasUrl}")`;
}
function inventoryIcon(gear) {
  const cell = 26 + gear, x = cell % 32 * 32, y = Math.floor(cell / 32) * 32;
  return `<span class="hud-atlas-icon" style="background-position:-${x}px -${y}px"></span>`;
}
function renderShop() {
  const items = simulationSnapshot?.shop ?? [];
  shopList.innerHTML = items.map((item, index) => `<div class="inventory-row${index === shopSelection ? " selected" : ""}">${inventoryIcon(item.gear)}<span>${item.name}</span><b>${item.price} cr</b></div>`).join("") || "No stock.";
  applyEmbeddedHudAtlas(shopList);
}
function renderInventory() {
  const items = simulationSnapshot?.inventory ?? [];
  inventorySelection = Math.max(0, Math.min(items.length - 1, inventorySelection));
  const selected = items[inventorySelection];
  const primary = selected ? inventoryPrimaryAction(selected.kind) : null;
  const primaryLabel = selected?.wielded ? "Already wielded" : selected?.worn ? "Already worn" : primary === "wield" ? "Wield selected" : primary === "wear" ? "Wear selected" : primary === "eat" ? "Consume selected" : primary === "use" ? "Use selected" : primary === "aim-throw" ? "Aim selected throw" : "No primary action";
  inventoryHint.innerHTML = `${primary && !selected?.wielded && !selected?.worn ? "<kbd>Enter</kbd> " : ""}${primaryLabel} · <kbd>J</kbd><kbd>K</kbd> select · <kbd>W</kbd> wield · <kbd>Shift W</kbd> wear · <kbd>E</kbd> eat · <kbd>U</kbd> use · <kbd>Shift T</kbd> aim throw · <kbd>Esc</kbd> close`;
  inventoryList.innerHTML = items.length ? items.map((item, index) => `<div class="inventory-row${index === inventorySelection ? " selected" : ""}">${inventoryIcon(item.gear)}<strong>${item.name}${item.count > 1 ? ` (${item.count})` : ""}</strong><span class="inventory-meta">${item.wielded ? "wielded" : item.worn ? "worn" : item.kind}</span></div>`).join("") : "Empty pockets.";
  applyEmbeddedHudAtlas(inventoryList);
}
function closeInventory() {
  inventoryModal.classList.remove("visible");
}
function closeGameplayOverlays() {
  closeInventory();
  shopModal.classList.remove("visible");
  targetMode = null;
}
function missionEnded() {
  return !simulationSnapshot || simulationSnapshot.won || simulationSnapshot.dead;
}
function visualDuration(next) {
  const base = presentationDurations.get(next) ?? frameDuration();
  if (projectilePreviews.has(next) || teleportPreviews.has(next))
    return base;
  if (next.mobs.some((mob) => mob.state === 7))
    return Math.max(base, MOB_DEATH_MS);
  const appearDuration = next.mobs.some((mob) => mob.appeared) ? MOB_APPEAR_MS : 0;
  return Math.max(base, appearDuration);
}
function attachEffects(next, effects) {
  if (effects.length)
    effectsBySnapshot.set(next, effects);
}
function enqueueTeleportPresentation(before, next, effects) {
  const transition = stageTeleportTransition(before, next);
  if (!transition)
    return false;
  const { departure, arrival } = splitTeleportEffects(effects);
  attachEffects(transition.departure, departure);
  presentationDurations.set(transition.departure, TELEPORT_OUT_MS);
  teleportPreviews.add(transition.departure);
  attachEffects(transition.arrival, arrival);
  presentationDurations.set(transition.arrival, TELEPORT_IN_MS);
  visualQueue.push(transition.departure, transition.arrival);
  pumpVisualQueue();
  return true;
}
function previewTeleportForAudit() {
  if (!simulationSnapshot)
    return null;
  const before = simulationSnapshot;
  const sourceX = before.player.cell % before.width;
  const sourceY = Math.floor(before.player.cell / before.width);
  const destination = [...before.seen].map((seen, cell) => ({ seen, cell })).filter(({ seen, cell }) => seen === "1" && before.map[cell] !== "#").sort((left, right) => {
    const distance = (cell) => Math.max(Math.abs(cell % before.width - sourceX), Math.abs(Math.floor(cell / before.width) - sourceY));
    return distance(right.cell) - distance(left.cell);
  })[0]?.cell;
  if (destination === undefined || destination === before.player.cell)
    return null;
  const next = {
    ...before,
    frame: before.frame + 1,
    action: "use:pocket universe marble",
    logs: [{ text: "Teleported.", cls: "good" }],
    player: { ...before.player, cell: destination, fromCell: destination, teleported: true }
  };
  enqueueTeleportPresentation(before, next, deriveRustIsoEffects(before, next));
  return { source: before.player.cell, destination };
}
async function replaceRenderer() {
  renderer?.destroy();
  effectLayer?.clear();
  const installCanvases = () => {
    const canvas2 = document.createElement("canvas");
    canvas2.id = "stage";
    canvas2.width = 1100;
    canvas2.height = 680;
    const effectCanvas = document.createElement("canvas");
    effectCanvas.id = "effects";
    effectCanvas.width = 1100;
    effectCanvas.height = 680;
    canvasHost.replaceChildren(canvas2, effectCanvas);
    effectLayer = new RustIsoEffectLayer(effectCanvas);
    return canvas2;
  };
  let canvas = installCanvases();
  status.textContent = "Calibrating tactical display…";
  try {
    renderer = await createRustIsoRenderer(canvas, rendererSelect.value);
  } catch (error) {
    if (rendererSelect.value !== "webgl2")
      throw error;
    console.warn("Enhanced renderer unavailable; using compatible renderer.", error);
    rendererSelect.value = "canvas2d";
    canvas = installCanvases();
    renderer = await createRustIsoRenderer(canvas, "canvas2d");
  }
  document.body.dataset.renderer = renderer.kind;
  status.textContent = renderer.kind === "canvas2d" ? "Tactical display online (compatible mode)." : "Tactical display online.";
  if (snapshot)
    draw(snapshot);
}
function preferCompatibleMobileRenderer() {
  return /Android|Mobile/i.test(navigator.userAgent) || matchMedia("(pointer: coarse)").matches;
}
function randomSeed() {
  const requested = Number(new URLSearchParams(location.search).get("seed"));
  if (Number.isInteger(requested) && requested > 0 && requested <= 4294967295)
    return requested;
  const value = new Uint32Array(1);
  crypto.getRandomValues(value);
  return value[0] || 1;
}
function showClassPicker() {
  if (autoplay)
    autoplayButton.click();
  resetEndingSequence();
  classPicker.classList.add("visible");
  status.hidden = false;
  hud.innerHTML = renderPregameHud({ mode: "classpick" });
  status.textContent = "Select an agent profile to begin.";
}
function draw(next, transitionMs = 1000, now = performance.now()) {
  snapshot = next;
  if (!renderer)
    return;
  const duration = visualDuration(next);
  const progress = duration ? Math.min(1, transitionMs / duration) : 1;
  const presentationFrame = next.frame + progress;
  const walkPose = Math.floor(presentationFrame / framesPerWalkPose());
  const gameClock = walkPose * 100;
  const rendererTransition = progress * 90;
  renderMs = renderer.render(next, rendererTransition, gameClock);
  totalRenderMs += renderMs;
  renderedFrames++;
  if (now - lastStatsAt < 250 && next.frame < next.frameCount)
    return;
  lastStatsAt = now;
  const nutrition = next.player.nutrition <= 0 ? "Starving" : next.player.nutrition <= 300 ? "Hungry" : "Fed";
  hud.innerHTML = renderHudView({
    hp: next.player.hp,
    maxHp: next.player.maxHp,
    floor: String(next.floor),
    floorTitle: `Floor ${next.floor}`,
    agent: String.fromCharCode(next.player.agent),
    agentTitle: `L${next.player.level}`,
    weapon: next.player.weapon,
    weaponTitle: next.player.weapon,
    damage: `${next.player.damageMin}-${next.player.damageMax}`,
    damageTitle: `Damage ${next.player.damageMin}-${next.player.damageMax}`,
    range: `R${next.player.range}`,
    rangeTitle: `Range ${next.player.range}`,
    ammo: String(next.player.ammo),
    ammoTitle: `${next.player.ammo} matching ammo`,
    armor: `AC${next.player.armor}`,
    armorTitle: `Armor ${next.player.armor}`,
    xpPercent: next.player.xpNext ? next.player.xp / next.player.xpNext * 100 : 0,
    xpTitle: `XP ${next.player.xp}/${next.player.xpNext}`,
    level: `L${next.player.level}`,
    credits: `$${next.player.credits}`,
    nutrition,
    nutritionTitle: `Nutrition ${next.player.nutrition}`,
    nutritionWarning: next.player.nutrition <= 300
  });
  enhanceHudEquipmentIcon(".hud-icon-weapon", next.player.weaponGear, "weapon");
  enhanceHudEquipmentIcon(".hud-icon-armor", next.player.armorGear, "armor");
  if (next.won || next.dead) {
    closeGameplayOverlays();
    if (endingFrame === next.frame && now >= endingRevealAt)
      ending.classList.add("visible");
    endingTitle.textContent = next.won ? "Assignment Complete" : "Assignment Failed";
    endingBody.innerHTML = [
      ["Score", next.score],
      ["Floor", next.floor],
      ["Kills", next.player.kills],
      ["Turns", next.turns]
    ].map(([label, value]) => `<span class="ending-stat"><i class="ending-icon ending-icon-${String(label).toLowerCase()}"></i><small>${label}</small><strong>${value}</strong></span>`).join("");
  } else if (autoplay)
    status.textContent = `F${next.floor} · Turn ${next.turns} · ${next.action}`;
  stats.textContent = [
    `mode         ${autoplay ? "autoplay" : "manual"}`,
    `display      tactical projection`,
    `seed/class   ${next.seed}/${next.class}`,
    `frame        ${next.frame}/${next.frameCount}`,
    `steps/sec    ${playbackFps() || "max"}`,
    `walk hold    ${framesPerWalkPose()} frames/pose`,
    `turn/floor   ${next.turns}/${next.floor}`,
    `hp           ${next.player.hp}/${next.player.maxHp}`,
    `score/kills  ${next.score}/${next.player.kills}`,
    `outcome      ${next.won ? "WIN" : next.dead ? "DEAD" : "running"}`,
    `orders       ${next.policy === "human" ? "manual control" : next.policy && next.policy !== "unplanned" ? "route secured" : "awaiting orders"}`,
    `mission prep ${initMs.toFixed(1)} ms`,
    `field sync   ${snapshotMs.toFixed(3)} ms`,
    `display      ${renderMs.toFixed(3)} ms`,
    `display avg  ${(totalRenderMs / renderedFrames).toFixed(3)} ms`
  ].join(`
`);
  stepButton.disabled = next.frame >= next.frameCount;
  if (next.won || next.dead || next.frame >= next.frameCount) {
    autoplay = false;
    autoplayButton.innerHTML = commandButton("auto", "Auto");
    autoplayButton.setAttribute("aria-pressed", "false");
  }
}
function present(next) {
  presentationStarted = performance.now();
  snapshot = next;
  if (!projectilePreviews.has(next) && !teleportPreviews.has(next) && !loggedSnapshots.has(next)) {
    loggedSnapshots.add(next);
    for (const line of next.logs)
      appendActionLog(line.text, line.cls);
  }
  prepareEndingSequence(next, presentationStarted);
  const duration = visualDuration(next);
  effectLayer?.play(effectsBySnapshot.get(next) ?? [], presentationStarted, duration || undefined);
  effectsBySnapshot.delete(next);
  if (duration)
    draw(next, 0, presentationStarted);
  else
    draw(next, 1000, presentationStarted);
}
function settlePresentationForManualInput(next) {
  const now = performance.now();
  snapshot = next;
  if (!projectilePreviews.has(next) && !teleportPreviews.has(next) && !loggedSnapshots.has(next)) {
    loggedSnapshots.add(next);
    for (const line of next.logs)
      appendActionLog(line.text, line.cls);
  }
  prepareEndingSequence(next, now);
  effectsBySnapshot.delete(next);
  effectLayer?.clear();
  presentationStarted = now - 1000;
  draw(next, 1000, now);
}
function pumpVisualQueue() {
  if (presentationPausedAt !== null || visualTimer !== null || !visualQueue.length)
    return;
  const next = visualQueue.shift();
  present(next);
  const duration = visualDuration(next);
  if (duration) {
    visualEndsAt = performance.now() + duration;
    visualTimer = window.setTimeout(completeVisualFrame, duration);
  } else
    queueMicrotask(pumpVisualQueue);
}
function requestAutoplayAfterPresentation() {
  if (!autoplay || pending || presentationPausedAt !== null || visualTimer !== null || visualQueue.length)
    return;
  requestNext();
}
function completeVisualFrame() {
  visualTimer = null;
  visualRemaining = 0;
  if (!visualQueue.length && snapshot?.mobs.some((mob) => mob.state === 7)) {
    const defeatedCells = snapshot.mobs.filter((mob) => mob.state === 7).map((mob) => mob.cell);
    effectLayer?.clearCells(defeatedCells);
    present({ ...snapshot, logs: [], mobs: snapshot.mobs.filter((mob) => mob.state !== 7) });
  }
  pumpVisualQueue();
  requestAutoplayAfterPresentation();
}
function animationLoop(now) {
  const presentationNow = presentationPausedAt ?? now;
  if (snapshot)
    draw(snapshot, presentationNow - presentationStarted, presentationNow);
  if (renderer)
    effectLayer?.render(renderer, presentationNow);
  requestAnimationFrame(animationLoop);
}
function requestNext() {
  if (pending || !simulationSnapshot || simulationSnapshot.won || simulationSnapshot.dead || simulationSnapshot.frame >= simulationSnapshot.frameCount)
    return;
  pending = true;
  if (!simulationSnapshot.policy || simulationSnapshot.policy === "unplanned" || simulationSnapshot.policy === "human") {
    if (!planning)
      planningBeganAt = performance.now();
    planning = true;
    planningModal.classList.add("visible");
    planningModal.setAttribute("aria-busy", "true");
    document.body.setAttribute("aria-busy", "true");
  }
  worker.postMessage({ type: "next" });
}
function requestManualAction(signature) {
  if (autoplay || pending || !simulationSnapshot || simulationSnapshot.won || simulationSnapshot.dead)
    return false;
  visualQueue = [];
  if (visualTimer !== null)
    window.clearTimeout(visualTimer);
  visualTimer = null;
  visualRemaining = 0;
  presentationPausedAt = null;
  settlePresentationForManualInput(simulationSnapshot);
  pending = true;
  manualRequestPending = true;
  worker.postMessage({ type: "action", signature });
  return true;
}
function nearestVisibleHostile(next) {
  const px = next.player.cell % next.width, py = Math.floor(next.player.cell / next.width);
  return next.mobs.filter((mob) => !mob.friendly && next.visible[mob.cell] === "1").sort((a, b) => {
    const ax = a.cell % next.width, ay = Math.floor(a.cell / next.width), bx = b.cell % next.width, by = Math.floor(b.cell / next.width);
    return Math.max(Math.abs(ax - px), Math.abs(ay - py)) - Math.max(Math.abs(bx - px), Math.abs(by - py));
  })[0];
}
function workerRequest(helper, request, expected) {
  return new Promise((resolve, reject) => {
    const receive = (event) => {
      if (event.data.type === "error") {
        cleanup();
        reject(new Error(event.data.message));
        return;
      }
      if (event.data.type !== expected)
        return;
      cleanup();
      resolve(event.data);
    };
    const failed = () => {
      cleanup();
      reject(new Error("planning worker failed"));
    };
    const cleanup = () => {
      helper.removeEventListener("message", receive);
      helper.removeEventListener("error", failed);
    };
    helper.addEventListener("message", receive);
    helper.addEventListener("error", failed);
    helper.postMessage(request);
  });
}
function betterPlan(left, right) {
  const leftRank = [Number(left.won), left.deepest, left.primary, left.score];
  const rightRank = [Number(right.won), right.deepest, right.primary, right.score];
  for (let index = 0;index < leftRank.length; index++) {
    if (leftRank[index] !== rightRank[index])
      return leftRank[index] > rightRank[index];
  }
  return false;
}
async function evaluateInitialPlan(candidateCount) {
  const setting = plannerStrengthSelect.value;
  let level;
  let baselineMs;
  let calibrationMs;
  let calibrationTimedOut = false;
  const evaluations = [];
  const candidateMs = [];
  const evaluationStarted = performance.now();
  const seed = Number(seedInput.value) || 1704334;
  const cls = classSelect.value;
  const helpers = [];
  try {
    const cached = setting === "adaptive" ? adaptivePlannerCalibration() : null;
    if (setting !== "adaptive")
      level = setting;
    else if (cached) {
      level = cached.level;
      baselineMs = cached.baselineMs;
    } else {
      const calibrationWorker = createSimulationWorker();
      planningModal.querySelector("p").innerHTML = "Calibrating planning strategy for this CPU.<br>Stand by for deployment.";
      const calibrationStarted = performance.now();
      const timeout = Symbol("calibration-timeout");
      const result = await Promise.race([
        (async () => {
          await workerRequest(calibrationWorker, { type: "start", seed, cls }, "ready");
          const probeStarted = performance.now();
          await workerRequest(calibrationWorker, { type: "benchmark-plan", index: 0, turnCap: CALIBRATION_TURN_CAP }, "plan-evaluation");
          return performance.now() - probeStarted;
        })(),
        new Promise((resolve) => window.setTimeout(() => resolve(timeout), CALIBRATION_BUDGET_MS))
      ]);
      calibrationMs = performance.now() - calibrationStarted;
      calibrationWorker.terminate();
      if (result === timeout) {
        calibrationTimedOut = true;
        baselineMs = REFERENCE_BASELINE_MS / (BALANCED_MIN_RELATIVE_SPEED * 0.9);
        level = "baseline";
      } else {
        baselineMs = result;
        level = calibratedPlannerLevel(baselineMs);
      }
      storePlannerCalibration(level, baselineMs);
      plannerStrengthValue.value = `Adaptive → ${plannerLevelName(level)}`;
      plannerStrengthHint.firstElementChild.textContent = calibrationTimedOut ? "CPU probe capped: Quick" : `CPU score: ${(REFERENCE_BASELINE_MS / baselineMs * 100).toFixed(0)}%`;
    }
    const candidateIndices2 = level === "baseline" ? [0] : level === "balanced" ? Array.from({ length: Math.min(7, candidateCount) }, (_, index) => index) : Array.from({ length: candidateCount }, (_, index) => index);
    const coreCount2 = Math.min(plannerCoreCount(), candidateIndices2.length);
    planningModal.querySelector("p").innerHTML = `${plannerLevelName(level)} planning is evaluating ${candidateIndices2.length} ${candidateIndices2.length === 1 ? "route" : "routes"} on ${coreCount2} CPU ${coreCount2 === 1 ? "core" : "cores"}.<br>Stand by for deployment.`;
    while (helpers.length < coreCount2)
      helpers.push(createSimulationWorker());
    await Promise.all(helpers.map((helper, index) => index === 0 && evaluations[0] ? Promise.resolve() : workerRequest(helper, { type: "start", seed, cls }, "ready")));
    let nextCandidate = 0;
    await Promise.all(helpers.map(async (helper) => {
      while (nextCandidate < candidateIndices2.length) {
        const index = candidateIndices2[nextCandidate++];
        if (evaluations[index])
          continue;
        const candidateStarted = performance.now();
        const message = await workerRequest(helper, { type: "evaluate-plan", index }, "plan-evaluation");
        candidateMs[index] = performance.now() - candidateStarted;
        evaluations[index] = message.evaluation;
      }
    }));
  } finally {
    helpers.forEach((helper) => helper.terminate());
  }
  const candidateIndices = level === "baseline" ? [0] : level === "balanced" ? Array.from({ length: Math.min(7, candidateCount) }, (_, index) => index) : Array.from({ length: candidateCount }, (_, index) => index);
  if (candidateIndices.some((index) => !evaluations[index]))
    throw new Error("parallel planning returned an incomplete policy set");
  const coreCount = Math.min(plannerCoreCount(), candidateIndices.length);
  planningTelemetry = { cores: coreCount, candidates: candidateIndices.length, evaluationMs: performance.now() - evaluationStarted, candidateMs, level, baselineMs, calibrationMs, calibrationTimedOut };
  let best = evaluations[candidateIndices[0]];
  for (const index of candidateIndices.slice(1)) {
    if (betterPlan(evaluations[index], best))
      best = evaluations[index];
  }
  worker.postMessage({ type: "install-plan", index: best.index });
}
worker.onmessage = (event) => {
  const message = event.data;
  if (message.type === "recommendation") {
    recommendationWaiters.get(message.requestId)?.(message.signature);
    recommendationWaiters.delete(message.requestId);
    return;
  }
  if (message.type === "plan-needed") {
    const selectedLevel = selectedPlannerLevel();
    if (selectedLevel && (!simulationSnapshot || simulationSnapshot.frame > 0 || plannerCoreCount() === 1)) {
      planningModal.querySelector("p").innerHTML = `${plannerLevelName(selectedLevel)} planning is preparing the next orders.<br>Stand by for deployment.`;
      worker.postMessage({ type: "plan-strategy", strategy: selectedLevel });
    } else {
      evaluateInitialPlan(message.candidates).catch((error) => {
        pending = false;
        planning = false;
        planningModal.classList.remove("visible");
        planningModal.removeAttribute("aria-busy");
        document.body.removeAttribute("aria-busy");
        autoplay = false;
        autoplayButton.innerHTML = commandButton("auto", "Auto");
        status.textContent = error instanceof Error ? error.message : String(error);
      });
    }
    return;
  }
  if (message.type === "plan-installed") {
    worker.postMessage({ type: "next" });
    return;
  }
  pending = false;
  if (planning) {
    if (planningTelemetry && planningBeganAt)
      planningTelemetry.totalMs = performance.now() - planningBeganAt;
    planning = false;
    planningModal.classList.remove("visible");
    planningModal.removeAttribute("aria-busy");
    document.body.removeAttribute("aria-busy");
  }
  if (message.type === "error") {
    manualRequestPending = false;
    autoplay = false;
    status.textContent = message.message;
    return;
  }
  if (message.type === "ready") {
    initMs = message.initMs;
    wasmMs = message.wasmMs;
    snapshotMs = 0;
    renderedFrames = 0;
    totalRenderMs = 0;
    simulationSnapshot = message.snapshot;
    visualQueue = [];
    if (visualTimer !== null)
      window.clearTimeout(visualTimer);
    visualTimer = null;
    visualRemaining = 0;
    presentationPausedAt = null;
    effectLayer?.clear();
    resetEndingSequence();
    actionLog = [];
    appendActionLog("NEURALYZED: A Men In Black Roguelike", "good");
    appendActionLog("A steady cleanup specialist sent to make the incident disappear.");
    appendActionLog("F1: HQ Evidence Lockdown. HQ locks down after an offworld breach.");
    appendActionLog(`Agent ${String.fromCharCode(message.snapshot.player.agent)} reports for duty.`, "good");
    appendActionLog("Field console ready. Open Help for controls.");
    present(message.snapshot);
    status.textContent = `Mission ready · Agent ${String.fromCharCode(message.snapshot.player.agent)} awaiting deployment.`;
  } else {
    snapshotMs = message.snapshotMs;
    const effects = deriveRustIsoEffects(simulationSnapshot, message.snapshot);
    const teleportTransition = stageTeleportTransition(simulationSnapshot, message.snapshot);
    const projectilePreview = stageRangedImpactTransition(simulationSnapshot, message.snapshot);
    if (teleportTransition) {
      enqueueTeleportPresentation(simulationSnapshot, message.snapshot, effects);
    } else if (projectilePreview) {
      const { travel, impact } = splitRangedEffects(effects);
      const isThrow = message.snapshot.action.startsWith("throw:");
      const currentFrameDuration = frameDuration();
      const travelDuration = isThrow ? THROW_EFFECT_MS : Math.min(TRACER_EFFECT_MS, Math.max(1, currentFrameDuration * 0.45));
      attachEffects(projectilePreview, travel);
      presentationDurations.set(projectilePreview, travelDuration);
      projectilePreviews.add(projectilePreview);
      attachEffects(message.snapshot, impact);
      if (!isThrow)
        presentationDurations.set(message.snapshot, Math.max(1, currentFrameDuration - travelDuration));
      visualQueue.push(projectilePreview, message.snapshot);
    } else {
      attachEffects(message.snapshot, effects);
      visualQueue.push(message.snapshot);
    }
    simulationSnapshot = message.snapshot;
    manualRequestPending = false;
    pumpVisualQueue();
  }
};
startButton.addEventListener("click", () => {
  autoplay = false;
  autoplayButton.innerHTML = commandButton("auto", "Auto");
  autoplayButton.setAttribute("aria-pressed", "false");
  resetEndingSequence();
  presentationPausedAt = null;
  planningTelemetry = null;
  planningBeganAt = 0;
  pending = true;
  status.textContent = "Preparing field assignment…";
  worker.postMessage({ type: "start", seed: Number(seedInput.value) || 1704334, cls: classSelect.value, e2e: new URLSearchParams(location.search).has("e2e") });
});
stepButton.addEventListener("click", requestNext);
resetButton.addEventListener("click", () => {
  autoplay = false;
  autoplayButton.innerHTML = commandButton("auto", "Auto");
  worker.postMessage({ type: "reset" });
});
autoplayButton.addEventListener("click", () => {
  autoplay = !autoplay;
  autoplayButton.innerHTML = commandButton(autoplay ? "stop" : "auto", autoplay ? "Stop" : "Auto");
  autoplayButton.setAttribute("aria-pressed", String(autoplay));
  if (autoplay) {
    closeGameplayOverlays();
    const now = performance.now();
    if (presentationPausedAt !== null)
      presentationStarted += now - presentationPausedAt;
    presentationPausedAt = null;
    if (visualRemaining > 0) {
      visualEndsAt = now + visualRemaining;
      visualTimer = window.setTimeout(completeVisualFrame, visualRemaining);
    } else
      pumpVisualQueue();
    status.textContent = "Autoplay started.";
    requestAutoplayAfterPresentation();
  } else {
    presentationPausedAt = performance.now();
    if (visualTimer !== null) {
      visualRemaining = Math.max(0, visualEndsAt - presentationPausedAt);
      window.clearTimeout(visualTimer);
      visualTimer = null;
    }
    status.textContent = "Mission paused.";
  }
});
newRunButton.addEventListener("click", showClassPicker);
endingNewButton.addEventListener("click", showClassPicker);
classPicker.addEventListener("click", (event) => {
  const button = event.target instanceof Element ? event.target.closest("[data-class]") : null;
  if (!button?.dataset.class)
    return;
  classSelect.value = button.dataset.class;
  seedInput.value = String(randomSeed());
  classPicker.classList.remove("visible");
  startButton.click();
});
settingsButton.addEventListener("click", () => {
  resumeAfterSettings = autoplay;
  if (autoplay)
    autoplayButton.click();
  settingsModal.classList.add("visible");
});
settingsClose.addEventListener("click", () => {
  settingsModal.classList.remove("visible");
  localStorage.setItem("mib_rust_steps_per_second", fpsInput.value);
  localStorage.setItem("mib_rust_planner_cores", plannerCoresInput.value);
  localStorage.setItem("mib_rust_planner_strength", plannerStrengthSelect.value);
  if (resumeAfterSettings)
    autoplayButton.click();
  resumeAfterSettings = false;
});
fpsInput.addEventListener("input", () => {
  syncFpsSetting(fpsInput.value);
});
fpsDefaultButton.addEventListener("click", (event) => {
  event.preventDefault();
  event.stopPropagation();
  syncFpsSetting(String(DEFAULT_AUTOPLAY_FPS));
});
plannerCoresInput.addEventListener("input", () => {
  syncPlannerCoreSetting(plannerCoresInput.value);
});
plannerStrengthSelect.addEventListener("change", () => {
  syncPlannerStrengthSetting(plannerStrengthSelect.value);
});
layoutLogButton.addEventListener("click", () => {
  const layout = nextLogLayout(document.body.dataset.logLayout);
  applyLogLayout(layout);
  localStorage.setItem("mib_rust_log_layout", layout);
  renderActionLog();
  if (snapshot)
    draw(snapshot, 1000, performance.now());
});
helpButton.addEventListener("click", () => {
  resumeAfterSettings = autoplay;
  if (autoplay)
    autoplayButton.click();
  helpModal.classList.add("visible");
});
helpClose.addEventListener("click", () => {
  helpModal.classList.remove("visible");
  if (resumeAfterSettings)
    autoplayButton.click();
  resumeAfterSettings = false;
});
document.addEventListener("keydown", (event) => {
  if (planning) {
    event.preventDefault();
    event.stopImmediatePropagation();
    return;
  }
  if (classPicker.classList.contains("visible") && /^[arvtm]$/i.test(event.key)) {
    classPicker.querySelector(`[data-class="${event.key.toLowerCase()}"]`)?.click();
  }
  if (event.key === "Escape") {
    if (settingsModal.classList.contains("visible"))
      settingsClose.click();
    if (helpModal.classList.contains("visible"))
      helpClose.click();
  }
  if (event.key === "?" && !classPicker.classList.contains("visible")) {
    event.preventDefault();
    if (helpModal.classList.contains("visible"))
      helpClose.click();
    else
      helpButton.click();
    return;
  }
  if (classPicker.classList.contains("visible") || settingsModal.classList.contains("visible") || helpModal.classList.contains("visible"))
    return;
  if (missionEnded() || autoplay) {
    closeGameplayOverlays();
    return;
  }
  if (shopModal.classList.contains("visible")) {
    event.preventDefault();
    const items = simulationSnapshot?.shop ?? [];
    if (event.key === "Escape") {
      shopModal.classList.remove("visible");
      return;
    }
    if (event.key === "j" || event.key === "ArrowDown") {
      shopSelection = Math.min(items.length - 1, shopSelection + 1);
      renderShop();
      return;
    }
    if (event.key === "k" || event.key === "ArrowUp") {
      shopSelection = Math.max(0, shopSelection - 1);
      renderShop();
      return;
    }
    if (event.key === "Enter" && items[shopSelection]) {
      const name = items[shopSelection].name;
      shopModal.classList.remove("visible");
      requestManualAction(`buy:${name}`);
    }
    return;
  }
  if (inventoryModal.classList.contains("visible")) {
    const items = simulationSnapshot?.inventory ?? [];
    if (event.key === "Escape" || event.key === "i") {
      event.preventDefault();
      closeInventory();
      return;
    }
    if (event.key === "j" || event.key === "ArrowDown") {
      event.preventDefault();
      inventorySelection = Math.min(items.length - 1, inventorySelection + 1);
      renderInventory();
      return;
    }
    if (event.key === "k" || event.key === "ArrowUp") {
      event.preventDefault();
      inventorySelection = Math.max(0, inventorySelection - 1);
      renderInventory();
      return;
    }
    const item = items[inventorySelection];
    if (!item)
      return;
    if (event.key === "Enter") {
      event.preventDefault();
      if (item.wielded || item.worn)
        return;
      const primary = inventoryPrimaryAction(item.kind);
      if (primary === "aim-throw") {
        closeInventory();
        targetMode = { action: "throw", item: item.name, cell: simulationSnapshot.player.cell };
        status.hidden = false;
        status.innerHTML = "\uD83C\uDFAF Aim throw · <kbd>Arrows</kbd> move · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel";
        return;
      }
      const action2 = primary === "wield" ? `wield:${item.name}` : primary === "wear" ? `wear:${item.name}` : primary === "eat" ? `eat:${item.name}` : primary === "use" ? `use:${item.name}` : null;
      if (action2) {
        closeInventory();
        requestManualAction(action2);
      }
      return;
    }
    if (event.key === "T") {
      event.preventDefault();
      closeInventory();
      targetMode = { action: "throw", item: item.name, cell: simulationSnapshot.player.cell };
      status.hidden = false;
      status.innerHTML = "\uD83C\uDFAF Aim throw · <kbd>Arrows</kbd> move · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel";
      return;
    }
    const action = event.key === "w" ? `wield:${item.name}` : event.key === "W" ? `wear:${item.name}` : event.key === "e" ? `eat:${item.name}` : event.key === "u" ? `use:${item.name}` : null;
    if (action) {
      event.preventDefault();
      closeInventory();
      requestManualAction(action);
      return;
    }
    return;
  }
  if (targetMode && simulationSnapshot) {
    event.preventDefault();
    if (event.key === "Escape") {
      targetMode = null;
      status.hidden = true;
      return;
    }
    if (event.key === "Enter" || event.key === "f") {
      const x = targetMode.cell % simulationSnapshot.width, y = Math.floor(targetMode.cell / simulationSnapshot.width);
      const signature = targetMode.action === "fire" ? `fire:${x},${y}` : `throw:${targetMode.item}:${x},${y}`;
      targetMode = null;
      status.hidden = true;
      requestManualAction(signature);
      return;
    }
    const moves = { ArrowLeft: [-1, 0], h: [-1, 0], ArrowRight: [1, 0], l: [1, 0], ArrowUp: [0, -1], k: [0, -1], ArrowDown: [0, 1], j: [0, 1], y: [-1, -1], u: [1, -1], b: [-1, 1], n: [1, 1] };
    const move = moves[event.key];
    if (move) {
      const x = Math.max(0, Math.min(simulationSnapshot.width - 1, targetMode.cell % simulationSnapshot.width + move[0]));
      const y = Math.max(0, Math.min(simulationSnapshot.height - 1, Math.floor(targetMode.cell / simulationSnapshot.width) + move[1]));
      targetMode.cell = y * simulationSnapshot.width + x;
      status.hidden = false;
      status.innerHTML = `\uD83C\uDFAF Target ${x},${y} · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel`;
    }
    return;
  }
  if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement)
    return;
  const movement = { ArrowLeft: "h", ArrowDown: "j", ArrowUp: "k", ArrowRight: "l", h: "h", j: "j", k: "k", l: "l", y: "y", u: "u", b: "b", n: "n" };
  const key = movement[event.key];
  if (key) {
    event.preventDefault();
    const now = performance.now();
    if (now - lastManualMovementAt < manualMovementRepeatDuration())
      return;
    if (requestManualAction(`command:${key}`))
      lastManualMovementAt = now;
    return;
  }
  if (event.key === "." || event.key === " ") {
    event.preventDefault();
    requestManualAction("command:.");
    return;
  }
  if (event.key === "g") {
    event.preventDefault();
    requestManualAction("command:g");
    return;
  }
  if (event.key === "i") {
    event.preventDefault();
    inventorySelection = 0;
    renderInventory();
    inventoryModal.classList.add("visible");
    return;
  }
  if ((event.key === "p" || event.key === "Enter") && simulationSnapshot?.map[simulationSnapshot.player.cell] === "_" && simulationSnapshot.shop.length) {
    event.preventDefault();
    shopSelection = 0;
    renderShop();
    shopModal.classList.add("visible");
    return;
  }
  if (event.key === "Enter" && simulationSnapshot) {
    event.preventDefault();
    const tile = simulationSnapshot.map[simulationSnapshot.player.cell];
    requestManualAction(`command:${tile === "<" ? "<" : ">"}`);
    return;
  }
  if (event.key === "F" && simulationSnapshot) {
    event.preventDefault();
    targetMode = { action: "fire", cell: simulationSnapshot.player.cell };
    status.hidden = false;
    status.innerHTML = "\uD83C\uDFAF Aim fire · <kbd>Arrows</kbd> move · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel";
    return;
  }
  if (event.key === "f" && simulationSnapshot) {
    const target = nearestVisibleHostile(simulationSnapshot);
    if (target) {
      event.preventDefault();
      requestManualAction(`fire:${target.cell % simulationSnapshot.width},${Math.floor(target.cell / simulationSnapshot.width)}`);
    }
    return;
  }
  if (["c", "B", "P"].includes(event.key)) {
    event.preventDefault();
    requestManualAction(`command:${event.key}`);
  }
});
rendererSelect.addEventListener("change", () => void replaceRenderer());
window.addEventListener("resize", () => {
  renderActionLog();
  if (snapshot)
    draw(snapshot, 1000, performance.now());
});
if (preferCompatibleMobileRenderer())
  rendererSelect.value = "canvas2d";
await replaceRenderer();
requestAnimationFrame(animationLoop);
syncFpsSetting(localStorage.getItem("mib_rust_steps_per_second"));
syncPlannerCoreSetting(localStorage.getItem("mib_rust_planner_cores"));
syncPlannerStrengthSetting(localStorage.getItem("mib_rust_planner_strength"));
applyLogLayout(storedLogLayout(localStorage.getItem("mib_rust_log_layout")));
status.textContent = "Select an agent profile to begin.";
hud.innerHTML = renderPregameHud({ mode: "classpick" });
document.body.dataset.rustReady = "true";
