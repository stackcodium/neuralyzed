import { describe, expect, test } from "bun:test";
import type { RustRenderSnapshot } from "../runtime/rust-wasm-protocol";
import { deriveRustIsoEffects, rustIsoEffectTimelineMs, rustIsoEffectTimeScale, splitRangedEffects, splitTeleportEffects, stageRangedImpactTransition, stageTeleportTransition, stageThrownFreezeTransition, TELEPORT_IN_MS, TELEPORT_OUT_MS, THROW_EFFECT_MS, TRACER_EFFECT_MS } from "./rust-iso-effects";

function snapshot(hp: number, action: string, logs: RustRenderSnapshot["logs"]): RustRenderSnapshot {
  return {
    floor: 15,
    width: 64,
    height: 36,
    action,
    logs,
    won: false,
    dead: false,
    player: { hp, cell: 10 },
    mobs: [],
  } as unknown as RustRenderSnapshot;
}

describe("Rust ISO healing effects", () => {
  test("does not draw an action-style wave for passive movement regeneration", () => {
    const before = snapshot(37, "command:l:approach boss by shortest arena route", []);
    const next = snapshot(38, "command:l:approach boss by shortest arena route", [
      { text: "Recovered 1 HP.", cls: "good" },
    ]);

    expect(deriveRustIsoEffects(before, next).some((effect) => effect.kind === "heal")).toBe(false);
  });

  test("keeps the healing wave for an explicit healing action", () => {
    const before = snapshot(30, "command:l", []);
    const next = snapshot(36, "eat:royal jelly", [
      { text: "Recovered 6 HP.", cls: "good" },
      { text: "Consumed royal jelly." },
    ]);

    expect(deriveRustIsoEffects(before, next)).toContainEqual({
      kind: "heal",
      cell: 10,
      magnitude: 6,
      delay: 0,
    });
  });

  test("keeps player damage and rewards that occur while entering a floor", () => {
    const before = snapshot(40, "command:>", []);
    before.floor = 5;
    before.mobs = [{ uid: 7, cell: 140, hp: 9 } as RustRenderSnapshot["mobs"][number]];
    const next = snapshot(35, "command:>", [
      { text: "F6: New operational sector entered.", cls: "warn" },
      { text: "MIB supplies received.", cls: "good" },
    ]);
    next.floor = 6;

    const effects = deriveRustIsoEffects(before, next);
    expect(effects.map((effect) => effect.kind)).toEqual(["player-damage", "reward"]);
    expect(effects.some((effect) => effect.cell === 140)).toBe(false);
  });
});

describe("Rust ISO containment-foam timing", () => {
  test("holds the previous enemy pose until the shortened throw reaches its target", () => {
    const before = snapshot(40, "command:l", []);
    before.mobs = [{ uid: 7, cell: 140, fromCell: 140, state: 0, direction: 2, frozen: 0 } as RustRenderSnapshot["mobs"][number]];
    const next = snapshot(40, "throw:containment foam grenade:12,2", []);
    next.mobs = [{ uid: 7, cell: 140, fromCell: 140, state: 4, direction: 2, frozen: 11 } as RustRenderSnapshot["mobs"][number]];

    const effects = deriveRustIsoEffects(before, next);
    const staged = stageThrownFreezeTransition(before, next);

    expect(THROW_EFFECT_MS).toBe(240);
    expect(effects).toContainEqual({ kind: "throw", cell: 10, targetCell: 140, delay: 0 });
    expect(staged?.mobs[0].state).toBe(0);
    expect(staged?.mobs[0].frozen).toBe(0);
    expect(next.mobs[0].state).toBe(4);
    expect(next.mobs[0].frozen).toBe(11);
  });
});

describe("Rust ISO teleport presentation", () => {
  test("stages a fixed-duration departure and arrival without changing simulation state", () => {
    const before = snapshot(40, "command:l", []);
    before.player = { ...before.player, cell: 10, fromCell: 9, state: 1, teleported: false };
    const next = snapshot(40, "use:pocket universe marble", [{ text: "Teleported.", cls: "good" }]);
    next.player = { ...next.player, cell: 420, fromCell: 420, teleported: true };

    const effects = deriveRustIsoEffects(before, next);
    const split = splitTeleportEffects(effects);
    const staged = stageTeleportTransition(before, next);

    expect(TELEPORT_OUT_MS).toBe(520);
    expect(TELEPORT_IN_MS).toBe(620);
    expect(split.departure).toEqual([{ kind: "teleport-out", cell: 10, delay: 0 }]);
    expect(split.arrival).toEqual([{ kind: "teleport-in", cell: 420, delay: 0 }]);
    expect(staged?.departure.player.cell).toBe(10);
    expect(staged?.departure.player.fromCell).toBe(10);
    expect(staged?.departure.player.state).toBe(0);
    expect(staged?.departure.player.teleportPhase).toBe("out");
    expect(staged?.arrival.player.cell).toBe(420);
    expect(staged?.arrival.player.fromCell).toBe(420);
    expect(staged?.arrival.player.state).toBe(0);
    expect(staged?.arrival.player.teleportPhase).toBe("in");
    expect(before.player.fromCell).toBe(9);
    expect(before.player.state).toBe(1);
    expect(next.player.teleportPhase).toBeUndefined();
  });
});

describe("Rust ISO ranged-impact timing", () => {
  test("fits ordinary effects inside the FPS frame without shortening terminal effects", () => {
    expect(rustIsoEffectTimeScale([{ kind: "damage", cell: 10 }], 1_000 / 6)).toBeCloseTo((1_000 / 6) / 420);
    expect(rustIsoEffectTimeScale([{ kind: "victory", cell: 10 }], 1_000 / 6)).toBe(1);
  });

  test("finishes projectile travel before starting damage and death effects", () => {
    const before = snapshot(40, "command:l", []);
    before.mobs = [{ uid: 7, cell: 140, fromCell: 140, state: 0, direction: 2, hp: 9, frozen: 0 } as RustRenderSnapshot["mobs"][number]];
    const next = snapshot(40, "fire:12,2", []);
    next.mobs = [{ uid: 7, cell: 140, fromCell: 140, state: 7, direction: 2, hp: 0, frozen: 0 } as RustRenderSnapshot["mobs"][number]];

    const effects = deriveRustIsoEffects(before, next);
    const { travel, impact } = splitRangedEffects(effects);
    const staged = stageRangedImpactTransition(before, next);

    expect(TRACER_EFFECT_MS).toBe(90);
    expect(travel).toEqual([{ kind: "tracer", cell: 10, targetCell: 140, delay: 0 }]);
    expect(impact.map((effect) => effect.kind)).toEqual(["damage", "kill"]);
    expect(staged?.mobs[0].state).toBe(0);
    expect(staged?.mobs[0].hp).toBe(9);
    expect(next.mobs[0].state).toBe(7);
    expect(next.mobs[0].hp).toBe(0);
  });

  test("finishes hit and kill effects before the corpse removal boundary", () => {
    const before = snapshot(40, "command:l", []);
    before.mobs = [{ uid: 7, cell: 140, hp: 9 } as RustRenderSnapshot["mobs"][number]];
    const next = snapshot(40, "command:l", []);
    next.mobs = [{ uid: 7, cell: 140, hp: 0 } as RustRenderSnapshot["mobs"][number]];

    expect(rustIsoEffectTimelineMs(deriveRustIsoEffects(before, next))).toBe(660);
    expect(rustIsoEffectTimelineMs(deriveRustIsoEffects(before, next))).toBeLessThan(760);
  });
});
