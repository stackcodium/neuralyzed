import { describe, expect, test } from "bun:test";
import { logLayoutButtonState, nextLogLayout, storedLogLayout } from "./log-layout";

describe("field log layout", () => {
  test("restores only the persisted below layout", () => {
    expect(storedLogLayout("below")).toBe("below");
    expect(storedLogLayout("side")).toBe("side");
    expect(storedLogLayout(null)).toBe("side");
    expect(storedLogLayout("invalid")).toBe("side");
  });

  test("keeps the button label and pressed state aligned with the layout", () => {
    expect(logLayoutButtonState("side")).toEqual({ label: "Side", pressed: true });
    expect(logLayoutButtonState("below")).toEqual({ label: "Below", pressed: false });
    expect(nextLogLayout("side")).toBe("below");
    expect(nextLogLayout("below")).toBe("side");
  });
});
