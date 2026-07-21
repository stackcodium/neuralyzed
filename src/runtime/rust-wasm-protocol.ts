export type RustRenderSnapshot = {
  version: 1;
  seed: number;
  class: "a" | "r" | "v" | "t" | "m";
  width: 64;
  height: 36;
  frame: number;
  frameCount: number;
  floor: number;
  turns: number;
  score: number;
  won: boolean;
  dead: boolean;
  action: string;
  logs: Array<{ text: string; cls?: "good" | "warn" | "flash" }>;
  policy?: string;
  inventory: Array<{ name: string; kind: "weapon" | "armor" | "food" | "pill" | "ammo" | "tool" | "thrown" | "quest"; count: number; gear: number; wielded: boolean; worn: boolean }>;
  shop: Array<{ name: string; gear: number; price: number }>;
  player: { cell: number; fromCell: number; teleported: boolean; teleportPhase?: "out" | "in"; state: number; direction: number; agent: number; hp: number; maxHp: number; level: number; xp: number; xpNext: number; credits: number; nutrition: number; kills: number; weapon: string; weaponGear: number; damageMin: number; damageMax: number; range: number; ammo: number; armor: number; armorGear: number };
  map: string;
  seen: string;
  visible: string;
  items: Array<{ cell: number; gear: number; name: string }>;
  mobs: Array<{
    uid: number;
    cell: number;
    fromCell: number;
    state: number;
    direction: number;
    appeared: boolean;
    kind: number;
    name: string;
    hp: number;
    maxHp: number;
    boss: boolean;
    friendly: boolean;
    spotted: boolean;
    asleep: boolean;
    pacified: boolean;
    frozen: number;
  }>;
};

export type RustWorkerRequest =
  | { type: "start"; seed: number; cls: RustRenderSnapshot["class"]; e2e?: boolean }
  | { type: "next" }
  | { type: "next-serial" }
  | { type: "evaluate-plan"; index: number }
  | { type: "benchmark-plan"; index: number; turnCap: number }
  | { type: "install-plan"; index: number }
  | { type: "plan-strategy"; strategy: "baseline" | "balanced" | "strongest" }
  | { type: "reset" }
  | { type: "action"; signature: string }
  | { type: "recommend"; requestId: number }
  | { type: "seek"; frame: number };

export type RustWorkerResponse =
  | { type: "ready"; snapshot: RustRenderSnapshot; initMs: number; wasmMs: number }
  | { type: "frame"; snapshot: RustRenderSnapshot; snapshotMs: number }
  | { type: "recommendation"; requestId: number; signature: string }
  | { type: "plan-needed"; candidates: number }
  | { type: "plan-evaluation"; evaluation: RustPlanEvaluation }
  | { type: "plan-installed"; index: number }
  | { type: "error"; message: string };

export type RustPlanEvaluation = {
  index: number;
  policy: string;
  won: boolean;
  deepest: number;
  primary: number;
  score: number;
  turns: number;
};
