import { createRustIsoRenderer, presentedPlayerPose, shouldRenderMob, type RustIsoRenderer, type RustIsoRendererKind } from "./renderer/rust-iso-atlas-renderer";
import { deriveRustIsoEffects, RustIsoEffectLayer, splitRangedEffects, splitTeleportEffects, stageRangedImpactTransition, stageTeleportTransition, TELEPORT_IN_MS, TELEPORT_OUT_MS, THROW_EFFECT_MS, TRACER_EFFECT_MS, type RustIsoEffect } from "./renderer/rust-iso-effects";
import type { RustPlanEvaluation, RustRenderSnapshot, RustWorkerRequest, RustWorkerResponse } from "./runtime/rust-wasm-protocol";
import { renderHudView, renderPregameHud } from "./shared/hud";
import { inventoryPrimaryAction } from "./shared/inventory-primary-action";

const canvasHost = document.querySelector<HTMLDivElement>("#canvasHost")!;
const stats = document.querySelector<HTMLPreElement>("#perfStats")!;
const status = document.querySelector<HTMLDivElement>("#status")!;
const history = document.querySelector<HTMLDivElement>("#history")!;
const seedInput = document.querySelector<HTMLInputElement>("#seed")!;
const classSelect = document.querySelector<HTMLSelectElement>("#class")!;
const rendererSelect = document.querySelector<HTMLSelectElement>("#renderer")!;
const fpsInput = document.querySelector<HTMLInputElement>("#fps")!;
const fpsValue = document.querySelector<HTMLOutputElement>("#fpsValue")!;
const fpsDefaultButton = document.querySelector<HTMLButtonElement>("#fpsDefault")!;
const plannerCoresInput = document.querySelector<HTMLInputElement>("#plannerCores")!;
const plannerCoresValue = document.querySelector<HTMLOutputElement>("#plannerCoresValue")!;
const plannerCoresMax = document.querySelector<HTMLSpanElement>("#plannerCoresMax")!;
const plannerStrengthSelect = document.querySelector<HTMLSelectElement>("#plannerStrength")!;
const plannerStrengthValue = document.querySelector<HTMLOutputElement>("#plannerStrengthValue")!;
const plannerStrengthHint = document.querySelector<HTMLSpanElement>("#plannerStrengthHint")!;
const startButton = document.querySelector<HTMLButtonElement>("#start")!;
const stepButton = document.querySelector<HTMLButtonElement>("#step")!;
const autoplayButton = document.querySelector<HTMLButtonElement>("#autoplay")!;
const resetButton = document.querySelector<HTMLButtonElement>("#reset")!;
const newRunButton = document.querySelector<HTMLButtonElement>("#newRun")!;
const endingNewButton = document.querySelector<HTMLButtonElement>("#endingNew")!;
const hud = document.querySelector<HTMLDivElement>("#stats")!;
const ending = document.querySelector<HTMLDivElement>("#ending")!;
const endingTitle = document.querySelector<HTMLElement>("#endingTitle")!;
const endingBody = document.querySelector<HTMLElement>("#endingBody")!;
const classPicker = document.querySelector<HTMLDivElement>("#classPicker")!;
const settingsModal = document.querySelector<HTMLDivElement>("#settingsModal")!;
const settingsButton = document.querySelector<HTMLButtonElement>("#settingsButton")!;
const settingsClose = document.querySelector<HTMLButtonElement>("#settingsClose")!;

function commandButton(icon: "auto" | "stop" | "new" | "below", label: string) {
  return `<span class="btn-icon icon-${icon}"></span><span>${label}</span>`;
}
const layoutLogButton = document.querySelector<HTMLButtonElement>("#layoutLog")!;
const helpButton = document.querySelector<HTMLButtonElement>("#helpButton")!;
const helpModal = document.querySelector<HTMLDivElement>("#helpModal")!;
const helpClose = document.querySelector<HTMLButtonElement>("#helpClose")!;
const inventoryModal = document.querySelector<HTMLDivElement>("#inventoryModal")!;
const inventoryList = document.querySelector<HTMLDivElement>("#inventoryList")!;
const inventoryHint = document.querySelector<HTMLDivElement>("#inventoryHint")!;
const shopModal = document.querySelector<HTMLDivElement>("#shopModal")!;
const shopList = document.querySelector<HTMLDivElement>("#shopList")!;
const planningModal = document.querySelector<HTMLDivElement>("#planningModal")!;
let inventorySelection = 0;
let shopSelection = 0;
let targetMode: null | { action: "fire" | "throw"; item?: string; cell: number } = null;
let recommendationId = 0;
const recommendationWaiters = new Map<number, (signature: string) => void>();
const DEFAULT_AUTOPLAY_FPS = 6;
const MAX_MANUAL_FPS = 6;
const REPORTED_LOGICAL_CPUS = Math.max(1, navigator.hardwareConcurrency || 2);
// Browsers expose logical processors only. Use the conservative SMT pairing
// estimate so a 6-core/12-thread CPU is never offered more than 6 workers.
const PLANNER_CORE_LIMIT = Math.max(1, Math.floor(REPORTED_LOGICAL_CPUS / 2));
const DEFAULT_PLANNER_CORES = Math.min(4, PLANNER_CORE_LIMIT);
const REFERENCE_BASELINE_MS = 84;
const CALIBRATION_TURN_CAP = 600;
const CALIBRATION_BUDGET_MS = 850;
const STRONGEST_MIN_RELATIVE_SPEED = 0.70;
const BALANCED_MIN_RELATIVE_SPEED = 0.35;
type PlannerLevel = "baseline" | "balanced" | "strongest";
type PlannerStrengthSetting = "adaptive" | PlannerLevel;
const MOB_APPEAR_MS = 280;
const MOB_DEATH_MS = 760;
const ENDING_PRELUDE_MS = 1_250;
let resumeAfterSettings = false;
let actionLog: Array<{ text: string; cls?: "good" | "warn" | "flash"; repeat: number }> = [];

if (new URLSearchParams(location.search).has("e2e")) {
  Object.defineProperty(window, "__MIB_RUST_E2E__", { value: {
    snapshot: () => simulationSnapshot,
    presented: () => snapshot,
    pose: () => snapshot ? presentedPlayerPose(snapshot, (presentationPausedAt ?? performance.now()) - presentationStarted) : null,
    effectAt: (cell: number) => effectLayer?.hasActiveAtCell(cell) ?? false,
    log: () => actionLog.map((entry) => ({ ...entry })),
    idle: () => !pending,
    planning: () => planningTelemetry && { ...planningTelemetry, candidateMs: [...planningTelemetry.candidateMs] },
    project: (cell: number) => renderer?.projectCell(cell) ?? null,
    mobRenderable: (uid: number) => simulationSnapshot?.mobs.some((mob) => mob.uid === uid && shouldRenderMob(simulationSnapshot!, mob)) ?? false,
    seek: (frame: number) => worker.postMessage({ type: "seek", frame }),
    act: (signature: string) => worker.postMessage({ type: "action", signature }),
    previewTeleport: () => previewTeleportForAudit(),
    recommend: () => new Promise<string>((resolve) => {
      const requestId = ++recommendationId;
      recommendationWaiters.set(requestId, resolve);
      worker.postMessage({ type: "recommend", requestId });
    }),
  }, configurable: true });
}

function appendActionLog(text: string, cls?: "good" | "warn" | "flash") {
  const last = actionLog[actionLog.length - 1];
  if (last?.text === text && last.cls === cls) last.repeat += 1;
  else actionLog.push({ text, cls, repeat: 1 });
  actionLog = actionLog.slice(-120);
  renderActionLog();
  status.hidden = true;
}

function renderActionLog() {
  const visibleLines = document.body.dataset.logLayout === "side" && window.innerWidth > 860 ? 16 : 4;
  history.replaceChildren(...actionLog.slice(-visibleLines).map((entry) => {
    const row = document.createElement("div");
    row.textContent = entry.repeat > 1 ? `${entry.text} ×${entry.repeat}` : entry.text;
    if (entry.cls) row.className = entry.cls;
    return row;
  }));
}

const embeddedWorkerUrl = (globalThis as typeof globalThis & { __MIB_RUST_WORKER_URL__?: string }).__MIB_RUST_WORKER_URL__;
function createSimulationWorker() {
  return embeddedWorkerUrl
  ? new Worker(embeddedWorkerUrl)
  : new Worker("./dist/isometric-rust-worker.js?v=20260721.1", { type: "module" });
}
const worker = createSimulationWorker();
let renderer: RustIsoRenderer | null = null;
let effectLayer: RustIsoEffectLayer | null = null;
const effectsBySnapshot = new WeakMap<RustRenderSnapshot, RustIsoEffect[]>();
let snapshot: RustRenderSnapshot | null = null;
let simulationSnapshot: RustRenderSnapshot | null = null;
let visualQueue: RustRenderSnapshot[] = [];
let visualTimer: number | null = null;
let visualEndsAt = 0;
let visualRemaining = 0;
let presentationPausedAt: number | null = null;
let autoplay = false;
let pending = false;
let planning = false;
let manualRequestPending = false;
let lastManualMovementAt = -Infinity;
const presentationDurations = new WeakMap<RustRenderSnapshot, number>();
const projectilePreviews = new WeakSet<RustRenderSnapshot>();
const teleportPreviews = new WeakSet<RustRenderSnapshot>();
const loggedSnapshots = new WeakSet<RustRenderSnapshot>();
let initMs = 0;
let wasmMs = 0;
let snapshotMs = 0;
let renderMs = 0;
let renderedFrames = 0;
let totalRenderMs = 0;
let presentationStarted = 0;
let lastStatsAt = 0;
let endingFrame = -1;
let endingRevealAt = 0;
let planningTelemetry: null | { cores: number; candidates: number; evaluationMs: number; candidateMs: number[]; level: PlannerLevel; baselineMs?: number; calibrationMs?: number; calibrationTimedOut?: boolean; totalMs?: number } = null;
let planningBeganAt = 0;
function resetEndingSequence() {
  endingFrame = -1;
  endingRevealAt = 0;
  ending.classList.remove("visible", "complete", "failed");
}

function prepareEndingSequence(next: RustRenderSnapshot, now: number) {
  if (!next.won && !next.dead) { resetEndingSequence(); return; }
  if (endingFrame === next.frame) return;
  endingFrame = next.frame;
  endingRevealAt = now + ENDING_PRELUDE_MS;
  ending.classList.remove("visible");
  ending.classList.toggle("complete", next.won);
  ending.classList.toggle("failed", next.dead);
}

function playbackFps() {
  return Math.max(1, Math.min(60, Number(fpsInput.value) || DEFAULT_AUTOPLAY_FPS));
}

function syncFpsSetting(value: string | null = fpsInput.value) {
  const fps = Math.max(1, Math.min(60, Number(value) || DEFAULT_AUTOPLAY_FPS));
  fpsInput.value = String(fps);
  fpsValue.value = `${fps} FPS`;
  fpsInput.setAttribute("aria-valuetext", `${fps} frames per second`);
}

function syncPlannerCoreSetting(value: string | null = plannerCoresInput.value) {
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

function syncPlannerStrengthSetting(value: string | null = plannerStrengthSelect.value) {
  const setting: PlannerStrengthSetting = value === "baseline" || value === "balanced" || value === "strongest" ? value : "adaptive";
  plannerStrengthSelect.value = setting;
  const cached = adaptivePlannerCalibration();
  plannerStrengthValue.value = setting === "adaptive"
    ? cached ? `Adaptive → ${plannerLevelName(cached.level)}` : "Adaptive"
    : plannerLevelName(setting);
  plannerStrengthHint.firstElementChild!.textContent = setting === "adaptive"
    ? cached ? `Benchmarked: ${plannerLevelName(cached.level)}` : "Benchmarked automatically"
    : setting === "baseline" ? "No tree search" : setting === "balanced" ? "Narrower tree search" : "Complete current search";
}

function plannerLevelName(level: PlannerLevel) {
  return level === "baseline" ? "Quick" : level === "balanced" ? "Tactical" : "Strategic";
}

function adaptivePlannerCalibration(): { level: PlannerLevel; baselineMs: number } | null {
  try {
    const value = JSON.parse(localStorage.getItem("mib_rust_planner_calibration_v2") || "null");
    if (value?.logicalCpus !== REPORTED_LOGICAL_CPUS || !["baseline", "balanced", "strongest"].includes(value?.level)) return null;
    return { level: value.level, baselineMs: Number(value.baselineMs) };
  } catch {
    return null;
  }
}

function calibratedPlannerLevel(baselineMs: number): PlannerLevel {
  const relativeSpeed = REFERENCE_BASELINE_MS / Math.max(1, baselineMs);
  if (relativeSpeed >= STRONGEST_MIN_RELATIVE_SPEED) return "strongest";
  if (relativeSpeed >= BALANCED_MIN_RELATIVE_SPEED) return "balanced";
  return "baseline";
}

function storePlannerCalibration(level: PlannerLevel, baselineMs: number) {
  localStorage.setItem("mib_rust_planner_calibration_v2", JSON.stringify({ logicalCpus: REPORTED_LOGICAL_CPUS, level, baselineMs }));
}

function selectedPlannerLevel(): PlannerLevel | null {
  const setting = plannerStrengthSelect.value as PlannerStrengthSetting;
  return setting === "adaptive" ? adaptivePlannerCalibration()?.level ?? null : setting;
}

function frameDuration() {
  const fps = playbackFps();
  return fps ? 1_000 / fps : 0;
}

function manualMovementRepeatDuration() {
  return 1_000 / Math.min(playbackFps(), MAX_MANUAL_FPS);
}

function framesPerWalkPose() {
  return 0.5;
}

function enhanceHudEquipmentIcon(selector: string, gear: number, kind: "weapon" | "armor") {
  if (gear < 0) return;
  const icon = hud.querySelector<HTMLElement>(selector);
  if (!icon) return;
  const cell = 26 + gear, x = cell % 32 * 32, y = Math.floor(cell / 32) * 32;
  const chip = icon.closest<HTMLElement>(".hud-chip");
  if (kind === "weapon") chip?.classList.add("hud-asset-backed");
  else chip?.classList.add("hud-armor-equipped");
  icon.className = "hud-atlas-icon hud-item-icon-wrap";
  applyEmbeddedHudAtlas(icon);
  icon.style.backgroundPosition = `-${x}px -${y}px`;
}

function applyEmbeddedHudAtlas(root: ParentNode) {
  const atlasUrl = (globalThis as typeof globalThis & { __MIB_RUST_EMBEDDED_ASSETS__?: { atlasUrl: string } }).__MIB_RUST_EMBEDDED_ASSETS__?.atlasUrl;
  if (!atlasUrl) return;
  const icons = root instanceof HTMLElement && root.classList.contains("hud-atlas-icon")
    ? [root]
    : [...root.querySelectorAll<HTMLElement>(".hud-atlas-icon")];
  for (const icon of icons) icon.style.backgroundImage = `url("${atlasUrl}")`;
}

function inventoryIcon(gear: number) {
  const cell=26+gear,x=cell%32*32,y=Math.floor(cell/32)*32;
  return `<span class="hud-atlas-icon" style="background-position:-${x}px -${y}px"></span>`;
}
function renderShop(){const items=simulationSnapshot?.shop??[];shopList.innerHTML=items.map((item,index)=>`<div class="inventory-row${index===shopSelection?" selected":""}">${inventoryIcon(item.gear)}<span>${item.name}</span><b>${item.price} cr</b></div>`).join("")||"No stock.";applyEmbeddedHudAtlas(shopList);}

function renderInventory() {
  const items=simulationSnapshot?.inventory ?? [];
  inventorySelection=Math.max(0,Math.min(items.length-1,inventorySelection));
  const selected = items[inventorySelection];
  const primary = selected ? inventoryPrimaryAction(selected.kind) : null;
  const primaryLabel = selected?.wielded ? "Already wielded"
    : selected?.worn ? "Already worn"
    : primary === "wield" ? "Wield selected"
    : primary === "wear" ? "Wear selected"
    : primary === "eat" ? "Consume selected"
    : primary === "use" ? "Use selected"
    : primary === "aim-throw" ? "Aim selected throw"
    : "No primary action";
  inventoryHint.innerHTML=`${primary && !selected?.wielded && !selected?.worn ? "<kbd>Enter</kbd> " : ""}${primaryLabel} · <kbd>J</kbd><kbd>K</kbd> select · <kbd>W</kbd> wield · <kbd>Shift W</kbd> wear · <kbd>E</kbd> eat · <kbd>U</kbd> use · <kbd>Shift T</kbd> aim throw · <kbd>Esc</kbd> close`;
  inventoryList.innerHTML=items.length?items.map((item,index)=>`<div class="inventory-row${index===inventorySelection?" selected":""}">${inventoryIcon(item.gear)}<strong>${item.name}${item.count>1?` (${item.count})`:""}</strong><span class="inventory-meta">${item.wielded?"wielded":item.worn?"worn":item.kind}</span></div>`).join(""):"Empty pockets.";
  applyEmbeddedHudAtlas(inventoryList);
}

function closeInventory() { inventoryModal.classList.remove("visible"); }

function closeGameplayOverlays() {
  closeInventory();
  shopModal.classList.remove("visible");
  targetMode = null;
}

function missionEnded() {
  return !simulationSnapshot || simulationSnapshot.won || simulationSnapshot.dead;
}

function visualDuration(next: RustRenderSnapshot) {
  const base = presentationDurations.get(next) ?? frameDuration();
  if (projectilePreviews.has(next) || teleportPreviews.has(next)) return base;
  if (next.mobs.some((mob) => mob.state === 7)) return Math.max(base, MOB_DEATH_MS);
  const appearDuration = next.mobs.some((mob) => mob.appeared) ? MOB_APPEAR_MS : 0;
  return Math.max(base, appearDuration);
}

function attachEffects(next: RustRenderSnapshot, effects: RustIsoEffect[]) {
  if (effects.length) effectsBySnapshot.set(next, effects);
}

function enqueueTeleportPresentation(before: RustRenderSnapshot | null, next: RustRenderSnapshot, effects: RustIsoEffect[]) {
  const transition = stageTeleportTransition(before, next);
  if (!transition) return false;
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
  if (!simulationSnapshot) return null;
  const before = simulationSnapshot;
  const sourceX = before.player.cell % before.width;
  const sourceY = Math.floor(before.player.cell / before.width);
  const destination = [...before.seen]
    .map((seen, cell) => ({ seen, cell }))
    .filter(({ seen, cell }) => seen === "1" && before.map[cell] !== "#")
    .sort((left, right) => {
      const distance = (cell: number) => Math.max(Math.abs(cell % before.width - sourceX), Math.abs(Math.floor(cell / before.width) - sourceY));
      return distance(right.cell) - distance(left.cell);
    })[0]?.cell;
  if (destination === undefined || destination === before.player.cell) return null;
  const next: RustRenderSnapshot = {
    ...before,
    frame: before.frame + 1,
    action: "use:pocket universe marble",
    logs: [{ text: "Teleported.", cls: "good" }],
    player: { ...before.player, cell: destination, fromCell: destination, teleported: true },
  };
  enqueueTeleportPresentation(before, next, deriveRustIsoEffects(before, next));
  return { source: before.player.cell, destination };
}

async function replaceRenderer() {
  renderer?.destroy();
  effectLayer?.clear();
  const installCanvases = () => {
    const canvas = document.createElement("canvas");
    canvas.id = "stage";
    canvas.width = 1100;
    canvas.height = 680;
    const effectCanvas = document.createElement("canvas");
    effectCanvas.id = "effects";
    effectCanvas.width = 1100;
    effectCanvas.height = 680;
    canvasHost.replaceChildren(canvas, effectCanvas);
    effectLayer = new RustIsoEffectLayer(effectCanvas);
    return canvas;
  };
  let canvas = installCanvases();
  status.textContent = "Calibrating tactical display…";
  try {
    renderer = await createRustIsoRenderer(canvas, rendererSelect.value as RustIsoRendererKind);
  } catch (error) {
    if (rendererSelect.value !== "webgl2") throw error;
    // Once a canvas has created a WebGL context, browsers will not let it
    // switch to 2D. Android GPU drivers can fail after accepting the context,
    // so retry compatible mode on a fresh canvas instead of leaving a blank map.
    console.warn("Enhanced renderer unavailable; using compatible renderer.", error);
    rendererSelect.value = "canvas2d";
    canvas = installCanvases();
    renderer = await createRustIsoRenderer(canvas, "canvas2d");
  }
  document.body.dataset.renderer = renderer.kind;
  status.textContent = renderer.kind === "canvas2d" ? "Tactical display online (compatible mode)." : "Tactical display online.";
  if (snapshot) draw(snapshot);
}

function preferCompatibleMobileRenderer() {
  return /Android|Mobile/i.test(navigator.userAgent) || matchMedia("(pointer: coarse)").matches;
}

function randomSeed() {
  const requested = Number(new URLSearchParams(location.search).get("seed"));
  if (Number.isInteger(requested) && requested > 0 && requested <= 0xffff_ffff) return requested;
  const value = new Uint32Array(1);
  crypto.getRandomValues(value);
  return value[0] || 1;
}

function showClassPicker() {
  if (autoplay) autoplayButton.click();
  resetEndingSequence();
  classPicker.classList.add("visible");
  status.hidden = false;
  hud.innerHTML = renderPregameHud({ mode: "classpick" });
  status.textContent = "Select an agent profile to begin.";
}

function draw(next: RustRenderSnapshot, transitionMs = 1_000, now = performance.now()) {
  snapshot = next;
  if (!renderer) return;
  const duration = visualDuration(next);
  const progress = duration ? Math.min(1, transitionMs / duration) : 1;
  const presentationFrame = next.frame + progress;
  const walkPose = Math.floor(presentationFrame / framesPerWalkPose());
  const gameClock = walkPose * 100;
  const rendererTransition = progress * 90;
  renderMs = renderer.render(next, rendererTransition, gameClock);
  totalRenderMs += renderMs;
  renderedFrames++;
  if (now - lastStatsAt < 250 && next.frame < next.frameCount) return;
  lastStatsAt = now;
  const nutrition = next.player.nutrition <= 0 ? "Starving" : next.player.nutrition <= 300 ? "Hungry" : "Fed";
  hud.innerHTML = renderHudView({
    hp: next.player.hp, maxHp: next.player.maxHp, floor: String(next.floor), floorTitle: `Floor ${next.floor}`,
    agent: String.fromCharCode(next.player.agent), agentTitle: `L${next.player.level}`,
    weapon: next.player.weapon, weaponTitle: next.player.weapon,
    damage: `${next.player.damageMin}-${next.player.damageMax}`, damageTitle: `Damage ${next.player.damageMin}-${next.player.damageMax}`,
    range: `R${next.player.range}`, rangeTitle: `Range ${next.player.range}`,
    ammo: String(next.player.ammo), ammoTitle: `${next.player.ammo} matching ammo`,
    armor: `AC${next.player.armor}`, armorTitle: `Armor ${next.player.armor}`,
    xpPercent: next.player.xpNext ? next.player.xp / next.player.xpNext * 100 : 0,
    xpTitle: `XP ${next.player.xp}/${next.player.xpNext}`, level: `L${next.player.level}`, credits: `$${next.player.credits}`,
    nutrition, nutritionTitle: `Nutrition ${next.player.nutrition}`, nutritionWarning: next.player.nutrition <= 300,
  });
  enhanceHudEquipmentIcon(".hud-icon-weapon", next.player.weaponGear, "weapon");
  enhanceHudEquipmentIcon(".hud-icon-armor", next.player.armorGear, "armor");
  if (next.won || next.dead) {
    closeGameplayOverlays();
    if (endingFrame === next.frame && now >= endingRevealAt) ending.classList.add("visible");
    endingTitle.textContent = next.won ? "Assignment Complete" : "Assignment Failed";
    endingBody.innerHTML = [
      ["Score", next.score], ["Floor", next.floor], ["Kills", next.player.kills], ["Turns", next.turns],
    ].map(([label, value]) => `<span class="ending-stat"><i class="ending-icon ending-icon-${String(label).toLowerCase()}"></i><small>${label}</small><strong>${value}</strong></span>`).join("");
  } else if (autoplay) status.textContent = `F${next.floor} · Turn ${next.turns} · ${next.action}`;
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
    `display avg  ${(totalRenderMs / renderedFrames).toFixed(3)} ms`,
  ].join("\n");
  stepButton.disabled = next.frame >= next.frameCount;
  if (next.won || next.dead || next.frame >= next.frameCount) {
    autoplay = false;
    autoplayButton.innerHTML = commandButton("auto", "Auto");
    autoplayButton.setAttribute("aria-pressed", "false");
  }
}

function present(next: RustRenderSnapshot) {
  presentationStarted = performance.now();
  snapshot = next;
  // Worker state may run ahead of the presentation queue during autoplay.
  // Emit an event only when its authoritative visual frame reaches the screen;
  // projectile previews intentionally defer their result until impact.
  if (!projectilePreviews.has(next) && !teleportPreviews.has(next) && !loggedSnapshots.has(next)) {
    loggedSnapshots.add(next);
    for (const line of next.logs) appendActionLog(line.text, line.cls);
  }
  prepareEndingSequence(next, presentationStarted);
  const duration = visualDuration(next);
  effectLayer?.play(effectsBySnapshot.get(next) ?? [], presentationStarted, duration || undefined);
  effectsBySnapshot.delete(next);
  if (duration) draw(next, 0, presentationStarted);
  else draw(next, 1_000, presentationStarted);
}

function settlePresentationForManualInput(next: RustRenderSnapshot) {
  const now = performance.now();
  snapshot = next;
  if (!projectilePreviews.has(next) && !teleportPreviews.has(next) && !loggedSnapshots.has(next)) {
    loggedSnapshots.add(next);
    for (const line of next.logs) appendActionLog(line.text, line.cls);
  }
  prepareEndingSequence(next, now);
  effectsBySnapshot.delete(next);
  effectLayer?.clear();
  // A completed snapshot is presented at frame + 1. The following snapshot
  // starts at that same presentation frame, keeping actor sprites and camera
  // state continuous instead of briefly rewinding toward `fromCell`.
  presentationStarted = now - 1_000;
  draw(next, 1_000, now);
}

function pumpVisualQueue() {
  if (presentationPausedAt !== null || visualTimer !== null || !visualQueue.length) return;
  const next = visualQueue.shift()!;
  present(next);
  const duration = visualDuration(next);
  if (duration) {
    visualEndsAt = performance.now() + duration;
    visualTimer = window.setTimeout(completeVisualFrame, duration);
  }
  else queueMicrotask(pumpVisualQueue);
}

function requestAutoplayAfterPresentation() {
  if (!autoplay || pending || presentationPausedAt !== null || visualTimer !== null || visualQueue.length) return;
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

function animationLoop(now: number) {
  const presentationNow = presentationPausedAt ?? now;
  if (snapshot) draw(snapshot, presentationNow - presentationStarted, presentationNow);
  if (renderer) effectLayer?.render(renderer, presentationNow);
  requestAnimationFrame(animationLoop);
}

function requestNext() {
  if (pending || !simulationSnapshot || simulationSnapshot.won || simulationSnapshot.dead || simulationSnapshot.frame >= simulationSnapshot.frameCount) return;
  pending = true;
  if (!simulationSnapshot.policy || simulationSnapshot.policy === "unplanned" || simulationSnapshot.policy === "human") {
    if (!planning) planningBeganAt = performance.now();
    planning = true;
    planningModal.classList.add("visible");
    planningModal.setAttribute("aria-busy", "true");
    document.body.setAttribute("aria-busy", "true");
  }
  worker.postMessage({ type: "next" });
}

function requestManualAction(signature: string) {
  if (autoplay || pending || !simulationSnapshot || simulationSnapshot.won || simulationSnapshot.dead) return false;
  // The simulation can be one presentation frame ahead. Human input always
  // branches from the authoritative Rust state, so discard stale visuals first.
  visualQueue = [];
  if (visualTimer !== null) window.clearTimeout(visualTimer);
  visualTimer = null;
  visualRemaining = 0;
  presentationPausedAt = null;
  settlePresentationForManualInput(simulationSnapshot);
  pending = true;
  manualRequestPending = true;
  worker.postMessage({ type: "action", signature });
  return true;
}

function nearestVisibleHostile(next: RustRenderSnapshot) {
  const px = next.player.cell % next.width, py = Math.floor(next.player.cell / next.width);
  return next.mobs.filter((mob) => !mob.friendly && next.visible[mob.cell] === "1").sort((a, b) => {
    const ax = a.cell % next.width, ay = Math.floor(a.cell / next.width), bx = b.cell % next.width, by = Math.floor(b.cell / next.width);
    return Math.max(Math.abs(ax - px), Math.abs(ay - py)) - Math.max(Math.abs(bx - px), Math.abs(by - py));
  })[0];
}

function workerRequest<T extends RustWorkerResponse["type"]>(helper: Worker, request: RustWorkerRequest, expected: T) {
  return new Promise<Extract<RustWorkerResponse, { type: T }>>((resolve, reject) => {
    const receive = (event: MessageEvent<RustWorkerResponse>) => {
      if (event.data.type === "error") { cleanup(); reject(new Error(event.data.message)); return; }
      if (event.data.type !== expected) return;
      cleanup();
      resolve(event.data as Extract<RustWorkerResponse, { type: T }>);
    };
    const failed = () => { cleanup(); reject(new Error("planning worker failed")); };
    const cleanup = () => {
      helper.removeEventListener("message", receive);
      helper.removeEventListener("error", failed);
    };
    helper.addEventListener("message", receive);
    helper.addEventListener("error", failed);
    helper.postMessage(request);
  });
}

function betterPlan(left: RustPlanEvaluation, right: RustPlanEvaluation) {
  const leftRank = [Number(left.won), left.deepest, left.primary, left.score];
  const rightRank = [Number(right.won), right.deepest, right.primary, right.score];
  for (let index = 0; index < leftRank.length; index++) {
    if (leftRank[index] !== rightRank[index]) return leftRank[index] > rightRank[index];
  }
  return false;
}

async function evaluateInitialPlan(candidateCount: number) {
  const setting = plannerStrengthSelect.value as PlannerStrengthSetting;
  let level: PlannerLevel;
  let baselineMs: number | undefined;
  let calibrationMs: number | undefined;
  let calibrationTimedOut = false;
  const evaluations: RustPlanEvaluation[] = [];
  const candidateMs: number[] = [];
  const evaluationStarted = performance.now();
  const seed = Number(seedInput.value) || 1704334;
  const cls = classSelect.value as RustRenderSnapshot["class"];
  const helpers: Worker[] = [];
  try {
    const cached = setting === "adaptive" ? adaptivePlannerCalibration() : null;
    if (setting !== "adaptive") level = setting;
    else if (cached) {
      level = cached.level;
      baselineMs = cached.baselineMs;
    } else {
      const calibrationWorker = createSimulationWorker();
      planningModal.querySelector("p")!.innerHTML = "Calibrating planning strategy for this CPU.<br>Stand by for deployment.";
      const calibrationStarted = performance.now();
      const timeout = Symbol("calibration-timeout");
      const result = await Promise.race([
        (async () => {
          await workerRequest(calibrationWorker, { type: "start", seed, cls }, "ready");
          const probeStarted = performance.now();
          await workerRequest(calibrationWorker, { type: "benchmark-plan", index: 0, turnCap: CALIBRATION_TURN_CAP }, "plan-evaluation");
          return performance.now() - probeStarted;
        })(),
        new Promise<typeof timeout>((resolve) => window.setTimeout(() => resolve(timeout), CALIBRATION_BUDGET_MS)),
      ]);
      calibrationMs = performance.now() - calibrationStarted;
      calibrationWorker.terminate();
      if (result === timeout) {
        calibrationTimedOut = true;
        baselineMs = REFERENCE_BASELINE_MS / (BALANCED_MIN_RELATIVE_SPEED * .9);
        level = "baseline";
      } else {
        // A typical successful baseline run finishes inside this 600-turn
        // sample already. Treat its measured wall time directly; scaling it
        // to the 3,600 safety cap would misclassify fast CPUs as slow.
        baselineMs = result;
        level = calibratedPlannerLevel(baselineMs);
      }
      storePlannerCalibration(level, baselineMs);
      plannerStrengthValue.value = `Adaptive → ${plannerLevelName(level)}`;
      plannerStrengthHint.firstElementChild!.textContent = calibrationTimedOut
        ? "CPU probe capped: Quick"
        : `CPU score: ${(REFERENCE_BASELINE_MS / baselineMs * 100).toFixed(0)}%`;
    }
    const candidateIndices = level === "baseline"
      ? [0]
      : level === "balanced"
        ? Array.from({ length: Math.min(7, candidateCount) }, (_, index) => index)
        : Array.from({ length: candidateCount }, (_, index) => index);
    const coreCount = Math.min(plannerCoreCount(), candidateIndices.length);
    planningModal.querySelector("p")!.innerHTML = `${plannerLevelName(level)} planning is evaluating ${candidateIndices.length} ${candidateIndices.length === 1 ? "route" : "routes"} on ${coreCount} CPU ${coreCount === 1 ? "core" : "cores"}.<br>Stand by for deployment.`;
    while (helpers.length < coreCount) helpers.push(createSimulationWorker());
    await Promise.all(helpers.map((helper, index) => index === 0 && evaluations[0]
      ? Promise.resolve()
      : workerRequest(helper, { type: "start", seed, cls }, "ready")));
    let nextCandidate = 0;
    await Promise.all(helpers.map(async (helper) => {
      while (nextCandidate < candidateIndices.length) {
        const index = candidateIndices[nextCandidate++];
        if (evaluations[index]) continue;
        const candidateStarted = performance.now();
        const message = await workerRequest(helper, { type: "evaluate-plan", index }, "plan-evaluation");
        candidateMs[index] = performance.now() - candidateStarted;
        evaluations[index] = message.evaluation;
      }
    }));
  } finally {
    helpers.forEach((helper) => helper.terminate());
  }
  const candidateIndices = level === "baseline"
    ? [0]
    : level === "balanced"
      ? Array.from({ length: Math.min(7, candidateCount) }, (_, index) => index)
      : Array.from({ length: candidateCount }, (_, index) => index);
  if (candidateIndices.some((index) => !evaluations[index])) throw new Error("parallel planning returned an incomplete policy set");
  const coreCount = Math.min(plannerCoreCount(), candidateIndices.length);
  planningTelemetry = { cores: coreCount, candidates: candidateIndices.length, evaluationMs: performance.now() - evaluationStarted, candidateMs, level, baselineMs, calibrationMs, calibrationTimedOut };
  let best = evaluations[candidateIndices[0]];
  for (const index of candidateIndices.slice(1)) {
    if (betterPlan(evaluations[index], best)) best = evaluations[index];
  }
  worker.postMessage({ type: "install-plan", index: best.index } satisfies RustWorkerRequest);
}

worker.onmessage = (event: MessageEvent<RustWorkerResponse>) => {
  const message = event.data;
  if (message.type === "recommendation") {
    recommendationWaiters.get(message.requestId)?.(message.signature);
    recommendationWaiters.delete(message.requestId);
    return;
  }
  if (message.type === "plan-needed") {
    const selectedLevel = selectedPlannerLevel();
    if (selectedLevel && (!simulationSnapshot || simulationSnapshot.frame > 0 || plannerCoreCount() === 1)) {
      planningModal.querySelector("p")!.innerHTML = `${plannerLevelName(selectedLevel)} planning is preparing the next orders.<br>Stand by for deployment.`;
      worker.postMessage({ type: "plan-strategy", strategy: selectedLevel } satisfies RustWorkerRequest);
    } else {
      void evaluateInitialPlan(message.candidates).catch((error) => {
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
    worker.postMessage({ type: "next" } satisfies RustWorkerRequest);
    return;
  }
  pending = false;
  if (planning) {
    if (planningTelemetry && planningBeganAt) planningTelemetry.totalMs = performance.now() - planningBeganAt;
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
    if (visualTimer !== null) window.clearTimeout(visualTimer);
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
      const travelDuration = isThrow
        ? THROW_EFFECT_MS
        : Math.min(TRACER_EFFECT_MS, Math.max(1, currentFrameDuration * 0.45));
      attachEffects(projectilePreview, travel);
      presentationDurations.set(projectilePreview, travelDuration);
      projectilePreviews.add(projectilePreview);
      attachEffects(message.snapshot, impact);
      if (!isThrow) presentationDurations.set(message.snapshot, Math.max(1, currentFrameDuration - travelDuration));
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
    if (presentationPausedAt !== null) presentationStarted += now - presentationPausedAt;
    presentationPausedAt = null;
    if (visualRemaining > 0) {
      visualEndsAt = now + visualRemaining;
      visualTimer = window.setTimeout(completeVisualFrame, visualRemaining);
    } else pumpVisualQueue();
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
  const button = event.target instanceof Element ? event.target.closest<HTMLButtonElement>("[data-class]") : null;
  if (!button?.dataset.class) return;
  classSelect.value = button.dataset.class;
  seedInput.value = String(randomSeed());
  classPicker.classList.remove("visible");
  startButton.click();
});
settingsButton.addEventListener("click", () => {
  resumeAfterSettings = autoplay;
  if (autoplay) autoplayButton.click();
  settingsModal.classList.add("visible");
});
settingsClose.addEventListener("click", () => {
  settingsModal.classList.remove("visible");
  localStorage.setItem("mib_rust_steps_per_second", fpsInput.value);
  localStorage.setItem("mib_rust_planner_cores", plannerCoresInput.value);
  localStorage.setItem("mib_rust_planner_strength", plannerStrengthSelect.value);
  if (resumeAfterSettings) autoplayButton.click();
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
  const side = document.body.dataset.logLayout !== "side";
  document.body.dataset.logLayout = side ? "side" : "below";
  layoutLogButton.setAttribute("aria-pressed", String(side));
  layoutLogButton.innerHTML = commandButton("below", side ? "Side" : "Below");
  localStorage.setItem("mib_rust_log_layout", side ? "side" : "below");
  renderActionLog();
  if (snapshot) draw(snapshot, 1_000, performance.now());
});
helpButton.addEventListener("click", () => {
  resumeAfterSettings = autoplay;
  if (autoplay) autoplayButton.click();
  helpModal.classList.add("visible");
});
helpClose.addEventListener("click", () => {
  helpModal.classList.remove("visible");
  if (resumeAfterSettings) autoplayButton.click();
  resumeAfterSettings = false;
});
document.addEventListener("keydown", (event) => {
  if (planning) { event.preventDefault(); event.stopImmediatePropagation(); return; }
  if (classPicker.classList.contains("visible") && /^[arvtm]$/i.test(event.key)) {
    classPicker.querySelector<HTMLButtonElement>(`[data-class="${event.key.toLowerCase()}"]`)?.click();
  }
  if (event.key === "Escape") {
    if (settingsModal.classList.contains("visible")) settingsClose.click();
    if (helpModal.classList.contains("visible")) helpClose.click();
  }
  if (event.key === "?" && !classPicker.classList.contains("visible")) {
    event.preventDefault();
    if (helpModal.classList.contains("visible")) helpClose.click(); else helpButton.click();
    return;
  }
  if (classPicker.classList.contains("visible") || settingsModal.classList.contains("visible") || helpModal.classList.contains("visible")) return;
  if (missionEnded() || autoplay) {
    closeGameplayOverlays();
    return;
  }
  if(shopModal.classList.contains("visible")){
    event.preventDefault();const items=simulationSnapshot?.shop??[];
    if(event.key==="Escape"){shopModal.classList.remove("visible");return;}
    if(event.key==="j"||event.key==="ArrowDown"){shopSelection=Math.min(items.length-1,shopSelection+1);renderShop();return;}
    if(event.key==="k"||event.key==="ArrowUp"){shopSelection=Math.max(0,shopSelection-1);renderShop();return;}
    if(event.key==="Enter"&&items[shopSelection]){const name=items[shopSelection].name;shopModal.classList.remove("visible");requestManualAction(`buy:${name}`);}return;
  }
  if (inventoryModal.classList.contains("visible")) {
    const items=simulationSnapshot?.inventory ?? [];
    if (event.key==="Escape"||event.key==="i"){event.preventDefault();closeInventory();return;}
    if(event.key==="j"||event.key==="ArrowDown"){event.preventDefault();inventorySelection=Math.min(items.length-1,inventorySelection+1);renderInventory();return;}
    if(event.key==="k"||event.key==="ArrowUp"){event.preventDefault();inventorySelection=Math.max(0,inventorySelection-1);renderInventory();return;}
    const item=items[inventorySelection]; if(!item)return;
    if(event.key==="Enter") {
      event.preventDefault();
      if(item.wielded||item.worn)return;
      const primary=inventoryPrimaryAction(item.kind);
      if(primary==="aim-throw"){closeInventory();targetMode={action:"throw",item:item.name,cell:simulationSnapshot!.player.cell};status.hidden=false;status.innerHTML="🎯 Aim throw · <kbd>Arrows</kbd> move · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel";return;}
      const action=primary==="wield"?`wield:${item.name}`:primary==="wear"?`wear:${item.name}`:primary==="eat"?`eat:${item.name}`:primary==="use"?`use:${item.name}`:null;
      if(action){closeInventory();requestManualAction(action);}
      return;
    }
    if(event.key==="T") { event.preventDefault(); closeInventory(); targetMode={action:"throw",item:item.name,cell:simulationSnapshot!.player.cell}; status.hidden=false; status.innerHTML="🎯 Aim throw · <kbd>Arrows</kbd> move · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel"; return; }
    const action=event.key==="w"?`wield:${item.name}`:event.key==="W"?`wear:${item.name}`:event.key==="e"?`eat:${item.name}`:event.key==="u"?`use:${item.name}`:null;
    if(action){event.preventDefault();closeInventory();requestManualAction(action);return;}
    return;
  }
  if (targetMode && simulationSnapshot) {
    event.preventDefault();
    if(event.key==="Escape"){targetMode=null;status.hidden=true;return;}
    if(event.key==="Enter"||event.key==="f"){
      const x=targetMode.cell%simulationSnapshot.width,y=Math.floor(targetMode.cell/simulationSnapshot.width);
      const signature=targetMode.action==="fire"?`fire:${x},${y}`:`throw:${targetMode.item}:${x},${y}`;
      targetMode=null;status.hidden=true;requestManualAction(signature);return;
    }
    const moves:Record<string,[number,number]>={ArrowLeft:[-1,0],h:[-1,0],ArrowRight:[1,0],l:[1,0],ArrowUp:[0,-1],k:[0,-1],ArrowDown:[0,1],j:[0,1],y:[-1,-1],u:[1,-1],b:[-1,1],n:[1,1]};
    const move=moves[event.key];if(move){const x=Math.max(0,Math.min(simulationSnapshot.width-1,targetMode.cell%simulationSnapshot.width+move[0]));const y=Math.max(0,Math.min(simulationSnapshot.height-1,Math.floor(targetMode.cell/simulationSnapshot.width)+move[1]));targetMode.cell=y*simulationSnapshot.width+x;status.hidden=false;status.innerHTML=`🎯 Target ${x},${y} · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel`;}
    return;
  }
  if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
  const movement: Record<string, string> = { ArrowLeft: "h", ArrowDown: "j", ArrowUp: "k", ArrowRight: "l", h: "h", j: "j", k: "k", l: "l", y: "y", u: "u", b: "b", n: "n" };
  const key = movement[event.key];
  if (key) {
    event.preventDefault();
    const now = performance.now();
    // Some browsers and embedded shells do not reliably mark synthesized or
    // platform key-repeat events. Pace every movement keydown so neither OS
    // repeat settings nor rapid dispatch can exceed the manual turn limit.
    if (now - lastManualMovementAt < manualMovementRepeatDuration()) return;
    if (requestManualAction(`command:${key}`)) lastManualMovementAt = now;
    return;
  }
  if (event.key === "." || event.key === " ") { event.preventDefault(); requestManualAction("command:."); return; }
  if (event.key === "g") { event.preventDefault(); requestManualAction("command:g"); return; }
  if (event.key === "i") { event.preventDefault(); inventorySelection=0; renderInventory(); inventoryModal.classList.add("visible"); return; }
  if ((event.key === "p" || event.key === "Enter") && simulationSnapshot?.map[simulationSnapshot.player.cell] === "_" && simulationSnapshot.shop.length) { event.preventDefault(); shopSelection=0; renderShop(); shopModal.classList.add("visible"); return; }
  if (event.key === "Enter" && simulationSnapshot) {
    event.preventDefault();
    const tile = simulationSnapshot.map[simulationSnapshot.player.cell];
    requestManualAction(`command:${tile === "<" ? "<" : ">"}`);
    return;
  }
  if (event.key === "F" && simulationSnapshot) { event.preventDefault(); targetMode={action:"fire",cell:simulationSnapshot.player.cell}; status.hidden=false; status.innerHTML="🎯 Aim fire · <kbd>Arrows</kbd> move · <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel"; return; }
  if (event.key === "f" && simulationSnapshot) {
    const target = nearestVisibleHostile(simulationSnapshot);
    if (target) { event.preventDefault(); requestManualAction(`fire:${target.cell % simulationSnapshot.width},${Math.floor(target.cell / simulationSnapshot.width)}`); }
    return;
  }
  if (["c", "B", "P"].includes(event.key)) { event.preventDefault(); requestManualAction(`command:${event.key}`); }
});
rendererSelect.addEventListener("change", () => void replaceRenderer());
window.addEventListener("resize", () => {
  renderActionLog();
  if (snapshot) draw(snapshot, 1_000, performance.now());
});

if (preferCompatibleMobileRenderer()) rendererSelect.value = "canvas2d";
await replaceRenderer();
requestAnimationFrame(animationLoop);
syncFpsSetting(localStorage.getItem("mib_rust_steps_per_second"));
syncPlannerCoreSetting(localStorage.getItem("mib_rust_planner_cores"));
syncPlannerStrengthSetting(localStorage.getItem("mib_rust_planner_strength"));
document.body.dataset.logLayout = localStorage.getItem("mib_rust_log_layout") === "below" ? "below" : "side";
layoutLogButton.setAttribute("aria-pressed", String(document.body.dataset.logLayout === "side"));
status.textContent = "Select an agent profile to begin.";
hud.innerHTML = renderPregameHud({ mode: "classpick" });
document.body.dataset.rustReady = "true";
