import { describe, expect, test } from "bun:test";
import type { RustRenderSnapshot } from "../runtime/rust-wasm-protocol";
import { cameraTarget, presentedMobAtlasCell, presentedPlayerPose, shouldRenderMob } from "./rust-iso-atlas-renderer";

describe("Rust ISO floor-entry camera", () => {
  test("focuses the new-floor destination instead of the previous floor coordinate", () => {
    const width = 64;
    const snapshot = {
      width,
      height: 64,
      frame: 91,
      floor: 3,
      seen: "0".repeat(width * 64),
      player: {
        fromCell: width + 1,
        cell: width * 32 + 32,
      },
    } as RustRenderSnapshot;

    expect(cameraTarget(snapshot, 0, false)).toMatchObject({ x: 0, y: 0 });
    expect(cameraTarget(snapshot, 0, true)).toMatchObject({ x: 21, y: 21 });
    expect(cameraTarget(snapshot, 89, true)).toMatchObject({ x: 21, y: 21 });
  });
});

describe("Rust ISO combat facing", () => {
  test("finishes movement, then faces a visible defeated enemy during its hold", () => {
    const snapshot = {
      width: 64,
      player: { cell: 64 * 10 + 10, state: 1, direction: 3 },
      mobs: [{ cell: 64 * 10 + 14, state: 7 }],
    } as RustRenderSnapshot;

    expect(presentedPlayerPose(snapshot, 89)).toEqual({ state: 1, direction: 3 });
    expect(presentedPlayerPose(snapshot, 90)).toEqual({ state: 0, direction: 1 });
    expect(presentedPlayerPose(snapshot, 700)).toEqual({ state: 0, direction: 1 });
  });

  test("does not override ordinary movement when no defeated enemy is visible", () => {
    const snapshot = {
      width: 64,
      player: { cell: 64 * 10 + 10, state: 1, direction: 3 },
      mobs: [{ cell: 64 * 10 + 14, state: 3 }],
    } as RustRenderSnapshot;

    expect(presentedPlayerPose(snapshot, 700)).toEqual({ state: 1, direction: 3 });
  });

  test("keeps every shooting frame aimed at the authoritative target direction", () => {
    const snapshot = {
      width: 64,
      player: { cell: 64 * 10 + 10, state: 3, direction: 1 },
      // A held corpse in the opposite direction previously stole the final
      // shooting frames after the movement transition boundary.
      mobs: [{ cell: 64 * 10 + 6, state: 7 }],
    } as RustRenderSnapshot;

    expect(presentedPlayerPose(snapshot, 0)).toEqual({ state: 3, direction: 1 });
    expect(presentedPlayerPose(snapshot, 89)).toEqual({ state: 3, direction: 1 });
    expect(presentedPlayerPose(snapshot, 90)).toEqual({ state: 3, direction: 1 });
    expect(presentedPlayerPose(snapshot, 700)).toEqual({ state: 3, direction: 1 });
  });
});

describe("Rust ISO friendly backup presentation", () => {
  test("never reuses the active player's exact atlas appearance", () => {
    const snapshot = {
      class: "a",
      mobs: [{ friendly: true, state: 2, direction: 1 }],
    } as RustRenderSnapshot;
    const backup = { ...snapshot.mobs[0], friendly: true, state: 2, direction: 1 };
    const activePlayerShootCell = 58 + 1 * 10 + 7;

    expect(presentedMobAtlasCell(snapshot, backup)).not.toBe(activePlayerShootCell);
    expect(presentedMobAtlasCell(snapshot, backup)).toBe(58 + 40 + 1 * 10 + 7);
  });
});

describe("Rust ISO nearby actor continuity", () => {
  const snapshot = (state: number, spotted = true, distance = 3) => ({
    width: 64,
    player: { cell: 64 * 10 + 10 },
    seen: "1".repeat(64 * 36),
    visible: "0".repeat(64 * 36),
    mobs: [{ cell: 64 * 10 + 10 + distance, state, spotted }],
  } as RustRenderSnapshot);

  test.each([4, 5, 6])("keeps spotted immobilized state %i visible nearby", (state) => {
    const view = snapshot(state);
    expect(shouldRenderMob(view, view.mobs[0])).toBe(true);
  });

  test("does not reveal active, unspotted, or distant actors outside line of sight", () => {
    for (const view of [snapshot(0), snapshot(5, false), snapshot(5, true, 10)]) {
      expect(shouldRenderMob(view, view.mobs[0])).toBe(false);
    }
  });
});
