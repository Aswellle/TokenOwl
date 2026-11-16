import type { Page } from "@playwright/test";

export interface Audit {
  mainLandmarks: number;
  navLandmarks: number;
  headings: string[];
  namelessButtons: string[];
  unlabelledControls: string[];
  duplicateIds: string[];
  smallTargets: string[];
  contrastViolations: string[];
}

/**
 * Structural accessibility audit evaluated in the real page. Contrast is
 * measured only where the background resolves to an opaque computed color;
 * the acrylic backdrop is composited by the OS and cannot be sampled from
 * CSS. Small heatmap cells are excluded from the target-size list: they are
 * a dense calendar with keyboard navigation, not pointer-first controls.
 */
export async function runAudit(page: Page): Promise<Audit> {
  return page.evaluate((): Audit => {
    const visible = (element: Element) => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        style.visibility !== "hidden" &&
        style.display !== "none" &&
        Number.parseFloat(style.opacity || "1") > 0.1
      );
    };
    const nameOf = (element: Element) =>
      (element.getAttribute("aria-label") ?? "").trim() ||
      (element.textContent ?? "").trim() ||
      (element.getAttribute("title") ?? "").trim();
    const buttons = [...document.querySelectorAll("button, a[href]")].filter(visible);
    const controls = [
      ...document.querySelectorAll("input:not([type=hidden]), select, textarea"),
    ].filter(visible);
    const ids = [...document.querySelectorAll("[id]")].map((element) => element.id);
    const duplicates = ids.filter((id, index) => ids.indexOf(id) !== index);

    const parse = (value: string) => {
      const match = value.match(/rgba?\(([^)]+)\)/);
      if (!match) return null;
      const parts = match[1].split(",").map((part) => Number.parseFloat(part.trim()));
      return { r: parts[0], g: parts[1], b: parts[2], a: parts[3] ?? 1 };
    };
    const channel = (value: number) => {
      const scaled = value / 255;
      return scaled <= 0.03928 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4;
    };
    const luminance = (color: { r: number; g: number; b: number }) =>
      0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
    const ratio = (
      first: { r: number; g: number; b: number },
      second: { r: number; g: number; b: number },
    ) => {
      const a = luminance(first);
      const b = luminance(second);
      return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    };
    const opaqueBackground = (element: Element) => {
      let current: Element | null = element;
      while (current) {
        const style = getComputedStyle(current);
        if (style.backgroundImage !== "none") return null;
        const background = parse(style.backgroundColor);
        if (background && background.a >= 0.99) return background;
        if (background && background.a > 0) return null;
        current = current.parentElement;
      }
      return { r: 255, g: 255, b: 255 };
    };

    const contrastViolations: string[] = [];
    for (const element of [...document.body.querySelectorAll("*")].filter(visible)) {
      const hasText = [...element.childNodes].some(
        (node) => node.nodeType === Node.TEXT_NODE && (node.textContent ?? "").trim().length > 0,
      );
      if (!hasText) continue;
      if (element.closest("[disabled], [aria-hidden='true']")) continue;
      const style = getComputedStyle(element);
      const foreground = parse(style.color);
      if (!foreground || foreground.a < 0.99) continue;
      const background = opaqueBackground(element);
      if (!background) continue;
      const size = Number.parseFloat(style.fontSize);
      const weight = Number.parseInt(style.fontWeight, 10) || 400;
      const large = size >= 24 || (size >= 18.66 && weight >= 700);
      const value = ratio(foreground, background);
      if (value < (large ? 3 : 4.5)) {
        contrastViolations.push(
          `${value.toFixed(2)}:1 ${size}px/${weight} "${(element.textContent ?? "")
            .trim()
            .slice(0, 48)}"`,
        );
      }
    }

    return {
      mainLandmarks: document.querySelectorAll("main").length,
      navLandmarks: document.querySelectorAll("nav").length,
      headings: [...document.querySelectorAll("h1,h2,h3,h4")].map(
        (heading) => `${heading.tagName}:${(heading.textContent ?? "").trim()}`,
      ),
      namelessButtons: buttons
        .filter((element) => !nameOf(element))
        .map((element) => element.outerHTML.slice(0, 120)),
      unlabelledControls: controls
        .filter((element) => {
          const input = element as HTMLInputElement;
          return (
            input.labels?.length === 0 &&
            !element.getAttribute("aria-label") &&
            !element.getAttribute("aria-labelledby")
          );
        })
        .map((element) => element.outerHTML.slice(0, 120)),
      duplicateIds: [...new Set(duplicates)],
      smallTargets: buttons
        .filter((element) => {
          // The traffic lights draw a 12px dot but their ::before hit area
          // is 26px; getBoundingClientRect does not include pseudo-elements.
          if (element.classList.contains("light")) return false;
          // Heatmap days are a dense keyboard-navigated calendar.
          if (element.classList.contains("heat-cell")) return false;
          const rect = element.getBoundingClientRect();
          return rect.width < 24 || rect.height < 24;
        })
        .map((element) => {
          const rect = element.getBoundingClientRect();
          return `${Math.round(rect.width)}x${Math.round(rect.height)} ${nameOf(element).slice(0, 40)}`;
        }),
      contrastViolations: [...new Set(contrastViolations)],
    };
  });
}
