import { expect, test, type Page } from "@playwright/test";

/**
 * Keyboard, focus, landmark and naming contracts. These assertions are the
 * machine-checkable half of docs/07's accessibility checklist; the visual and
 * assistive-technology half stays a manual pass.
 */
async function openDashboard(page: Page, query = ""): Promise<void> {
  await page.goto(`/${query}`);
  await expect(page.locator(".window")).toBeVisible();
  // First mount under fully loaded parallel workers can exceed the default
  // expect timeout; every later assertion retries on its own, so only this
  // initial landmark wait needs to be patient.
  await expect(page.getByRole("main")).toBeVisible({ timeout: 15_000 });
}

test.describe("landmarks and headings", () => {
  test("the dashboard exposes one main landmark and one live page heading", async ({ page }) => {
    await openDashboard(page);
    await expect(page.getByRole("main")).toHaveCount(1);
    await expect(page.getByRole("main")).toHaveAttribute("aria-label", "TokenOwl 面板");
    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("概览");

    await page.getByRole("button", { name: "历史", exact: true }).click();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("历史");
  });

  test("the dock and provider rail are named navigation landmarks", async ({ page }) => {
    await openDashboard(page);
    await expect(page.getByRole("navigation", { name: "功能导航" })).toBeVisible();
    await expect(page.getByRole("navigation", { name: "已连接供应商" })).toBeVisible();
  });
});

test.describe("names for controls", () => {
  test("every button on each view has an accessible name", async ({ page }) => {
    await openDashboard(page);
    for (const view of ["概览", "历史", "通知", "设置"]) {
      await page.getByRole("button", { name: view, exact: true }).click();
      const unnamed = await page.evaluate(() =>
        Array.from(document.querySelectorAll("button"))
          .filter((button) => {
            const name =
              button.getAttribute("aria-label") ??
              button.textContent ??
              button.getAttribute("title") ??
              "";
            return name.trim().length === 0;
          })
          .map((button) => button.className),
      );
      expect(unnamed, `unnamed buttons in ${view}`).toEqual([]);
    }
  });

  test("every form control on the settings view is labelled", async ({ page }) => {
    await openDashboard(page);
    await page.getByRole("button", { name: "设置", exact: true }).click();
    await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
    const unlabelled = await page.evaluate(() => {
      const controls = Array.from(
        document.querySelectorAll<HTMLElement>("input, select, textarea"),
      );
      return controls
        .filter((control) => {
          const id = control.getAttribute("id");
          const hasFor = id ? document.querySelector(`label[for="${id}"]`) !== null : false;
          const wrapped = control.closest("label") !== null;
          const aria = control.getAttribute("aria-label") ?? control.getAttribute("aria-labelledby");
          return !hasFor && !wrapped && !aria;
        })
        .map((control) => control.outerHTML.slice(0, 120));
    });
    expect(unlabelled).toEqual([]);
  });
});

test.describe("keyboard", () => {
  test("dock buttons activate with Enter and Space and expose their pressed state", async ({
    page,
  }) => {
    await openDashboard(page);
    const history = page.getByRole("button", { name: "历史", exact: true });
    await history.focus();
    await page.keyboard.press("Enter");
    await expect(history).toHaveAttribute("aria-pressed", "true");

    const settings = page.getByRole("button", { name: "设置", exact: true });
    await settings.focus();
    await page.keyboard.press(" ");
    await expect(settings).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByRole("heading", { name: "连接 AI 供应商账户" })).toBeVisible();
  });

  test("focused controls draw a visible focus ring", async ({ page }) => {
    await openDashboard(page);
    const dock = page.getByRole("button", { name: "历史", exact: true });
    await dock.focus();
    const outline = await dock.evaluate((element) => {
      const style = getComputedStyle(element);
      return { style: style.outlineStyle, width: style.outlineWidth };
    });
    expect(outline.style).not.toBe("none");
    expect(Number.parseFloat(outline.width)).toBeGreaterThanOrEqual(1);
  });

  test("the LAN dialog traps Tab, starts on cancel, and returns focus on Escape", async ({
    page,
  }) => {
    await openDashboard(page);
    await page.getByRole("button", { name: "设置", exact: true }).click();
    await page.getByLabel("账户名称").fill("本机网关");
    await page.getByLabel("接口地址").fill("http://localhost:4000/v1");
    await page.getByLabel("API 密钥").fill("sk-test");
    await page.getByRole("button", { name: "连接账户" }).click();

    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    const cancel = dialog.getByRole("button", { name: "取消" });
    const confirm = dialog.getByRole("button", { name: "我确认，继续" });
    await expect(cancel).toBeFocused();

    await page.keyboard.press("Tab");
    await expect(confirm).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(cancel).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(confirm).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(page.getByRole("button", { name: "连接账户" })).toBeFocused();
  });
});

test.describe("reduced motion", () => {
  test("the bubble reports and applies the reduced-motion contract", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/?window=bubble");
    const svg = page.locator("svg.quota-rings");
    await expect(svg).toHaveAttribute("data-reduced-motion", "true");
    // The dial is already at its final offset on the first painted frame.
    const value = page.locator(".quota-rings__value--outer");
    await expect(value).toHaveCount(1);
    const transition = await value.evaluate((el) => getComputedStyle(el).transitionProperty);
    expect(transition).not.toContain("stroke-dashoffset");
  });
});
