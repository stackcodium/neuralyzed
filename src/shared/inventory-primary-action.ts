export type InventoryPrimaryKind = "weapon" | "armor" | "food" | "pill" | "tool" | "thrown" | "ammo" | "quest";
export type InventoryPrimaryAction = "wield" | "wear" | "eat" | "use" | "aim-throw";

export function inventoryPrimaryAction(kind: InventoryPrimaryKind): InventoryPrimaryAction | null {
  if (kind === "weapon") return "wield";
  if (kind === "armor") return "wear";
  if (kind === "food" || kind === "pill") return "eat";
  if (kind === "tool") return "use";
  if (kind === "thrown") return "aim-throw";
  return null;
}
