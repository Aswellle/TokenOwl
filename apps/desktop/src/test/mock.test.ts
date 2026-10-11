import { describe, expect, it } from "vitest";
import { createMockBridge } from "../lib/mock";

describe("mock manifestProfiles", () => {
  it("mirrors the full built-in catalog with wire-protocol classification", async () => {
    const profiles = await createMockBridge().manifestProfiles();

    expect(profiles).toHaveLength(21);
    expect(new Set(profiles.map((item) => item.id)).size).toBe(21);

    const native = profiles.filter((item) => item.apiFormat !== "openai-compatible");
    expect(native.map((item) => item.id)).toEqual(["anthropic", "gemini"]);
    expect(native.map((item) => item.apiFormat)).toEqual(["anthropic", "google-ai"]);

    const gateways = profiles.filter((item) => item.customBaseUrl);
    expect(gateways.map((item) => item.id)).toEqual(["new-api", "one-api", "litellm"]);
    for (const gateway of gateways) {
      expect(gateway.baseUrl).toContain("your-gateway.example");
    }

    for (const item of profiles) {
      expect(item.name.length).toBeGreaterThan(0);
      expect(item.level).toMatch(/^L[1-4]$/);
      for (const path of Object.values(item.endpoints)) {
        expect(path).toMatch(/^\//);
      }
    }
  });
});
