import { expect, test } from "./fixtures";
import { accountRow, createCustomAccount, openOverview } from "./helpers";

const KEY = ["sk", "native", "old"].join("-");

/**
 * Degraded-path behaviour with a real HTTP server: a 5xx, a 429 with
 * Retry-After, and a non-JSON 200 must all keep the last good snapshot,
 * name the failure class, and recover on the next successful request. The
 * endpoint panel must classify a 401 as auth, not as a generic fault.
 */
test("refresh failures keep the last good snapshot, explain the class, and recover", async ({
  app,
  provider,
}) => {
  const page = app.page;
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await createCustomAccount(page, {
    label: "失败恢复账户",
    baseUrl: provider.baseUrl,
    apiKey: KEY,
    confirmLan: true,
  });
  await openOverview(page);
  const row = accountRow(page, "失败恢复账户");
  await expect(row).toContainText("58%");
  const refresh = row.getByRole("button", { name: "刷新账户 失败恢复账户" });

  provider.mode = "server-error";
  await refresh.click();
  await expect(row).toContainText("同步失败", { timeout: 15_000 });
  await expect(row).toContainText("供应商服务端错误");
  await expect(row).toContainText("58%", { timeout: 5_000 });

  provider.mode = "rate-limit";
  await refresh.click();
  await expect(row).toContainText("供应商正在限流", { timeout: 15_000 });
  await expect(row).toContainText("58%");

  provider.mode = "malformed";
  await refresh.click();
  await expect(row).toContainText("供应商响应格式异常", { timeout: 15_000 });
  await expect(row).toContainText("58%");

  // A 401 during endpoint testing is reported per endpoint and does not hide
  // the last snapshot.
  provider.mode = "auth";
  await refresh.click();
  await expect(row).toContainText("供应商拒绝了当前密钥", { timeout: 15_000 });
  await row.locator(".account__identity").click();
  await page.getByRole("button", { name: "测试端点" }).click();
  const probe = page.locator(".endpoint-test__row");
  await expect(probe).toHaveCount(1, { timeout: 15_000 });
  await expect(probe).toContainText("认证失败");
  await expect(probe).toContainText("HTTP 401");
  await page.getByRole("button", { name: "返回概览" }).click();

  provider.mode = "windowed";
  await row.getByRole("button", { name: "刷新账户 失败恢复账户" }).click();
  await expect(row).not.toContainText("同步失败", { timeout: 15_000 });
  await expect(row).toContainText("58%");
});
