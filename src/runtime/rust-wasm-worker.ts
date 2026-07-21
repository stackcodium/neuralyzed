/// <reference lib="webworker" />

import type { RustRenderSnapshot, RustWorkerRequest, RustWorkerResponse } from "./rust-wasm-protocol";

type Episode = {
  snapshot_json(): string;
  next_snapshot_json(): string;
  reset(): string;
  seek_snapshot_json(frame: number): string;
  apply_action_signature_json(signature: string): string;
  recommend_action_signature?: () => string;
  selected_policy?: () => string;
  needs_plan?: () => boolean;
  planning_candidate_count?: () => number;
  evaluate_planning_candidate_json?: (index: number) => string;
  benchmark_planning_candidate_json?: (index: number, turnCap: number) => string;
  install_planning_candidate?: (index: number) => void;
  plan_strategy?: (strategy: string) => void;
  free(): void;
};

let episode: Episode | null = null;
let wasmModule: Promise<any> | null = null;

function post(message: RustWorkerResponse) {
  self.postMessage(message);
}

function parseSnapshot(json: string): RustRenderSnapshot {
  const snapshot = JSON.parse(json) as RustRenderSnapshot;
  if (episode?.selected_policy) snapshot.policy = episode.selected_policy();
  return snapshot;
}

async function loadWasm() {
  if (!wasmModule) {
    wasmModule = import("../../dist/rust-wasm/mib_rust_wasm.js").then(async (module) => {
      const embedded = (self as typeof self & { __MIB_RUST_WASM_BASE64__?: string }).__MIB_RUST_WASM_BASE64__;
      if (embedded) {
        const raw = atob(embedded);
        const bytes = new Uint8Array(raw.length);
        for (let index = 0; index < raw.length; index++) bytes[index] = raw.charCodeAt(index);
        await module.default(bytes);
      } else await module.default(new URL("./rust-wasm/mib_rust_wasm_bg.wasm?v=20260721.1", import.meta.url));
      return module;
    });
  }
  return wasmModule;
}

self.onmessage = async (event: MessageEvent<RustWorkerRequest>) => {
  try {
    const request = event.data;
    if (request.type === "start") {
      const started = performance.now();
      const loadStarted = performance.now();
      const module = await loadWasm();
      const wasmMs = performance.now() - loadStarted;
      episode?.free();
      episode = request.e2e ? new module.RustE2eEpisode(request.seed, request.cls) : new module.RustEpisode(request.seed, request.cls);
      const snapshot = parseSnapshot(episode.snapshot_json());
      post({ type: "ready", snapshot, initMs: performance.now() - started, wasmMs });
      return;
    }
    if (!episode) throw new Error("No mission is currently active.");
    if (request.type === "recommend") {
      if (!episode.recommend_action_signature) throw new Error("recommendations require E2E mode");
      post({ type: "recommendation", requestId: request.requestId, signature: episode.recommend_action_signature() });
      return;
    }
    if (request.type === "evaluate-plan") {
      if (!episode.evaluate_planning_candidate_json) throw new Error("parallel planning is unavailable");
      post({ type: "plan-evaluation", evaluation: JSON.parse(episode.evaluate_planning_candidate_json(request.index)) });
      return;
    }
    if (request.type === "benchmark-plan") {
      if (!episode.benchmark_planning_candidate_json) throw new Error("planning benchmark is unavailable");
      post({ type: "plan-evaluation", evaluation: JSON.parse(episode.benchmark_planning_candidate_json(request.index, request.turnCap)) });
      return;
    }
    if (request.type === "install-plan") {
      if (!episode.install_planning_candidate) throw new Error("parallel planning is unavailable");
      episode.install_planning_candidate(request.index);
      post({ type: "plan-installed", index: request.index });
      return;
    }
    if (request.type === "plan-strategy") {
      if (!episode.plan_strategy) throw new Error("planning strategies are unavailable");
      episode.plan_strategy(request.strategy);
      post({ type: "plan-installed", index: -1 });
      return;
    }
    if (request.type === "next" && episode.needs_plan?.()) {
      post({ type: "plan-needed", candidates: episode.planning_candidate_count?.() ?? 0 });
      return;
    }
    const started = performance.now();
    const json = request.type === "next" || request.type === "next-serial" ? episode.next_snapshot_json()
      : request.type === "reset" ? episode.reset()
        : request.type === "action" ? episode.apply_action_signature_json(request.signature)
        : episode.seek_snapshot_json(request.frame);
    post({ type: "frame", snapshot: parseSnapshot(json), snapshotMs: performance.now() - started });
  } catch (error) {
    post({ type: "error", message: error instanceof Error ? error.message : String(error) });
  }
};
