import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { BrandMark } from "../components/BrandMark";

describe("BrandMark", () => {
  it("draws one masked silhouette with two pupils", () => {
    const { container } = render(<BrandMark />);
    const svg = container.querySelector("svg")!;
    expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
    // The head is a single path cut by the eye/beak mask, not three shapes
    // parked next to each other — that is what keeps it crisp at 19 px.
    const silhouette = svg.querySelector("path[mask]")!;
    expect(silhouette).not.toBeNull();
    expect(silhouette.getAttribute("d")).toContain("C");
    expect(svg.querySelectorAll("circle")).toHaveLength(2);
    expect(svg.querySelector("mask")).not.toBeNull();
  });

  it("gives each instance its own mask id", () => {
    const { container } = render(
      <>
        <BrandMark />
        <BrandMark />
      </>,
    );
    const ids = Array.from(container.querySelectorAll("mask")).map((m) => m.id);
    expect(ids).toHaveLength(2);
    expect(ids[0]).not.toBe(ids[1]);
    // Each silhouette references its own mask.
    const refs = Array.from(container.querySelectorAll("path[mask]")).map((p) =>
      p.getAttribute("mask"),
    );
    expect(refs).toEqual(ids.map((id) => `url(#${id})`));
  });
});
