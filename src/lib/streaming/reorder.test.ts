import { describe, expect, it } from "vitest";
import { moveItemAt, slotHitY, slotIndexFromClientY, slotShiftY } from "./reorder";

describe("moveItemAt", () => {
  it("moves an item forward", () => {
    expect(moveItemAt(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
  });

  it("moves an item backward", () => {
    expect(moveItemAt(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
  });

  it("returns a copy when the index does not change", () => {
    const items = ["a", "b"];
    const next = moveItemAt(items, 1, 1);
    expect(next).toEqual(items);
    expect(next).not.toBe(items);
  });

  it("ignores out-of-range indexes", () => {
    expect(moveItemAt(["a", "b"], -1, 0)).toEqual(["a", "b"]);
    expect(moveItemAt(["a", "b"], 0, 9)).toEqual(["a", "b"]);
  });
});

describe("slotIndexFromClientY", () => {
  const slots = [
    { top: 0, bottom: 40 },
    { top: 48, bottom: 88 },
    { top: 96, bottom: 136 },
  ];

  it("returns the first slot above its midpoint", () => {
    expect(slotIndexFromClientY(slots, 10)).toBe(0);
  });

  it("crosses to the next slot past the midpoint", () => {
    expect(slotIndexFromClientY(slots, 30)).toBe(1);
  });

  it("returns the last slot below the list", () => {
    expect(slotIndexFromClientY(slots, 200)).toBe(2);
  });

  it("returns null for an empty list", () => {
    expect(slotIndexFromClientY([], 10)).toBeNull();
  });

  it("uses the floating card center so dragging down can reach later slots", () => {
    const hit = slotHitY(30, 8, 40);
    expect(hit).toBe(42);
    expect(slotIndexFromClientY(slots, hit)).toBe(1);
  });
});

describe("slotShiftY", () => {
  it("shifts the dragged item to the drop index and others out of the way", () => {
    expect(slotShiftY(0, 0, 2, 50)).toBe(100);
    expect(slotShiftY(1, 0, 2, 50)).toBe(-50);
    expect(slotShiftY(2, 0, 2, 50)).toBe(-50);
  });

  it("shifts items down when dragging upward", () => {
    expect(slotShiftY(2, 2, 0, 50)).toBe(-100);
    expect(slotShiftY(0, 2, 0, 50)).toBe(50);
    expect(slotShiftY(1, 2, 0, 50)).toBe(50);
  });

  it("does not shift when the drop index is unchanged", () => {
    expect(slotShiftY(0, 1, 1, 50)).toBe(0);
    expect(slotShiftY(1, 1, 1, 50)).toBe(0);
  });
});
