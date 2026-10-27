import { describe, expect, it } from "vitest";
import { DIRECT_BRANDS, PROVIDER_PREFIXES } from "../components/providerIconMap";

/**
 * Every built-in manifest id, kept in step with `providers/builtin/*.yaml`.
 * The two gateway projects stay on the neutral key glyph until their project
 * logos are bundled (the icon catalog does not carry them); every other
 * built-in must resolve to an official brand icon, so adding a manifest
 * without an icon mapping fails here instead of shipping a default glyph.
 */
const BUILTIN_PROVIDER_IDS = [
  "01ai",
  "360",
  "anthropic",
  "anthropic-openai",
  "baichuan",
  "baidu-qianfan",
  "cohere",
  "coze",
  "deepseek",
  "doubao",
  "gemini",
  "gemini-openai",
  "groq",
  "iflytek",
  "kimi",
  "litellm",
  "minimax",
  "mistral",
  "new-api",
  "one-api",
  "openai",
  "openrouter",
  "qwen",
  "siliconflow",
  "stepfun",
  "tencent-hunyuan",
  "xiaomi-mimo",
  "xai",
  "zhipu",
  "zhipu-international",
  "kimi-international",
] as const;

const KNOWN_FALLBACKS = new Set(["one-api", "litellm"]);

describe("provider icon coverage", () => {
  it("resolves every built-in provider except the unbundled gateways", () => {
    for (const id of BUILTIN_PROVIDER_IDS) {
      if (KNOWN_FALLBACKS.has(id)) {
        expect(PROVIDER_PREFIXES[id]).toBeUndefined();
        expect(DIRECT_BRANDS[id]).toBeUndefined();
        continue;
      }
      const mapped =
        PROVIDER_PREFIXES[id] !== undefined || DIRECT_BRANDS[id] !== undefined;
      expect(mapped, `built-in "${id}" must map to an official brand icon`).toBe(true);
    }
  });

  it("maps every catalog entry to a non-empty lobehub provider id", () => {
    for (const [prefix, lobeId] of Object.entries(PROVIDER_PREFIXES)) {
      expect(lobeId.trim().length, `prefix "${prefix}" maps to an empty id`).toBeGreaterThan(0);
    }
  });
});
