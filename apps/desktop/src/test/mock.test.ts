import { describe, expect, it } from "vitest";
import { createMockBridge } from "../lib/mock";

describe("mock manifestProfiles", () => {
  it("mirrors the full built-in catalog with wire-protocol classification", async () => {
    const { profiles } = await createMockBridge().manifestProfiles();

    // The mock mirrors the built-in catalog plus the OpenAI-compatible
    // relays and the 国际版 platform variants of multi-platform brands.
    expect(profiles).toHaveLength(25);
    expect(new Set(profiles.map((item) => item.id)).size).toBe(25);

    // Platform variants of one brand share the brand key and carry their
    // platform label — the picker collapses them into one capsule.
    const zhipu = profiles.filter((item) => item.brand === "zhipu");
    expect(zhipu.map((item) => item.variant)).toEqual(["国内版", "国际版"]);
    const kimi = profiles.filter((item) => item.brand === "kimi");
    expect(kimi.map((item) => item.variant)).toEqual(["国内版", "国际版"]);

    const native = profiles.filter((item) => item.apiFormat !== "openai-compatible");
    expect(native.map((item) => item.id)).toEqual(["anthropic", "gemini"]);
    expect(native.map((item) => item.apiFormat)).toEqual(["anthropic", "google-ai"]);

    const gateways = profiles.filter((item) => item.customBaseUrl);
    expect(gateways.map((item) => item.id)).toEqual(["new-api", "one-api", "litellm"]);

    // Both providers whose native protocol is offered also expose the
    // official OpenAI-compatible entry as a separate, selectable profile.
    const compatVariants = profiles.filter((item) => item.id.endsWith("-openai"));
    expect(compatVariants.map((item) => item.id)).toEqual(["gemini-openai", "anthropic-openai"]);
    for (const variant of compatVariants) {
      expect(variant.apiFormat).toBe("openai-compatible");
    }
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
