import { expect, test } from "@playwright/test";

/**
 * Bubble form and morph scenarios over the mock bridge (plain-browser dev
 * preview). The preview shares every view-level decision with the native
 * shell — which rings render, how the clip reveal is computed, what a click
 * does — while the OS-level behaviors (taskbar, window drag, edge snap) live
 * in the real-device pass; see SCENARIOS.md for the boundary matrix.
 */
test.describe("bubble form", () => {
  test("renders the bubble dial with two separated rings for windowed quotas", async ({
    page,
  }) => {
    await page.goto("/?window=bubble");
    const bubble = page.locator(".bubble");
    await expect(bubble).toBeVisible();
    // The accessible name carries the whole contract: brand, health, per-
    // window readings, and the action.
    await expect(bubble).toHaveAttribute("aria-label", /TokenOwl.*展开 TokenOwl 面板/s);

    // Two bands on the layered radii, each with its own unlit track, and no
    // shared channel circle underneath.
    const values = page.locator(".quota-rings__value");
    await expect(values).toHaveCount(2);
    await expect(values.nth(0)).toHaveAttribute("r", "21");
    await expect(values.nth(1)).toHaveAttribute("r", "14");
    await expect(page.locator(".quota-rings__track")).toHaveCount(2);
    await expect(page.locator(".quota-rings__channel")).toHaveCount(0);
    // Rings draw the consumed share: the demo 5h window used 128 of 800.
    await expect(page.locator('[data-ring-tone="outer"]')).toHaveAttribute(
      "data-ring-consumed",
      "0.160",
    );
  });

  test("renders the solo-owl empty state without any dial when no provider exists", async ({
    page,
  }) => {
    await page.goto("/?window=bubble&empty=1");
    await expect(page.locator(".bubble")).toBeVisible();
    await expect(page.locator(".bubble")).toHaveClass(/bubble--unconfigured/);
    await expect(page.locator(".bubble")).toHaveAttribute(
      "aria-label",
      /尚未连接.*等待添加第一个供应商/,
    );
    await expect(page.locator(".bubble__mark--solo")).toBeVisible();
    await expect(page.locator(".quota-rings__track")).toHaveCount(0);
    await expect(page.locator(".quota-rings__value")).toHaveCount(0);
  });

  test("the empty state's centre mark is the owl glyph, never a provider mark", async ({
    page,
  }) => {
    await page.goto("/?window=bubble&empty=1");
    const svg = page.locator("svg.quota-rings");
    await expect(svg).toHaveAttribute("data-ring-count", "0");
    await expect(svg).toHaveAttribute("data-morph-phase", "idle");
    await expect(page.locator(".bubble__mark--solo svg")).toBeVisible();
    await expect(page.locator(".bubble__mark .provider-icon")).toHaveCount(0);
  });

  test("two windowed quotas carry the provider mark, exact readings, and the two-ring icon size", async ({
    page,
  }) => {
    await page.goto("/?window=bubble");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /额度已用：5h 16%，7d 10%/);
    await expect(bubble.locator("svg.quota-rings")).toHaveAttribute("data-ring-count", "2");
    await expect(bubble.locator("svg.quota-rings")).toHaveAttribute("data-morph-phase", "idle");
    // The centre mark swaps the owl for the primary account's provider icon,
    // sized for the two-ring layout's 26px hole.
    await expect(page.locator(".bubble__mark .provider-icon")).toBeVisible();
    await expect(page.locator(".bubble__mark .provider-icon")).toHaveCSS("width", "17px");
    await expect(page.locator(".bubble__mark--solo")).toHaveCount(0);
    // The inner band carries the weekly window's own consumed share.
    await expect(page.locator('[data-ring-tone="inner"]')).toHaveAttribute(
      "data-ring-consumed",
      "0.103",
    );
  });

  test("the rings answer for the primary account, and a fallback dial replaces them when the primary has no trusted window", async ({
    page,
  }) => {
    // scenario=error marks the first account errored, so the relay account
    // becomes primary; its only window lacks a usable span, which must yield
    // the honest worst-share fallback dial — not another account's rings.
    await page.goto("/?window=bubble&scenario=error");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /额度已用：\d+%/);
    await expect(bubble.locator("svg.quota-rings")).toHaveAttribute("data-ring-count", "1");
    await expect(page.locator(".quota-rings__value")).toHaveCount(1);
    await expect(page.locator(".quota-rings__value").first()).toHaveAttribute("r", "21");
    // The centre mark still belongs to the (non-errored) primary account.
    await expect(page.locator(".bubble__mark .provider-icon")).toBeVisible();
  });

  test("a partial window keeps its ring while errored and windowless snapshots stay out", async ({
    page,
  }) => {
    await page.goto("/?window=bubble&scenario=partial");
    const bubble = page.locator(".bubble");
    // partial status is still provider-reported truth; the errored window
    // and the windowless balance contribute nothing.
    await expect(bubble).toHaveAttribute("aria-label", /5h 50%/);
    await expect(page.locator(".quota-rings__value")).toHaveCount(1);
    await expect(page.locator(".quota-rings__value").first()).toHaveAttribute("r", "21");
    await expect(page.locator(".quota-rings__track")).toHaveCount(1);
  });

  test("clicking the bubble returns to the dashboard and the morph lands", async ({ page }) => {
    await page.goto("/?window=bubble");
    await page.locator(".bubble").click();
    await expect(page).toHaveURL(/\/$/);
    // The preview plays the same clip reveal as the native shell: the panel
    // is fully present once the morph settles.
    await expect(page.locator(".window")).toBeVisible();
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
  });

  test("the dashboard's Bubble pill collapses back into the bubble", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Bubble", exact: true }).click();
    await expect(page).toHaveURL(/window=bubble/);
    await expect(page.locator(".bubble")).toBeVisible();
  });

  test("reduced motion lands ring progress without a tween", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/?window=bubble");
    const value = page.locator(".quota-rings__value--outer");
    await expect(value).toHaveCount(1);
    // Demo 5h window: 128 used of 800 → 0.16 consumed → dashoffset is 84% of
    // the circumference, written immediately with no transition running.
    const offset = await value.evaluate((el) => Number(getComputedStyle(el).strokeDashoffset.replace("px", "")));
    const circumference = 2 * Math.PI * 21;
    expect(offset).toBeCloseTo(circumference * (1 - 0.16), 0);
    const transition = await value.evaluate((el) => getComputedStyle(el).transitionProperty);
    expect(transition).not.toContain("stroke-dashoffset");
  });

  test("survives ten expand/collapse round trips without wedging", async ({ page }) => {
    // Ten animated round trips: the default 30 s budget is tight once the
    // machine is busy, and a slow run is not a wedged shell.
    test.slow();
    await page.goto("/");
    for (let round = 0; round < 10; round += 1) {
      await page.getByRole("button", { name: "Bubble", exact: true }).click();
      await expect(page.locator(".bubble")).toBeVisible();
      await page.locator(".bubble").click();
      await expect(page.locator(".window")).toBeVisible();
    }
    // The dashboard is still interactive after the loop.
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
  });

  test("the expand reveal sweeps a growing circle across the panel", async ({ page }) => {
    await page.goto("/?window=bubble");
    await expect(page.locator(".bubble")).toBeVisible();
    // Sample the panel wrapper's clip in-page at ~16 ms so the assertion sees
    // the actual animation frames instead of racing Playwright round-trips.
    await page.evaluate(() => {
      const samples: string[] = [];
      (window as unknown as { __clipSamples: string[] }).__clipSamples = samples;
      const timer = window.setInterval(() => {
        const panel = document.querySelector(".window")?.parentElement;
        if (panel) samples.push(window.getComputedStyle(panel).clipPath);
      }, 16);
      window.setTimeout(() => window.clearInterval(timer), 2500);
    });
    await page.locator(".bubble").click();
    await expect(page.locator(".window")).toBeVisible();
    await page.waitForTimeout(1500);

    const { radii, finalCoversViewport } = await page.evaluate(() => {
      const clips = (window as unknown as { __clipSamples: string[] }).__clipSamples;
      const circles = clips.filter((clip) => clip.startsWith("circle("));
      const radii = circles.map((clip) => Number(/circle\(([\d.]+)px/.exec(clip)?.[1] ?? 0));
      const panel = document.querySelector(".window")?.parentElement;
      const finalRadius = Number(
        /circle\(([\d.]+)px/.exec(panel ? window.getComputedStyle(panel).clipPath : "")?.[1] ??
          0,
      );
      const corner = Math.hypot(window.innerWidth / 2, window.innerHeight / 2);
      return { radii, finalCoversViewport: finalRadius >= corner };
    });
    // A real path animation: many distinct circle frames sweeping outward.
    const distinct = new Set(radii).size;
    expect(distinct).toBeGreaterThanOrEqual(4);
    expect(radii.at(-1)! - radii[0]).toBeGreaterThan(200);
    expect(finalCoversViewport).toBe(true);
  });

  test("the collapse clips the panel back into the bubble's circle", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".window")).toBeVisible();
    await page.evaluate(() => {
      const samples: string[] = [];
      (window as unknown as { __clipSamples: string[] }).__clipSamples = samples;
      const timer = window.setInterval(() => {
        const panel = document.querySelector(".window")?.parentElement;
        if (panel) samples.push(window.getComputedStyle(panel).clipPath);
      }, 16);
      window.setTimeout(() => window.clearInterval(timer), 2500);
    });
    await page.getByRole("button", { name: "Bubble", exact: true }).click();
    await expect(page.locator(".bubble")).toBeVisible();
    await page.waitForTimeout(1500);

    const radii = await page.evaluate(() => {
      const clips = (window as unknown as { __clipSamples: string[] }).__clipSamples;
      return clips
        .filter((clip) => clip.startsWith("circle("))
        .map((clip) => Number(/circle\(([\d.]+)px/.exec(clip)?.[1] ?? 0));
    });
    const distinct = new Set(radii).size;
    expect(distinct).toBeGreaterThanOrEqual(3);
    // The circle sweeps inward toward the bubble's own diameter.
    expect(radii[0] - radii.at(-1)!).toBeGreaterThan(200);
    expect(radii.at(-1)!).toBeLessThanOrEqual(48);
  });

  test("reduced motion replaces the clip sweep with an instant fade", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/?window=bubble");
    await page.locator(".bubble").click();
    await expect(page.locator(".window")).toBeVisible();
    const clip = await page.evaluate(
      () =>
        window.getComputedStyle(
          document.querySelector(".window")!.parentElement as Element,
        ).clipPath,
    );
    // No circular clip under reduced motion: the views cross-fade instead.
    expect(clip).toBe("none");
  });

  test("the bubble window shows no dashboard chrome", async ({ page }) => {
    await page.goto("/?window=bubble");
    // A 56×56 floating window has no dock, no titlebar, no scroll region.
    await expect(page.locator(".dock")).toHaveCount(0);
    await expect(page.locator(".titlebar")).toHaveCount(0);
  });

  test("three real windows produce three rings with tightened geometry", async ({ page }) => {
    await page.goto("/?window=bubble&scenario=three-windows");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /额度已用：5h 40%，7d 70%，30d 10%/);
    await expect(bubble.locator("svg.quota-rings")).toHaveAttribute("data-ring-count", "3");

    const values = page.locator(".quota-rings__value");
    await expect(values).toHaveCount(3);
    await expect(values.nth(0)).toHaveAttribute("r", "21");
    await expect(values.nth(1)).toHaveAttribute("r", "16");
    await expect(values.nth(2)).toHaveAttribute("r", "11");
    for (const index of [0, 1, 2]) {
      await expect(values.nth(index)).toHaveAttribute("stroke-width", "4");
    }
    await expect(page.locator(".quota-rings__track")).toHaveCount(3);
    // The centre mark steps down to keep a clear margin to the inner band.
    await expect(page.locator(".bubble__mark .provider-icon")).toHaveCSS("width", "15px");
  });

  test("unknown windows keep track-only rings and never fake a zero arc", async ({ page }) => {
    await page.goto("/?window=bubble&scenario=unknown");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /5h 未知，7d 未知/);
    await expect(page.locator("[data-ring-consumed='unknown']")).toHaveCount(2);
    await expect(page.locator(".quota-rings__value")).toHaveCount(0);
    await expect(page.locator(".quota-rings__track")).toHaveCount(2);
  });

  test("a fully consumed window closes the ring with a butt cap", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/?window=bubble&scenario=full");
    await expect(page.locator("[data-ring-consumed='1.000']")).toHaveCount(2);
    const value = page.locator(".quota-rings__value").first();
    await expect(value).toHaveAttribute("stroke-linecap", "butt");
    const offset = await value.evaluate((el) =>
      Number(getComputedStyle(el).strokeDashoffset.replace("px", "")),
    );
    expect(offset).toBeCloseTo(0, 0);
  });

  test("stale data keeps the rings and labels the bubble stale", async ({ page }) => {
    await page.goto("/?window=bubble&scenario=stale");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveClass(/bubble--stale/);
    await expect(bubble).toHaveAttribute("aria-label", /数据偏旧/);
    await expect(page.locator(".quota-rings__value")).toHaveCount(2);
  });

  test("offline falls back to the solo owl with an offline label", async ({ page }) => {
    await page.goto("/?window=bubble&scenario=offline");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveClass(/bubble--offline/);
    await expect(bubble).toHaveAttribute("aria-label", /本地服务不可用/);
    await expect(page.locator(".bubble__mark--solo")).toBeVisible();
    await expect(page.locator(".quota-rings__track")).toHaveCount(0);
  });

  test("an account without windowed quotas explains the fallback instead of inventing one", async ({
    page,
  }) => {
    await page.goto("/?window=bubble&scenario=no-snapshots");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /该账户未上报按窗口计的额度/);
    await expect(page.locator(".quota-rings__track")).toHaveCount(0);
    await expect(page.locator(".quota-rings__value")).toHaveCount(0);
    // A real account still owns the centre mark; only the dials are absent.
    await expect(page.locator(".bubble__mark--solo")).toHaveCount(0);
  });

  test("the bubble is keyboard activatable", async ({ page }) => {
    await page.goto("/?window=bubble");
    await page.locator(".bubble").focus();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/$/);
    await expect(page.locator(".window")).toBeVisible();
  });

  test("a drag past the threshold suppresses the click, a small move does not", async ({
    page,
  }) => {
    await page.goto("/?window=bubble");
    const bubble = page.locator(".bubble");
    await expect(bubble).toBeVisible();
    const box = await bubble.boundingBox();
    expect(box).not.toBeNull();
    const cx = box!.x + box!.width / 2;
    const cy = box!.y + box!.height / 2;

    await page.mouse.move(cx, cy);
    await page.mouse.down();
    await page.mouse.move(cx + 12, cy);
    await page.mouse.up();
    await page.waitForTimeout(400);
    await expect(page).toHaveURL(/window=bubble/);

    await page.mouse.move(cx, cy);
    await page.mouse.down();
    await page.mouse.move(cx + 2, cy);
    await page.mouse.up();
    await expect(page).toHaveURL(/\/$/);
  });

  test("a zero-use window stays a bare track: no arc and no stray cap dot", async ({ page }) => {
    await page.goto("/?window=bubble&scenario=zero");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /5h 0%，7d 40%/);
    await expect(bubble.locator("svg.quota-rings")).toHaveAttribute("data-ring-count", "2");

    // The zero window keeps its honest 0.000 reading but draws nothing: the
    // guard must not render a round cap dot at 12 o'clock.
    const zeroGroup = page.locator('[data-ring-id="scenario-zero-5h"]');
    await expect(zeroGroup).toHaveAttribute("data-ring-consumed", "0.000");
    await expect(zeroGroup.locator(".quota-rings__value")).toHaveCount(0);
    await expect(zeroGroup.locator(".quota-rings__track")).toHaveCount(1);

    // The sibling window still draws its real arc on the inner band.
    const usedGroup = page.locator('[data-ring-id="scenario-zero-7d"]');
    await expect(usedGroup).toHaveAttribute("data-ring-consumed", "0.400");
    await expect(usedGroup.locator(".quota-rings__value")).toHaveCount(1);
    await expect(page.locator(".quota-rings__value")).toHaveCount(1);
  });

  test("an overdrawn window clamps to a full butt-capped ring", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/?window=bubble&scenario=overdrawn");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /5h 100%/);
    const value = page.locator(".quota-rings__value");
    await expect(value).toHaveCount(1);
    await expect(page.locator('[data-ring-consumed="1.000"]')).toHaveCount(1);
    // The consumed share must never exceed one turn: no overshoot past the
    // full dial, and the seam stays a clean butt joint.
    await expect(value).toHaveAttribute("stroke-linecap", "butt");
    const offset = await value.evaluate((el) =>
      Number(getComputedStyle(el).strokeDashoffset.replace("px", "")),
    );
    expect(offset).toBeCloseTo(0, 0);
  });

  test("a known window and an unreadable one render an arc beside a bare track", async ({
    page,
  }) => {
    await page.goto("/?window=bubble&scenario=mixed-known");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /5h 40%，7d 未知/);
    // One real arc plus two tracks: the unknown ring stays visible as a dim
    // band instead of vanishing or faking a value.
    await expect(page.locator(".quota-rings__value")).toHaveCount(1);
    await expect(page.locator(".quota-rings__track")).toHaveCount(2);
    await expect(page.locator('[data-ring-id="scenario-mixed-5h"]')).toHaveAttribute(
      "data-ring-consumed",
      "0.400",
    );
    await expect(page.locator('[data-ring-id="scenario-mixed-7d"]')).toHaveAttribute(
      "data-ring-consumed",
      "unknown",
    );
  });

  test("two snapshots naming the same window collapse into one ring", async ({ page }) => {
    await page.goto("/?window=bubble&scenario=duplicate");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /5h 50%/);
    // The duplicated tag must not draw two coincident bands: one ring, first
    // snapshot's reading wins, nothing invented for the second.
    await expect(bubble.locator("svg.quota-rings")).toHaveAttribute("data-ring-count", "1");
    await expect(page.locator(".quota-rings__value")).toHaveCount(1);
    await expect(page.locator(".quota-rings__track")).toHaveCount(1);
    await expect(page.locator('[data-ring-id="scenario-dup-a"]')).toHaveCount(1);
    await expect(page.locator('[data-ring-id="scenario-dup-b"]')).toHaveCount(0);
    await expect(page.locator('[data-ring-consumed="0.500"]')).toHaveCount(1);
  });

  test("five real windows still yield exactly three semantically distinct rings", async ({
    page,
  }) => {
    await page.goto("/?window=bubble&scenario=five-windows");
    const bubble = page.locator(".bubble");
    await expect(bubble).toHaveAttribute("aria-label", /5h 20%，90d 50%，180d 80%/);
    await expect(bubble.locator("svg.quota-rings")).toHaveAttribute("data-ring-count", "3");
    // Shortest outside, the window furthest from both ends in the middle,
    // longest inside — and the tightened three-ring geometry.
    const tones = page.locator(".quota-rings__value");
    await expect(tones).toHaveCount(3);
    await expect(page.locator('[data-ring-tone="outer"]')).toHaveAttribute(
      "data-ring-id",
      "scenario-w-5h",
    );
    await expect(page.locator('[data-ring-tone="outer"]')).toHaveAttribute(
      "data-ring-consumed",
      "0.200",
    );
    await expect(page.locator('[data-ring-tone="middle"]')).toHaveAttribute(
      "data-ring-id",
      "scenario-w-90d",
    );
    await expect(page.locator('[data-ring-tone="inner"]')).toHaveAttribute(
      "data-ring-id",
      "scenario-w-180d",
    );
    await expect(tones.nth(0)).toHaveAttribute("r", "21");
    await expect(tones.nth(1)).toHaveAttribute("r", "16");
    await expect(tones.nth(2)).toHaveAttribute("r", "11");
  });

  test("band edges keep a visible gap between rings at every ring count", async ({ page }) => {
    // Two rings: 21/14 with 5-unit bands leaves a 2-unit gap; three rings:
    // 21/16/11 with 4-unit bands leaves a 1-unit gap. The gap is the "clear
    // boundary" of the design — assert it straight from the DOM geometry so a
    // future radius change cannot silently fuse the bands.
    const readGeometry = async () => {
      await page.locator("svg.quota-rings").waitFor({ state: "attached" });
      return page.evaluate(() => {
        const circles = Array.from(
          document.querySelectorAll<SVGCircleElement>(".quota-rings__track"),
        );
        return circles
          .map((circle) => ({
            radius: Number(circle.getAttribute("r")),
            width: Number(circle.getAttribute("stroke-width")),
          }))
          .sort((a, b) => b.radius - a.radius);
      });
    };

    await page.goto("/?window=bubble");
    const two = await readGeometry();
    expect(two).toHaveLength(2);
    expect(two[0].radius - two[0].width / 2 - (two[1].radius + two[1].width / 2)).toBeGreaterThanOrEqual(2);

    await page.goto("/?window=bubble&scenario=three-windows");
    const three = await readGeometry();
    expect(three).toHaveLength(3);
    expect(three[0].radius - three[0].width / 2 - (three[1].radius + three[1].width / 2)).toBeGreaterThanOrEqual(1);
    expect(three[1].radius - three[1].width / 2 - (three[2].radius + three[2].width / 2)).toBeGreaterThanOrEqual(1);
  });

  test("a full expand/collapse round trip leaves the rings exactly as they were", async ({
    page,
  }) => {
    await page.goto("/?window=bubble");
    const readRings = () =>
      page.evaluate(() => ({
        count: document.querySelector("svg.quota-rings")?.getAttribute("data-ring-count"),
        values: Array.from(document.querySelectorAll(".quota-rings__value")).map((el) => ({
          id: el.parentElement?.getAttribute("data-ring-id"),
          consumed: el.parentElement?.getAttribute("data-ring-consumed"),
          r: el.getAttribute("r"),
        })),
        tracks: document.querySelectorAll(".quota-rings__track").length,
      }));

    // The bubble re-fetches its accounts after remount; poll for the dials
    // instead of guessing at the bridge latency — both before and after.
    await expect(page.locator(".quota-rings__value")).toHaveCount(2);
    const before = await readRings();
    expect(before.count).toBe("2");

    await page.locator(".bubble").click();
    await expect(page.locator(".window")).toBeVisible();
    await page.getByRole("button", { name: "Bubble", exact: true }).click();
    await expect(page.locator(".bubble")).toBeVisible();
    await expect(page.locator(".quota-rings__value")).toHaveCount(2);

    const after = await readRings();
    // Same two dials, same readings, same radii — the morph must not leave a
    // duplicated arc, a stale third circle, or a reset behind.
    expect(after).toEqual(before);
  });

  test("rapid expand/collapse toggling mid-spring never wedges the shell", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".window")).toBeVisible();

    // Interrupt the clip morph repeatedly while its spring is still moving.
    for (let round = 0; round < 3; round += 1) {
      await page.getByRole("button", { name: "Bubble", exact: true }).click();
      await page.waitForTimeout(120);
      await page.locator(".bubble").click();
      await page.waitForTimeout(120);
    }
    // Land on the dashboard and let every spring settle.
    await page.getByRole("button", { name: "Bubble", exact: true }).click();
    await expect(page.locator(".bubble")).toBeVisible();
    await page.locator(".bubble").click();
    await expect(page.locator(".window")).toBeVisible();
    await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
    await page.waitForTimeout(1_200);

    // No stale clip: the settled dashboard is fully revealed — a circle that
    // reaches every corner — and interactive.
    const settled = await page.evaluate(() => {
      const clip = window.getComputedStyle(
        document.querySelector(".window")!.parentElement as Element,
      ).clipPath;
      const radius = Number(/circle\(([\d.]+)px/.exec(clip)?.[1] ?? 0);
      const corner = Math.hypot(window.innerWidth / 2, window.innerHeight / 2);
      return { radius, corner, covers: radius >= corner };
    });
    expect(settled.covers).toBe(true);
    // The rings survived the interrupts unchanged.
    await page.getByRole("button", { name: "Bubble", exact: true }).click();
    await expect(page.locator(".bubble")).toBeVisible();
    await expect(page.locator('[data-ring-consumed="0.160"]')).toHaveCount(1);
    await expect(page.locator(".quota-rings__value")).toHaveCount(2);
  });
});
