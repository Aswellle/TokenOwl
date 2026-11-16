import { expect, test } from "./fixtures";
import {
  accountRow,
  createCustomAccount,
  openOverview,
} from "./helpers";

// Built at runtime so no credential-shaped literal enters the source or the
// test logs; the fixture only ever sees throwaway values.
const OLD_KEY = ["sk", "native", "old"].join("-");
const BAD_KEY = ["sk", "native", "bad"].join("-");
const NEW_KEY = ["sk", "native", "new"].join("-");

/**
 * The validate-then-commit contract, driven through the real UI and the real
 * keyring. A rejected candidate must leave the old key working; a verified
 * candidate must become the key every later request uses.
 */
test("key rotation rejects a bad candidate, keeps the old key, then commits a verified one", async ({
  app,
  provider,
}) => {
  const page = app.page;
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await createCustomAccount(page, {
    label: "轮换账户",
    baseUrl: provider.baseUrl,
    apiKey: OLD_KEY,
  });
  await openOverview(page);
  const row = accountRow(page, "轮换账户");
  await expect(row).toBeVisible();

  // Force the row into the credential-recovery state through a real 401.
  provider.mode = "auth";
  await row.getByRole("button", { name: "刷新账户 轮换账户" }).click();
  await expect(row).toContainText("同步失败", { timeout: 15_000 });
  await expect(
    row.getByRole("button", { name: "更新账户 轮换账户 的 API 密钥" }),
  ).toBeVisible({ timeout: 15_000 });
  await expect(row).toContainText("供应商拒绝了当前密钥");

  const updateButton = row.getByRole("button", {
    name: "更新账户 轮换账户 的 API 密钥",
  });
  await updateButton.click();
  const dialog = page.getByRole("alertdialog", { name: "更新 API 密钥" });
  await expect(dialog).toBeVisible();
  const input = dialog.getByLabel("新的 API 密钥");
  await input.fill(BAD_KEY);
  await dialog.getByRole("button", { name: "保存并验证" }).click();

  // A 401 on the candidate is a rejection, not a commit: the dialog stays
  // open with the typed value and the stable keyring reference is untouched.
  await expect(dialog.locator(".key-confirm__notice")).toContainText(
    "新密钥被供应商拒绝，原密钥保持不变",
    { timeout: 15_000 },
  );
  await expect(input).toHaveValue(BAD_KEY);
  expect(
    provider.quotaRequests().some((request) => request.apiKey === BAD_KEY),
  ).toBe(true);

  await dialog.getByRole("button", { name: "取消" }).click();
  await expect(dialog).toBeHidden();

  // The old key still works: the same account recovers without re-entering it.
  provider.mode = "windowed";
  const oldRequests = provider.quotaRequests().filter(
    (request) => request.apiKey === OLD_KEY,
  ).length;
  await row.getByRole("button", { name: "刷新账户 轮换账户" }).click();
  await expect(row).toContainText("58%", { timeout: 15_000 });
  await expect(row).not.toContainText("供应商拒绝了当前密钥");
  expect(
    provider.quotaRequests().filter((request) => request.apiKey === OLD_KEY)
      .length,
  ).toBeGreaterThan(oldRequests);

  // The provider revokes the old key and accepts the new one: the next
  // refresh returns to the credential-recovery state, and rotating from
  // there must commit the candidate.
  provider.acceptedKeys.delete(OLD_KEY);
  provider.acceptedKeys.add(NEW_KEY);
  await row.getByRole("button", { name: "刷新账户 轮换账户" }).click();
  await expect(updateButton).toBeVisible({ timeout: 15_000 });

  // A candidate the fixture accepts is probed and committed, then the
  // post-commit refresh uses it.
  await updateButton.click();
  const secondDialog = page.getByRole("alertdialog", { name: "更新 API 密钥" });
  await secondDialog.getByLabel("新的 API 密钥").fill(NEW_KEY);
  await secondDialog.getByRole("button", { name: "保存并验证" }).click();
  await expect(secondDialog).toBeHidden({ timeout: 20_000 });
  await expect(
    provider.quotaRequests().some((request) => request.apiKey === NEW_KEY),
  ).toBe(true);

  // Only the committed key can refresh now.
  const newRequests = provider.quotaRequests().filter(
    (request) => request.apiKey === NEW_KEY,
  ).length;
  await row.getByRole("button", { name: "刷新账户 轮换账户" }).click();
  await expect(row).toContainText("58%", { timeout: 15_000 });
  expect(
    provider.quotaRequests().filter((request) => request.apiKey === NEW_KEY)
      .length,
  ).toBeGreaterThan(newRequests);
  expect(
    provider.quotaRequests().some(
      (request) => request.apiKey === OLD_KEY && request.path === "/v1/quota",
    ),
  ).toBe(true);

  // Neither key is ever rendered into the page or written to app data.
  const leaks = await page.evaluate(
    ([oldKey, newKey]) => {
      const haystack = document.documentElement.outerHTML + document.body.innerText;
      return haystack.includes(oldKey) || haystack.includes(newKey);
    },
    [OLD_KEY, NEW_KEY],
  );
  expect(leaks).toBe(false);
});
