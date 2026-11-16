import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";

/**
 * Loopback provider used by the native suite. It speaks the same
 * OpenAI-compatible quota shape the shipped lenient parser accepts, and its
 * state is mutated directly by the test so a real Rust request path can be
 * driven through success, auth, outage, rate-limit and malformed branches.
 *
 * Only throwaway keys live here. Request logging keeps the header value so a
 * test can prove which key the real credential store resolved, but nothing is
 * written to disk and the suite never receives a real provider secret.
 */
export type QuotaMode =
  | "windowed"
  | "low"
  | "balance"
  | "auth"
  | "server-error"
  | "rate-limit"
  | "malformed"
  | "empty";

export interface RecordedRequest {
  method: string;
  path: string;
  apiKey: string | null;
}

function iso(time: number): string {
  return new Date(time).toISOString().replace(/\.\d{3}Z$/, "Z");
}

function windowedBody(mode: QuotaMode): unknown {
  const now = Date.now();
  const fiveHourEnd = now + 2 * 3_600_000;
  const weekEnd = now + 2 * 86_400_000;
  const first = mode === "low"
    ? { used: 950_000, limit: 1_000_000, remaining: 50_000 }
    : { used: 420_000, limit: 1_000_000, remaining: 580_000 };
  return {
    data: [
      {
        id: "prompt-window-5h",
        metric_name: "Prompt tokens (5h window)",
        unit: "tokens",
        ...first,
        starts_at: iso(fiveHourEnd - 5 * 3_600_000),
        ends_at: iso(fiveHourEnd),
        reset_at: iso(fiveHourEnd),
      },
      {
        id: "prompt-window-7d",
        metric_name: "Prompt tokens (7d window)",
        unit: "tokens",
        used: mode === "low" ? 9_800_000 : 8_200_000,
        limit: 10_000_000,
        remaining: mode === "low" ? 200_000 : 1_800_000,
        starts_at: iso(weekEnd - 7 * 86_400_000),
        ends_at: iso(weekEnd),
        reset_at: iso(weekEnd),
      },
    ],
  };
}

export class FixtureProvider {
  private server: Server | null = null;
  readonly requests: RecordedRequest[] = [];
  readonly acceptedKeys = new Set<string>();
  mode: QuotaMode = "windowed";

  get port(): number {
    if (!this.server) throw new Error("fixture provider is not started");
    return (this.server.address() as AddressInfo).port;
  }

  get baseUrl(): string {
    return `http://127.0.0.1:${this.port}/v1`;
  }

  /** Requests that actually reached the quota endpoint, in order. */
  quotaRequests(): RecordedRequest[] {
    return this.requests.filter((request) => request.path === "/v1/quota");
  }

  async start(): Promise<void> {
    this.server = createServer((request, response) => this.handle(request, response));
    await new Promise<void>((resolve, reject) => {
      this.server!.once("error", reject);
      this.server!.listen(0, "127.0.0.1", () => resolve());
    });
  }

  async stop(): Promise<void> {
    const server = this.server;
    this.server = null;
    if (!server) return;
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }

  private send(response: ServerResponse, status: number, body: string, headers = {}): void {
    response.writeHead(status, {
      "content-type": "application/json",
      "content-length": Buffer.byteLength(body),
      connection: "close",
      ...headers,
    });
    response.end(body);
  }

  private handle(request: IncomingMessage, response: ServerResponse): void {
    const path = request.url?.split("?")[0] ?? "";
    const header = request.headers.authorization ?? "";
    const apiKey = header.startsWith("Bearer ") ? header.slice("Bearer ".length) : null;
    this.requests.push({ method: request.method ?? "", path, apiKey });

    if (path === "/v1/models") {
      this.send(response, 200, JSON.stringify({ data: [{ id: "fixture-model" }] }));
      return;
    }

    if (this.mode === "auth" || apiKey === null || !this.acceptedKeys.has(apiKey)) {
      this.send(response, 401, JSON.stringify({ error: "invalid api key" }));
      return;
    }

    if (path === "/v1/usage") {
      this.send(response, 200, JSON.stringify({ data: { used: 123 } }));
      return;
    }
    if (path === "/v1/reset") {
      this.send(
        response,
        200,
        JSON.stringify({ data: { reset_at: Math.floor(Date.now() / 1000) + 3_600 } }),
      );
      return;
    }
    if (path !== "/v1/quota") {
      this.send(response, 404, JSON.stringify({ error: "not found" }));
      return;
    }

    switch (this.mode) {
      case "server-error":
        this.send(response, 500, JSON.stringify({ error: "fixture outage" }));
        return;
      case "rate-limit":
        this.send(response, 429, JSON.stringify({ error: "slow down" }), {
          "retry-after": "30",
        });
        return;
      case "malformed":
        response.writeHead(200, { "content-type": "application/json", connection: "close" });
        response.end("this is not json");
        return;
      case "empty":
        this.send(response, 200, JSON.stringify({}));
        return;
      case "balance":
        this.send(
          response,
          200,
          JSON.stringify({ balance: 12.5, remaining: 12.5, currency: "USD" }),
        );
        return;
      case "low":
      case "windowed":
        this.send(response, 200, JSON.stringify(windowedBody(this.mode)));
        return;
    }
  }
}
