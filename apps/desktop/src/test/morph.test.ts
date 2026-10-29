import { describe, it, expect } from "vitest";
import { clipCircle, cornerRadius } from "../lib/morph";

/**
 * The morph's geometry helpers are pure and every branch matters: a wrong
 * radius clips the window after the reveal, a misformatted clip-path breaks
 * the interpolation between states. These tests pin the contract.
 */
describe("cornerRadius", () => {
  it("covers from the window center with the half-diagonal", () => {
    expect(cornerRadius(380, 360, 760, 720)).toBeCloseTo(Math.hypot(380, 360));
  });

  it("covers from a corner with the full diagonal", () => {
    expect(cornerRadius(0, 0, 760, 720)).toBeCloseTo(Math.hypot(760, 720));
    expect(cornerRadius(760, 720, 760, 720)).toBeCloseTo(Math.hypot(760, 720));
  });

  it("uses the farthest edge when the anchor is off-center", () => {
    // Anchor on the left edge: the far edge is 760 away horizontally.
    expect(cornerRadius(0, 360, 760, 720)).toBeCloseTo(Math.hypot(760, 360));
  });

  it("grows with off-window anchors so the reveal still clears the corners", () => {
    // A bubble parked 500px right of the dashboard's right edge.
    const offWindow = cornerRadius(1260, 360, 760, 720);
    expect(offWindow).toBeGreaterThan(cornerRadius(760, 360, 760, 720));
  });
});

describe("clipCircle", () => {
  it("formats a circle at the anchor with px units", () => {
    expect(clipCircle({ x: 120, y: 90 }, 28)).toBe("circle(28px at 120px 90px)");
  });

  it("clamps negative radii to zero so a shrinking clip never inverts", () => {
    expect(clipCircle({ x: 10, y: 10 }, -5)).toBe("circle(0px at 10px 10px)");
  });

  it("keeps fractional radii usable for interpolation", () => {
    expect(clipCircle({ x: 0, y: 0 }, 12.5)).toBe("circle(12.5px at 0px 0px)");
  });
});
