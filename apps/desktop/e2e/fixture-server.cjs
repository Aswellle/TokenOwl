// Offline fixture: an OpenAI-compatible endpoint that reports windowed quota
// (used/limit + window start/end), so the consumed-ratio rings can be verified
// natively without a real provider that exposes windowed quotas. Values are
// deterministic test data, clearly labeled; timestamps are generated relative
// to "now" so the window tags resolve through the canonical span path.
//
// Usage: node e2e/fixture-server.cjs   (listens on 127.0.0.1:8791)
// Then add a custom provider account in the app:
//   base URL http://127.0.0.1:8791/v1, quota endpoint /quota, any API key.
const http = require("node:http");
const fs = require("node:fs");
const path = require("node:path");

const PORT = 8791;
const LOG_PATH = path.join(__dirname, "..", "verify-shots", "e2e-native", "fixture-server.log");

function log(line) {
  fs.mkdirSync(path.dirname(LOG_PATH), { recursive: true });
  fs.appendFileSync(LOG_PATH, `${new Date().toISOString()} ${line}\n`);
}

function iso(time) {
  return new Date(time).toISOString().replace(/\.\d{3}Z$/, "Z");
}

const server = http.createServer((req, res) => {
  log(
    `${req.method} ${req.url} auth=${req.headers.authorization ? "present" : "absent"}` +
      ` accept=${req.headers.accept ?? "-"}`,
  );
  if (req.method === "GET" && req.url === "/v1/quota") {
    const now = Date.now();
    const end5h = now + 2 * 3_600_000;
    const end7d = now + 2 * 86_400_000;
    const body = JSON.stringify({
      data: [
        {
          id: "prompt-window-5h",
          metric_name: "Prompt tokens (5h window)",
          unit: "tokens",
          used: 420000,
          limit: 1000000,
          remaining: 580000,
          starts_at: iso(end5h - 5 * 3_600_000),
          ends_at: iso(end5h),
          reset_at: iso(end5h),
        },
        {
          id: "prompt-window-7d",
          metric_name: "Prompt tokens (7d window)",
          unit: "tokens",
          used: 8200000,
          limit: 10000000,
          remaining: 1800000,
          starts_at: iso(end7d - 7 * 86_400_000),
          ends_at: iso(end7d),
          reset_at: iso(end7d),
        },
      ],
    });
    res.writeHead(200, { "content-type": "application/json" });
    res.end(body);
    return;
  }
  // OpenRouter /key shape: usage spent (USD), remaining credits, daily usage.
  if (req.method === "GET" && req.url === "/v1/key") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        data: {
          label: "fixture key",
          usage: 0.11,
          limit: 50,
          limit_remaining: 49.89,
          usage_daily: 0.02,
          usage_weekly: 0.07,
          usage_monthly: 0.09,
          is_free_tier: false,
        },
      }),
    );
    return;
  }
  // DeepSeek balance envelope: total plus the official granted/topped-up split.
  if (req.method === "GET" && req.url === "/v1/balance") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        is_available: true,
        balance_infos: [
          {
            currency: "CNY",
            total_balance: "88.88",
            granted_balance: "0.88",
            topped_up_balance: "88.00",
          },
        ],
      }),
    );
    return;
  }
  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
});

server.listen(PORT, "127.0.0.1", () => log(`listening on 127.0.0.1:${PORT}`));
