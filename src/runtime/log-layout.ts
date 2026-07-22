export type LogLayout = "side" | "below";

export function storedLogLayout(value: string | null): LogLayout {
  return value === "below" ? "below" : "side";
}

export function nextLogLayout(value: string | undefined): LogLayout {
  return value === "side" ? "below" : "side";
}

export function logLayoutButtonState(layout: LogLayout) {
  return { label: layout === "side" ? "Side" : "Below", pressed: layout === "side" };
}
