import type { RustRenderSnapshot } from "../runtime/rust-wasm-protocol";
import type { RustIsoRenderer, RustIsoScreenPoint } from "./rust-iso-atlas-renderer";

type EffectKind = "damage" | "player-damage" | "heal" | "kill" | "tracer" | "throw" | "teleport-out" | "teleport-in" | "miss" | "reward" | "level" | "status" | "contact" | "victory" | "defeat";
export type RustIsoEffect = { kind: EffectKind; cell: number; targetCell?: number; magnitude?: number; color?: string; delay?: number };
type ActiveEffect = RustIsoEffect & { startedAt: number; duration: number };
export const THROW_EFFECT_MS = 240;
export const TRACER_EFFECT_MS = 90;
export const TELEPORT_OUT_MS = 520;
export const TELEPORT_IN_MS = 620;

const DURATIONS: Record<EffectKind, number> = {
  damage: 420, "player-damage": 420, heal: 520, kill: 560, tracer: TRACER_EFFECT_MS,
  throw: THROW_EFFECT_MS, "teleport-out": TELEPORT_OUT_MS, "teleport-in": TELEPORT_IN_MS,
  miss: 340, reward: 420, level: 560, status: 520, contact: 280,
  victory: 1_900, defeat: 1_900,
};

// Effect dimensions were authored against the normal 1180x800 desktop camera,
// whose measured scene scale is 0.5330729167 CSS pixels per logical pixel.
// Normalize the live
// backing-pixel scene scale to that baseline: desktop retains its established
// visual size, while mobile follows the map's actual CSS-size ratio.
const EFFECT_DESKTOP_SCENE_SCALE = 0.5330729166666667;

export function rustIsoEffectTimelineMs(effects: RustIsoEffect[]) {
  return effects.reduce((longest, effect) => Math.max(longest, (effect.delay ?? 0) + DURATIONS[effect.kind]), 0);
}

export function rustIsoEffectTimeScale(effects: RustIsoEffect[], windowMs?: number) {
  const naturalTimeline = rustIsoEffectTimelineMs(effects);
  const terminal = effects.some((effect) => effect.kind === "victory" || effect.kind === "defeat");
  return !terminal && windowMs && naturalTimeline > windowMs ? windowMs / naturalTimeline : 1;
}

export function deriveRustIsoEffects(before: RustRenderSnapshot | null, next: RustRenderSnapshot): RustIsoEffect[] {
  if (!before) return [];
  const sameFloor = before.floor === next.floor;
  const effects: RustIsoEffect[] = [];
  const add = (effect: RustIsoEffect) => effects.push({ ...effect, delay: effect.delay ?? effects.filter((row) => row.cell === effect.cell).length * 90 });

  if (sameFloor && next.player.teleported && before.player.cell !== next.player.cell) {
    add({ kind: "teleport-out", cell: before.player.cell, delay: 0 });
    add({ kind: "teleport-in", cell: next.player.cell, delay: 0 });
  }

  if (before.player.hp > next.player.hp) add({ kind: "player-damage", cell: next.player.cell, magnitude: before.player.hp - next.player.hp });
  else if (before.player.hp < next.player.hp && /^(eat|use):/.test(next.action)) {
    // Safe movement passively restores 1 HP every fourth turn. A full healing
    // pulse on each of those steps reads like an action and overlaps into a
    // continuous green wave at normal autoplay speed. Reserve the prominent
    // effect for healing the player explicitly triggered.
    add({ kind: "heal", cell: next.player.cell, magnitude: next.player.hp - before.player.hp });
  }

  if (sameFloor) {
    for (const oldMob of before.mobs) {
      const mob = next.mobs.find((candidate) => candidate.uid === oldMob.uid);
      if (!mob || oldMob.hp > 0 && mob.hp <= 0) {
        add({ kind: "damage", cell: oldMob.cell, magnitude: Math.max(1, oldMob.hp) });
        add({ kind: "kill", cell: oldMob.cell, delay: 100 });
      } else if (oldMob.hp > mob.hp) add({ kind: "damage", cell: mob.cell, magnitude: oldMob.hp - mob.hp });
    }

    for (const mob of next.mobs) if (mob.appeared) add({ kind: "contact", cell: mob.cell });
  }

  const target = actionTarget(next);
  if (target !== null && next.action.startsWith("fire:")) add({ kind: "tracer", cell: next.player.cell, targetCell: target, delay: 0 });
  if (target !== null && next.action.startsWith("throw:")) add({ kind: "throw", cell: next.player.cell, targetCell: target, delay: 0 });
  if (target !== null && next.logs.some((line) => line.text.startsWith("Shot missed"))) add({ kind: "miss", cell: target });

  for (const line of next.logs) {
    if (line.text.startsWith("Picked up ") || line.text === "MIB supplies received.") add({ kind: "reward", cell: next.player.cell, color: "#73c8d6" });
    else if (/^\+\d+ credits\.$/.test(line.text)) add({ kind: "reward", cell: next.player.cell, color: "#e4c15d" });
    else if (/^Level \d+\./.test(line.text)) add({ kind: "level", cell: next.player.cell });
    else if (line.text.endsWith(" active.")) add({ kind: "status", cell: next.player.cell, color: line.cls === "good" ? "#9adf91" : "#ff756f" });
  }
  if (next.won && !before.won) add({ kind: "victory", cell: next.player.cell, delay: 0 });
  else if (next.dead && !before.dead) add({ kind: "defeat", cell: next.player.cell, delay: 0 });
  return effects;
}

export function splitRangedEffects(effects: RustIsoEffect[]) {
  return {
    travel: effects.filter((effect) => effect.kind === "tracer" || effect.kind === "throw"),
    impact: effects.filter((effect) => effect.kind !== "tracer" && effect.kind !== "throw"),
  };
}

export function splitTeleportEffects(effects: RustIsoEffect[]) {
  return {
    departure: effects.filter((effect) => effect.kind === "teleport-out"),
    arrival: effects.filter((effect) => effect.kind !== "teleport-out"),
  };
}

export function stageTeleportTransition(before: RustRenderSnapshot | null, next: RustRenderSnapshot) {
  if (!before || before.floor !== next.floor || !next.player.teleported || before.player.cell === next.player.cell) return null;
  const departure: RustRenderSnapshot = {
    ...before,
    frame: next.frame,
    frameCount: next.frameCount,
    action: next.action,
    logs: [],
    policy: next.policy,
    // A teleport can immediately follow a walk. Do not carry that walk's
    // from-cell interpolation into the effect: the departure sprite and its
    // aperture must share one stable tile until the phase is complete.
    player: {
      ...before.player,
      fromCell: before.player.cell,
      state: 0,
      teleported: false,
      teleportPhase: "out",
    },
  };
  const arrival: RustRenderSnapshot = {
    ...next,
    player: {
      ...next.player,
      fromCell: next.player.cell,
      state: 0,
      teleportPhase: "in",
    },
  };
  return { departure, arrival };
}

export function stageRangedImpactTransition(before: RustRenderSnapshot | null, next: RustRenderSnapshot) {
  if (!before || before.floor !== next.floor || !/^(fire|throw):/.test(next.action)) return null;
  const mobs = next.mobs.map((mob) => {
    const previous = before.mobs.find((candidate) => candidate.uid === mob.uid);
    if (!previous || previous.hp <= mob.hp && previous.frozen === mob.frozen) return mob;
    // The Rust state is already authoritative. This copy affects only the
    // presentation frame shown while the projectile is in flight, keeping
    // hurt, death, and frozen poses at the impact end of the trajectory.
    return {
      ...mob,
      hp: previous.hp,
      fromCell: previous.fromCell,
      state: previous.state,
      direction: previous.direction,
      asleep: previous.asleep,
      pacified: previous.pacified,
      frozen: previous.frozen,
    };
  });
  return { ...next, won: before.won, dead: before.dead, mobs };
}

// Kept as a narrow compatibility export for existing pipeline callers.
export const stageThrownFreezeTransition = stageRangedImpactTransition;

function actionTarget(snapshot: RustRenderSnapshot) {
  const fields = snapshot.action.split(":");
  const value = snapshot.action.startsWith("fire:") ? fields[1] : snapshot.action.startsWith("throw:") ? fields[2] : undefined;
  if (!value) return null;
  const [x, y] = value.split(",").map(Number);
  return Number.isInteger(x) && Number.isInteger(y) && x >= 0 && y >= 0 && x < snapshot.width && y < snapshot.height ? y * snapshot.width + x : null;
}

export class RustIsoEffectLayer {
  private ctx: CanvasRenderingContext2D;
  private active: ActiveEffect[] = [];

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
  }

  clear() {
    this.active = [];
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
  }

  clearCells(cells: number[]) {
    if (!cells.length) return;
    const removed = new Set(cells);
    this.active = this.active.filter((effect) => !removed.has(effect.cell) && (effect.targetCell === undefined || !removed.has(effect.targetCell)));
  }

  hasActiveAtCell(cell: number) {
    return this.active.some((effect) => effect.cell === cell || effect.targetCell === cell);
  }

  play(effects: RustIsoEffect[], now: number, windowMs?: number) {
    const scale = rustIsoEffectTimeScale(effects, windowMs);
    for (const effect of effects) {
      if (effect.kind === "damage" || effect.kind === "kill") {
        this.active = this.active.filter((active) => active.kind !== "miss" || active.cell !== effect.cell);
      }
      this.active.push({
        ...effect,
        startedAt: now + (effect.delay ?? 0) * scale,
        duration: DURATIONS[effect.kind] * scale,
      });
    }
    if (this.active.length > 40) this.active.splice(0, this.active.length - 40);
  }

  render(renderer: RustIsoRenderer, now: number) {
    const dpr = Math.min(devicePixelRatio || 1, 1.5);
    const width = Math.max(1, Math.round(this.canvas.clientWidth * dpr));
    const height = Math.max(1, Math.round(this.canvas.clientHeight * dpr));
    if (this.canvas.width !== width || this.canvas.height !== height) { this.canvas.width = width; this.canvas.height = height; }
    const ctx = this.ctx;
    ctx.clearRect(0, 0, width, height);
    this.active = this.active.filter((effect) => now < effect.startedAt + effect.duration);
    const ordered = [...this.active].sort((a, b) => effectPriority(a.kind) - effectPriority(b.kind));
    for (const effect of ordered) {
      if (now < effect.startedAt) continue;
      const muzzle = effect.kind === "tracer" ? renderer.projectPlayerMuzzle() : null;
      const point = muzzle ?? renderer.projectCell(effect.cell);
      if (!point) continue;
      const t = Math.min(1, (now - effect.startedAt) / effect.duration);
      drawEffect(ctx, effect, point, effect.targetCell === undefined ? null : renderer.projectCell(effect.targetCell), t, width, height, Boolean(muzzle));
    }
  }
}

function drawEffect(ctx: CanvasRenderingContext2D, effect: ActiveEffect, point: RustIsoScreenPoint, target: RustIsoScreenPoint | null, t: number, width: number, height: number, exactSource = false) {
  // projectCell returns backing-canvas coordinates and the exact scene scale
  // used by the map camera. Effects must use that scale too; devicePixelRatio
  // only describes canvas density and drifts badly from the zoomed scene on
  // narrow mobile viewports.
  const scale = point.scale / EFFECT_DESKTOP_SCENE_SCALE;
  ctx.save();
  const alpha = t < .68 ? 1 : 1 - (t - .68) / .32;
  ctx.globalAlpha = Math.max(0, alpha);
  if (effect.kind === "victory") drawTerminalEffect(ctx, point, t, scale, width, height, true);
  else if (effect.kind === "defeat") drawTerminalEffect(ctx, point, t, scale, width, height, false);
  else if (effect.kind === "tracer" && target) drawTracer(ctx, point, target, t, scale, false, exactSource);
  else if (effect.kind === "throw" && target) drawTracer(ctx, point, target, t, scale, true);
  else if (effect.kind === "teleport-out" || effect.kind === "teleport-in") drawTeleport(ctx, point, t, scale, effect.kind === "teleport-in");
  else if (effect.kind === "contact") drawRings(ctx, point, t, scale, "#73c8d6", 3);
  else {
    const color = effect.color ?? (effect.kind === "heal" ? "#9adf91" : effect.kind === "kill" || effect.kind === "level" ? "#e4c15d" : effect.kind === "reward" || effect.kind === "status" ? "#73c8d6" : "#ff655f");
    if (effect.kind === "player-damage") drawVignette(ctx, t, width, height);
    if (["damage", "player-damage", "kill"].includes(effect.kind)) drawBurst(ctx, point, t, scale, color, effect.magnitude ?? 0);
    else if (effect.kind === "miss") drawMiss(ctx, point, t, scale, color);
    else drawRings(ctx, point, t, scale, color, effect.kind === "level" ? 3 : 2);
  }
  ctx.restore();
}

function effectPriority(kind: EffectKind) {
  return kind === "victory" || kind === "defeat" ? 12 : kind === "player-damage" ? 10 : kind === "kill" || kind === "level" ? 8 : kind === "damage" || kind === "heal" ? 6 : 2;
}

function drawTerminalEffect(ctx: CanvasRenderingContext2D, point: RustIsoScreenPoint, t: number, scale: number, width: number, height: number, victory: boolean) {
  const primary = victory ? "#e4c15d" : "#ff655f", secondary = victory ? "#73c8d6" : "#a22d36";
  const opening = Math.min(1, t / .42), fade = t < .78 ? 1 : 1 - (t - .78) / .22;
  const wash = ctx.createRadialGradient(point.x, point.y - 36 * scale, 0, point.x, point.y - 36 * scale, Math.max(width, height) * .72);
  wash.addColorStop(0, victory ? `rgba(228,193,93,${.24 * opening * fade})` : `rgba(255,70,62,${.2 * opening * fade})`);
  wash.addColorStop(.44, victory ? `rgba(48,160,154,${.1 * opening * fade})` : `rgba(75,4,12,${.18 * opening * fade})`);
  wash.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = wash; ctx.fillRect(0, 0, width, height);
  if (!victory) drawVignette(ctx, Math.min(.38, t * .38), width, height);

  ctx.globalAlpha *= fade;
  for (let ring = 0; ring < 4; ring++) {
    const phase = Math.max(0, Math.min(1, opening - ring * .14));
    if (!phase) continue;
    ctx.strokeStyle = ring % 2 ? secondary : primary; ctx.lineWidth = Math.max(1, (4 - phase * 3) * scale);
    ctx.globalAlpha = fade * (1 - phase * .72);
    ctx.beginPath(); ctx.ellipse(point.x, point.y - 25 * scale, (18 + phase * 115) * scale, (7 + phase * 42) * scale, 0, 0, Math.PI * 2); ctx.stroke();
  }

  const particles = victory ? 24 : 18;
  for (let index = 0; index < particles; index++) {
    const angle = index / particles * Math.PI * 2 + (victory ? -Math.PI / 2 : Math.PI / 2);
    const speed = (34 + index % 6 * 9) * scale, travel = Math.sin(Math.min(1, t * 1.7) * Math.PI / 2) * speed;
    const drift = victory ? -t * (28 + index % 4 * 8) * scale : t * (18 + index % 5 * 7) * scale;
    const x = point.x + Math.cos(angle) * travel, y = point.y - 34 * scale + Math.sin(angle) * travel + drift;
    ctx.fillStyle = index % 3 ? primary : secondary; ctx.globalAlpha = fade * (.5 + (index % 4) * .12);
    ctx.save(); ctx.translate(x, y); ctx.rotate(angle + t * 4); const size = (3 + index % 3) * scale;
    if (victory) ctx.fillRect(-size / 2, -size, size, size * 2); else { ctx.beginPath(); ctx.moveTo(0, -size * 1.5); ctx.lineTo(size, size); ctx.lineTo(-size, size); ctx.closePath(); ctx.fill(); }
    ctx.restore();
  }
}

function drawBurst(ctx: CanvasRenderingContext2D, point: RustIsoScreenPoint, t: number, scale: number, color: string, magnitude: number) {
  const pulse = Math.min(1, t / .32), radius = (10 + Math.min(16, magnitude) * .8 + 34 * pulse) * scale;
  ctx.strokeStyle = color; ctx.lineWidth = Math.max(1, (4 - pulse * 3) * scale); ctx.globalAlpha *= 1 - pulse * .55;
  ctx.beginPath(); ctx.arc(point.x, point.y - 35 * scale, radius, 0, Math.PI * 2); ctx.stroke();
  for (let index = 0; index < 10; index++) {
    const angle = index / 10 * Math.PI * 2, inner = radius * .45, outer = radius * (1.05 + (index % 3) * .16);
    ctx.beginPath(); ctx.moveTo(point.x + Math.cos(angle) * inner, point.y - 35 * scale + Math.sin(angle) * inner); ctx.lineTo(point.x + Math.cos(angle) * outer, point.y - 35 * scale + Math.sin(angle) * outer); ctx.stroke();
  }
  const pips = Math.max(0, Math.min(20, Math.round(magnitude)));
  ctx.fillStyle = color; ctx.globalAlpha = Math.max(.15, 1 - t);
  for (let index = 0; index < pips; index++) {
    const angle = index / Math.max(1, pips) * Math.PI * 2 - Math.PI / 2;
    const orbit = (22 + (index % 2) * 8 + pulse * 22) * scale;
    const x = point.x + Math.cos(angle) * orbit, y = point.y - 35 * scale + Math.sin(angle) * orbit;
    ctx.save(); ctx.translate(x, y); ctx.rotate(angle + Math.PI / 4); ctx.fillRect(-2.5 * scale, -2.5 * scale, 5 * scale, 5 * scale); ctx.restore();
  }
}

function drawMiss(ctx: CanvasRenderingContext2D, point: RustIsoScreenPoint, t: number, scale: number, color: string) {
  const pulse = Math.min(1, t / .4), radius = (12 + 28 * pulse) * scale, y = point.y - 28 * scale;
  ctx.strokeStyle = color; ctx.lineWidth = Math.max(1, (3 - pulse * 2) * scale); ctx.globalAlpha *= 1 - pulse * .55;
  ctx.beginPath(); ctx.arc(point.x, y, radius, 0, Math.PI * 2); ctx.stroke();
  const arm = radius * .55;
  ctx.beginPath(); ctx.moveTo(point.x - arm, y - arm); ctx.lineTo(point.x + arm, y + arm); ctx.moveTo(point.x + arm, y - arm); ctx.lineTo(point.x - arm, y + arm); ctx.stroke();
  ctx.globalAlpha = Math.max(0, 1 - t * 1.4);
  ctx.fillStyle = "rgba(255,190,184,.82)"; ctx.textAlign = "center"; ctx.textBaseline = "bottom";
  ctx.font = `700 ${Math.max(7, 11 * scale)}px "Courier New",monospace`;
  ctx.fillText("MISS", point.x, y - radius - 4 * scale);
}

function drawRings(ctx: CanvasRenderingContext2D, point: RustIsoScreenPoint, t: number, scale: number, color: string, count: number) {
  ctx.strokeStyle = color; ctx.lineWidth = Math.max(1, 2 * scale);
  for (let index = 0; index < count; index++) {
    const phase = Math.max(0, Math.min(1, t * 1.5 - index * .13));
    ctx.globalAlpha *= Math.max(.15, 1 - phase);
    ctx.beginPath(); ctx.ellipse(point.x, point.y - 14 * scale, (12 + phase * 42) * scale, (5 + phase * 17) * scale, 0, 0, Math.PI * 2); ctx.stroke();
  }
}

function drawTracer(ctx: CanvasRenderingContext2D, from: RustIsoScreenPoint, to: RustIsoScreenPoint, t: number, scale: number, arc: boolean, exactSource = false) {
  const progress = Math.min(1, t), x = from.x + (to.x - from.x) * progress;
  const sourceY = exactSource ? from.y : from.y - 38 * scale;
  const targetY = to.y - 38 * scale;
  const baseY = sourceY + (targetY - sourceY) * progress, y = arc ? baseY - Math.sin(progress * Math.PI) * 75 * scale : baseY;
  ctx.strokeStyle = arc ? "#e4c15d" : "#73e6ff"; ctx.lineWidth = Math.max(1, (arc ? 3 : 2) * scale); ctx.shadowColor = ctx.strokeStyle; ctx.shadowBlur = 10 * scale;
  ctx.beginPath(); ctx.moveTo(from.x, sourceY);
  if (arc) ctx.quadraticCurveTo((from.x + to.x) / 2, Math.min(from.y, to.y) - 100 * scale, x, y); else ctx.lineTo(x, y);
  ctx.stroke(); ctx.shadowBlur = 0; ctx.fillStyle = ctx.strokeStyle; ctx.beginPath(); ctx.arc(x, y, Math.max(1.5, 4 * scale), 0, Math.PI * 2); ctx.fill();
}

function drawTeleport(ctx: CanvasRenderingContext2D, point: RustIsoScreenPoint, t: number, scale: number, arriving: boolean) {
  const phase = arriving ? t : 1 - t;
  const centerY = point.y - 38 * scale;
  const glow = .35 + Math.sin(Math.min(1, t) * Math.PI) * .65;
  ctx.save();
  ctx.globalCompositeOperation = "screen";
  ctx.shadowColor = "#73e6ff";
  ctx.shadowBlur = (12 + glow * 22) * scale;

  const column = ctx.createLinearGradient(point.x, point.y - 105 * scale, point.x, point.y + 4 * scale);
  column.addColorStop(0, "rgba(115,230,255,0)");
  column.addColorStop(.35, `rgba(115,230,255,${.12 + glow * .2})`);
  column.addColorStop(.72, `rgba(174,118,255,${.1 + glow * .2})`);
  column.addColorStop(1, "rgba(174,118,255,0)");
  ctx.fillStyle = column;
  ctx.fillRect(point.x - (8 + 14 * glow) * scale, point.y - 108 * scale, (16 + 28 * glow) * scale, 112 * scale);

  for (let ring = 0; ring < 4; ring++) {
    const stagger = Math.max(0, Math.min(1, t * 1.35 - ring * .1));
    const radius = arriving ? 48 - stagger * 34 : 14 + stagger * 38;
    ctx.strokeStyle = ring % 2 ? "#b98aff" : "#73e6ff";
    ctx.lineWidth = Math.max(1, (3 - stagger * 1.8) * scale);
    ctx.globalAlpha = Math.max(.08, (1 - stagger * .72) * glow);
    ctx.beginPath();
    ctx.ellipse(point.x, point.y - 8 * scale, radius * scale, (5 + radius * .28) * scale, 0, 0, Math.PI * 2);
    ctx.stroke();
  }

  ctx.globalAlpha = .35 + glow * .65;
  for (let index = 0; index < 18; index++) {
    const angle = index / 18 * Math.PI * 2 + t * (arriving ? -2.5 : 3.5);
    const orbit = (10 + (index % 5) * 7 + (1 - phase) * 25) * scale;
    const lift = ((index % 6) * 15 - 42) * scale;
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

function drawVignette(ctx: CanvasRenderingContext2D, t: number, width: number, height: number) {
  const strength = Math.max(0, 1 - t * 2.4) * .48;
  const gradient = ctx.createRadialGradient(width / 2, height / 2, Math.min(width, height) * .18, width / 2, height / 2, Math.max(width, height) * .68);
  gradient.addColorStop(0, "rgba(130,0,0,0)"); gradient.addColorStop(1, `rgba(235,45,35,${strength})`);
  ctx.fillStyle = gradient; ctx.fillRect(0, 0, width, height);
}
