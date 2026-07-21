import type { RustRenderSnapshot } from "../runtime/rust-wasm-protocol";

type AtlasMeta = {
  version: 1;
  width: number;
  height: number;
  cellSize: number;
  columns: number;
  anchors: Array<[number, number]>;
  muzzles?: Array<[number, number] | null>;
};

type Draw = { cell: number; x: number; y: number; scale: number; z: number; anchor?: [number, number]; opacity?: number; muzzle?: [number, number] };
type CameraState = { x: number; y: number; zoom: number; floor: number; entryFrame: number; initialized: boolean; lastAt: number };
type ViewTransform = { ox: number; oy: number; sceneScale: number; cameraX: number; cameraY: number; width: number };
export type RustIsoScreenPoint = { x: number; y: number; scale: number };
export type RustIsoRendererKind = "webgl2" | "canvas2d";

const LOGICAL_W = 2304;
const LOGICAL_H = 1248;
const TILE_W = 96;
const TILE_H = 48;
const VIEW_W = 23;
const VIEW_H = 23;
// The logical camera intentionally retains a 23x23 safety margin for smooth
// following, but the presentation crops that margin so explored rooms use the
// available panel instead of floating in a large black field.
const ATLAS_COLUMNS = 32;
const ATLAS_CELL = 128;
const TILE_CELLS: Record<string, number> = { "#": 1, "+": 2, "'": 3, ">": 4, "<": 5, "_": 6, "*": 7, "%": 8, "^": 9, "~": 10 };
const CLASS_INDEX = { a: 0, r: 1, v: 2, t: 3, m: 4 } as const;
const PLAYER_BASE = 58;
const MOB_BASE = 258;
const PLAYER_DIRECTION_STRIDE = 10;
const PLAYER_CLASS_STRIDE = 40;
const PLAYER_STATE_OFFSETS = [0, 1, 6, 7, 8, 9];
const FRIENDLY_PLAYER_STATE_OFFSETS = [0, 1, 7, 8, 0, 0, 0, 9];
const MOB_DIRECTION_STRIDE = 12;
const MOB_KIND_STRIDE = 48;
const MOB_STATE_OFFSETS = [0, 1, 6, 7, 8, 9, 10, 11];
const FLOOR_TILES = new Set([10]);
// Above-floor map props share the actor band so walls, the shop, and the altar
// depth-sort against agents and mobs instead of being painted as floor decals.
const ACTOR_TILES = new Set([1, 2, 6, 7]);
const IMMOBILIZED_MOB_STATES = new Set([4, 5, 6]);

function zKey(x: number, y: number, band: number, order: number) {
  return band * 1_000_000_000 + (x + y) * 100_000 + y * 1_000 + order;
}

export interface RustIsoRenderer {
  readonly kind: RustIsoRendererKind;
  render(snapshot: RustRenderSnapshot, transitionMs?: number, clockMs?: number): number;
  projectCell(cell: number): RustIsoScreenPoint | null;
  projectPlayerMuzzle(): RustIsoScreenPoint | null;
  destroy(): void;
}

function projectCell(view: ViewTransform | null, cell: number): RustIsoScreenPoint | null {
  if (!view) return null;
  const x = cell % view.width, y = Math.floor(cell / view.width);
  return {
    x: view.ox + (LOGICAL_W / 2 + ((x - view.cameraX) - (y - view.cameraY)) * TILE_W / 2) * view.sceneScale,
    y: view.oy + (72 + ((x - view.cameraX) + (y - view.cameraY)) * TILE_H / 2) * view.sceneScale,
    scale: view.sceneScale,
  };
}

type EmbeddedRustAssets = { atlasUrl: string; atlasMeta: AtlasMeta };

export async function createRustIsoRenderer(canvas: HTMLCanvasElement, preference: RustIsoRendererKind): Promise<RustIsoRenderer> {
  const embedded = (globalThis as typeof globalThis & { __MIB_RUST_EMBEDDED_ASSETS__?: EmbeddedRustAssets }).__MIB_RUST_EMBEDDED_ASSETS__;
  const [image, meta] = await Promise.all([
    loadImage(embedded?.atlasUrl ?? "./dist/rust-wasm/iso-atlas.png?v=20260721.1"),
    embedded ? Promise.resolve(embedded.atlasMeta) : fetch("./dist/rust-wasm/iso-atlas-meta.json?v=20260721.1", { cache: "no-store" }).then((response) => response.json() as Promise<AtlasMeta>),
  ]);
  if (preference === "webgl2") {
    const gl = canvas.getContext("webgl2", { alpha: false, antialias: false, depth: false, preserveDrawingBuffer: false });
    if (gl && gl.getParameter(gl.MAX_TEXTURE_SIZE) >= Math.max(meta.width, meta.height)) return new WebGlAtlasRenderer(canvas, gl, image, meta);
  }
  return new CanvasAtlasRenderer(canvas, image, meta);
}

function loadImage(src: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(`failed to load ${src}`));
    image.src = src;
  });
}

export function cameraTarget(snapshot: RustRenderSnapshot, transitionMs: number, enteringFloor = false) {
  // A floor-change snapshot may carry fromCell from the stairs on the previous
  // map. That coordinate has no visual relationship to the new scene. Entry
  // must therefore focus the authoritative destination immediately instead of
  // interpolating across two unrelated floor coordinate spaces.
  const progress = enteringFloor ? 1 : Math.max(0, Math.min(1, transitionMs / 90));
  const fromX = snapshot.player.fromCell % snapshot.width, fromY = Math.floor(snapshot.player.fromCell / snapshot.width);
  const toX = snapshot.player.cell % snapshot.width, toY = Math.floor(snapshot.player.cell / snapshot.width);
  const playerX = fromX + (toX - fromX) * progress, playerY = fromY + (toY - fromY) * progress;
  const targetX = Math.max(0, Math.min(snapshot.width - VIEW_W, playerX - Math.floor(VIEW_W / 2)));
  const targetY = Math.max(0, Math.min(snapshot.height - VIEW_H, playerY - Math.floor(VIEW_H / 2)));
  let minX=snapshot.width,maxX=0,minY=snapshot.height,maxY=0;
  for(let cell=0;cell<snapshot.seen.length;cell++)if(snapshot.seen[cell]==="1"){
    const x=cell%snapshot.width,y=Math.floor(cell/snapshot.width);
    if(x>=targetX&&x<targetX+VIEW_W&&y>=targetY&&y<targetY+VIEW_H){minX=Math.min(minX,x);maxX=Math.max(maxX,x);minY=Math.min(minY,y);maxY=Math.max(maxY,y);}
  }
  const spanX=maxX>=minX?maxX-minX+1:VIEW_W,spanY=maxY>=minY?maxY-minY+1:VIEW_H;
  const targetZoom=Math.max(1.12,Math.min(2.25,Math.min(VIEW_W/(spanX+1),VIEW_H/(spanY+1))));
  return { x: targetX, y: targetY, zoom: targetZoom };
}

function cameraOrigin(state: CameraState, snapshot: RustRenderSnapshot, transitionMs: number) {
  const enteringFloor = !state.initialized || state.floor !== snapshot.floor;
  const target = cameraTarget(snapshot, transitionMs, enteringFloor);
  const progress = Math.max(0, Math.min(1, transitionMs / 90));
  const presentedAt = snapshot.frame + progress;
  if (enteringFloor || snapshot.player.teleported) {
    state.x = target.x; state.y = target.y; state.zoom = target.zoom; state.floor = snapshot.floor; state.entryFrame = enteringFloor ? snapshot.frame : -1; state.initialized = true;
  } else {
    // Follow presentation progress, not wall time or the quantized walk-sprite
    // clock. PNG encoding can take arbitrarily long without changing motion.
    const elapsedFrames = Math.min(1, Math.max(0, presentedAt - state.lastAt));
    const follow = 1 - Math.exp(-elapsedFrames / 0.52);
    state.x += (target.x - state.x) * follow;
    state.y += (target.y - state.y) * follow;
    state.zoom += (target.zoom - state.zoom) * follow;
  }
  state.lastAt = presentedAt;
  return state;
}

export function presentedPlayerPose(snapshot: RustRenderSnapshot, transitionMs: number) {
  // The snapshot direction is authoritative throughout combat animations.
  // Only a completed walk may need its pose settled toward a corpse that is
  // deliberately held on screen. Otherwise the 90 ms walk boundary could
  // redirect the final frames of a ranged attack toward another dead mob.
  if (transitionMs < 90 || snapshot.player.state !== 1) return { state: snapshot.player.state, direction: snapshot.player.direction };
  const dead = snapshot.mobs
    .filter((mob) => mob.state === 7)
    .sort((left, right) => {
      const distance = (cell: number) => {
        const x = cell % snapshot.width, y = Math.floor(cell / snapshot.width);
        const px = snapshot.player.cell % snapshot.width, py = Math.floor(snapshot.player.cell / snapshot.width);
        return Math.max(Math.abs(x - px), Math.abs(y - py));
      };
      return distance(left.cell) - distance(right.cell);
    })[0];
  if (!dead) return { state: snapshot.player.state, direction: snapshot.player.direction };
  const px = snapshot.player.cell % snapshot.width, py = Math.floor(snapshot.player.cell / snapshot.width);
  const tx = dead.cell % snapshot.width, ty = Math.floor(dead.cell / snapshot.width);
  const dx = tx - px, dy = ty - py;
  const direction = Math.abs(dx) >= Math.abs(dy) ? (dx >= 0 ? 1 : 3) : (dy >= 0 ? 2 : 0);
  return { state: 0, direction };
}

export function presentedMobAtlasCell(snapshot: RustRenderSnapshot, mob: RustRenderSnapshot["mobs"][number], animationFrame = 0) {
  if (mob.friendly) {
    // Backup agents have no independent sprite family in the source catalog.
    // Never fall back to the active player's exact appearance: use the next
    // profile in the atlas so a squadmate cannot read as a duplicated player.
    const backupClass = (CLASS_INDEX[snapshot.class] + 1) % Object.keys(CLASS_INDEX).length;
    return PLAYER_BASE + backupClass * PLAYER_CLASS_STRIDE + mob.direction * PLAYER_DIRECTION_STRIDE
      + FRIENDLY_PLAYER_STATE_OFFSETS[mob.state] + animationFrame;
  }
  return MOB_BASE + mob.kind * MOB_KIND_STRIDE + mob.direction * MOB_DIRECTION_STRIDE
    + MOB_STATE_OFFSETS[mob.state] + animationFrame;
}

export function shouldRenderMob(snapshot: RustRenderSnapshot, mob: RustRenderSnapshot["mobs"][number]) {
  if (snapshot.visible[mob.cell] === "1") return true;
  if (!mob.spotted || snapshot.seen[mob.cell] !== "1" || !IMMOBILIZED_MOB_STATES.has(mob.state)) return false;
  const x = mob.cell % snapshot.width, y = Math.floor(mob.cell / snapshot.width);
  const px = snapshot.player.cell % snapshot.width, py = Math.floor(snapshot.player.cell / snapshot.width);
  return Math.max(Math.abs(x - px), Math.abs(y - py)) <= 9;
}

function commands(snapshot: RustRenderSnapshot, meta: AtlasMeta, transitionMs: number, clockMs: number, camera: CameraState): Draw[] {
  const transitionProgress = Math.max(0, Math.min(1, transitionMs / 90));
  const px = snapshot.player.cell % snapshot.width;
  const py = Math.floor(snapshot.player.cell / snapshot.width);
  const x0 = Math.floor(camera.x);
  const y0 = Math.floor(camera.y);
  const x1 = Math.min(snapshot.width, x0 + VIEW_W + (camera.x % 1 ? 1 : 0));
  const y1 = Math.min(snapshot.height, y0 + VIEW_H + (camera.y % 1 ? 1 : 0));
  const result: Draw[] = [];
  const project = (x: number, y: number) => ({
    x: LOGICAL_W / 2 + ((x - camera.x) - (y - camera.y)) * TILE_W / 2,
    y: 72 + ((x - camera.x) + (y - camera.y)) * TILE_H / 2,
  });
  const animatedPoint = (cell: number, fromCell: number, state: number) => {
    const tx = cell % snapshot.width, ty = Math.floor(cell / snapshot.width);
    if (camera.entryFrame === snapshot.frame || state !== 1 || fromCell === cell) return { ...project(tx, ty), gridX: tx, gridY: ty };
    const fx = fromCell % snapshot.width, fy = Math.floor(fromCell / snapshot.width);
    const progress = Math.min(1, transitionMs / 90);
    const gridX = fx + (tx - fx) * progress, gridY = fy + (ty - fy) * progress;
    return { ...project(gridX, gridY), gridX, gridY };
  };
  const animationFrame = (state: number) => state === 1 ? Math.floor(clockMs / 100) % 5 : 0;
  const walkAnchor = (firstCell: number, state: number): [number, number] | undefined => {
    if (state !== 1) return undefined;
    const anchors = meta.anchors.slice(firstCell, firstCell + 5);
    return [anchors.reduce((sum, anchor) => sum + anchor[0], 0) / anchors.length, anchors.reduce((sum, anchor) => sum + anchor[1], 0) / anchors.length];
  };
  for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) {
    const index = y * snapshot.width + x;
    if (snapshot.seen[index] !== "1") continue;
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
    if (snapshot.seen[item.cell] !== "1") continue;
    const x = item.cell % snapshot.width, y = Math.floor(item.cell / snapshot.width);
    if (x < x0 || x >= x1 || y < y0 || y >= y1) continue;
    result.push({ cell: 26 + item.gear, ...project(x, y), scale: 0.7, z: zKey(x, y, 3, 20) });
  }
  for (const mob of snapshot.mobs) {
    if (!shouldRenderMob(snapshot, mob)) continue;
    const x = mob.cell % snapshot.width, y = Math.floor(mob.cell / snapshot.width);
    if (x < x0 || x >= x1 || y < y0 || y >= y1) continue;
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
  const playerOpacity = snapshot.player.teleportPhase === "in"
      ? Math.min(1, teleportProgress * 1.45)
      : 1;
  const playerCell = PLAYER_BASE + CLASS_INDEX[snapshot.class] * PLAYER_CLASS_STRIDE + playerPose.direction * PLAYER_DIRECTION_STRIDE + PLAYER_STATE_OFFSETS[playerPose.state] + animationFrame(playerPose.state);
  result.push({
    cell: playerCell,
    x: playerPosition.x,
    y: playerPosition.y,
    scale: 1,
    opacity: playerOpacity,
    z: zKey(playerPosition.gridX, playerPosition.gridY, 3, 30) + 5,
    anchor: walkAnchor(PLAYER_BASE + CLASS_INDEX[snapshot.class] * PLAYER_CLASS_STRIDE + playerPose.direction * PLAYER_DIRECTION_STRIDE + PLAYER_STATE_OFFSETS[playerPose.state], playerPose.state),
    muzzle: meta.muzzles?.[playerCell] ?? undefined,
  });
  return result.sort((a, b) => a.z - b.z || a.y - b.y || a.x - b.x).filter((draw) => meta.anchors[draw.cell]);
}

function resize(canvas: HTMLCanvasElement) {
  const dpr = Math.min(devicePixelRatio || 1, 1.5);
  const width = Math.max(1, Math.round(canvas.clientWidth * dpr));
  const height = Math.max(1, Math.round(canvas.clientHeight * dpr));
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  return { width, height, scale: Math.min(width / LOGICAL_W, height / LOGICAL_H) };
}

class CanvasAtlasRenderer implements RustIsoRenderer {
  readonly kind = "canvas2d" as const;
  private ctx: CanvasRenderingContext2D;
  private camera: CameraState = { x: 0, y: 0, zoom: 1.12, floor: 0, entryFrame: -1, initialized: false, lastAt: performance.now() };
  private view: ViewTransform | null = null;
  private playerMuzzle: RustIsoScreenPoint | null = null;
  constructor(private canvas: HTMLCanvasElement, private image: HTMLImageElement, private meta: AtlasMeta) {
    this.ctx = canvas.getContext("2d", { alpha: false })!;
  }
  render(snapshot: RustRenderSnapshot, transitionMs = 1_000, clockMs = transitionMs) {
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
      if (draw.muzzle) this.playerMuzzle = { x: x0 + draw.muzzle[0] * scale, y: y0 + draw.muzzle[1] * scale, scale: sceneScale };
      ctx.drawImage(this.image, sx, sy, ATLAS_CELL, ATLAS_CELL, x0, y0, ATLAS_CELL * scale, ATLAS_CELL * scale);
    }
    ctx.globalAlpha = 1;
    return performance.now() - started;
  }
  projectCell(cell: number) { return projectCell(this.view, cell); }
  projectPlayerMuzzle() { return this.playerMuzzle; }
  destroy() {}
}

class WebGlAtlasRenderer implements RustIsoRenderer {
  readonly kind = "webgl2" as const;
  private program: WebGLProgram;
  private buffer: WebGLBuffer;
  private position: number;
  private camera: CameraState = { x: 0, y: 0, zoom: 1.12, floor: 0, entryFrame: -1, initialized: false, lastAt: performance.now() };
  private view: ViewTransform | null = null;
  private playerMuzzle: RustIsoScreenPoint | null = null;
  constructor(private canvas: HTMLCanvasElement, private gl: WebGL2RenderingContext, image: HTMLImageElement, private meta: AtlasMeta) {
    const vertex = compile(gl, gl.VERTEX_SHADER, `#version 300 es
      in vec2 a_position; in vec2 a_uv; in float a_opacity; out vec2 v_uv; out float v_opacity; uniform vec2 u_size;
      void main(){ vec2 p=a_position/u_size*2.0-1.0; gl_Position=vec4(p.x,-p.y,0,1); v_uv=a_uv; v_opacity=a_opacity; }`);
    const fragment = compile(gl, gl.FRAGMENT_SHADER, `#version 300 es
      precision mediump float; in vec2 v_uv; in float v_opacity; out vec4 color; uniform sampler2D u_atlas;
      void main(){ color=texture(u_atlas,v_uv)*v_opacity; if(color.a<0.02) discard; }`);
    this.program = gl.createProgram()!;
    gl.attachShader(this.program, vertex); gl.attachShader(this.program, fragment); gl.linkProgram(this.program);
    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(this.program) || "WebGL link failed");
    this.position = gl.getAttribLocation(this.program, "a_position");
    const uv = gl.getAttribLocation(this.program, "a_uv");
    this.buffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.enableVertexAttribArray(this.position); gl.vertexAttribPointer(this.position, 2, gl.FLOAT, false, 20, 0);
    gl.enableVertexAttribArray(uv); gl.vertexAttribPointer(uv, 2, gl.FLOAT, false, 20, 8);
    const opacity = gl.getAttribLocation(this.program, "a_opacity");
    gl.enableVertexAttribArray(opacity); gl.vertexAttribPointer(opacity, 1, gl.FLOAT, false, 20, 16);
    const texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);
    gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  }
  render(snapshot: RustRenderSnapshot, transitionMs = 1_000, clockMs = transitionMs) {
    const started = performance.now();
    const size = resize(this.canvas);
    const gl = this.gl;
    gl.viewport(0, 0, size.width, size.height);
    gl.clearColor(0.008, 0.02, 0.016, 1); gl.clear(gl.COLOR_BUFFER_BIT);
    const camera = cameraOrigin(this.camera, snapshot, transitionMs);
    const sceneScale = size.scale * camera.zoom;
    const ox = (size.width - LOGICAL_W * sceneScale) / 2;
    const oy = (size.height - LOGICAL_H * sceneScale) / 2;
    this.view = { ox, oy, sceneScale, cameraX: camera.x, cameraY: camera.y, width: snapshot.width };
    this.playerMuzzle = null;
    const data: number[] = [];
    for (const draw of commands(snapshot, this.meta, transitionMs, clockMs, camera)) {
      const anchor = draw.anchor ?? this.meta.anchors[draw.cell];
      const scale = draw.scale * sceneScale;
      const x0 = ox + draw.x * sceneScale - anchor[0] * scale, y0 = oy + draw.y * sceneScale - anchor[1] * scale;
      if (draw.muzzle) this.playerMuzzle = { x: x0 + draw.muzzle[0] * scale, y: y0 + draw.muzzle[1] * scale, scale: sceneScale };
      const x1 = x0 + ATLAS_CELL * scale, y1 = y0 + ATLAS_CELL * scale;
      const u0 = draw.cell % ATLAS_COLUMNS * ATLAS_CELL / this.meta.width;
      const v0 = Math.floor(draw.cell / ATLAS_COLUMNS) * ATLAS_CELL / this.meta.height;
      const u1 = u0 + ATLAS_CELL / this.meta.width, v1 = v0 + ATLAS_CELL / this.meta.height;
      const opacity = draw.opacity ?? 1;
      data.push(x0,y0,u0,v0,opacity, x1,y0,u1,v0,opacity, x0,y1,u0,v1,opacity, x0,y1,u0,v1,opacity, x1,y0,u1,v0,opacity, x1,y1,u1,v1,opacity);
    }
    gl.useProgram(this.program);
    gl.uniform2f(gl.getUniformLocation(this.program, "u_size"), size.width, size.height);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.DYNAMIC_DRAW);
    gl.drawArrays(gl.TRIANGLES, 0, data.length / 5);
    return performance.now() - started;
  }
  projectCell(cell: number) { return projectCell(this.view, cell); }
  projectPlayerMuzzle() { return this.playerMuzzle; }
  destroy() { this.gl.deleteBuffer(this.buffer); this.gl.deleteProgram(this.program); }
}

function compile(gl: WebGL2RenderingContext, kind: number, source: string) {
  const shader = gl.createShader(kind)!;
  gl.shaderSource(shader, source); gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader) || "WebGL compile failed");
  return shader;
}
