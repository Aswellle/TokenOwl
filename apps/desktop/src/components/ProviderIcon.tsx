/**
 * Official AI provider brand icons from @lobehub/icons.
 *
 * Provider ids carry their brand as the prefix before the colon
 * (`openai:account-1`), so the brand is matched on the prefix and unknown
 * prefixes — including `generic.openai_compatible` relays — fall back to a
 * neutral key glyph instead of guessing a brand.
 */

import { ProviderIcon as LobeProviderIcon } from "@lobehub/icons";

/** providerId prefix → @lobehub/icons provider id. */
const PROVIDER_PREFIXES: Record<string, string> = {
  openai: "openai",
  anthropic: "anthropic",
  claude: "claude",
  gemini: "google",
  mistral: "mistral",
  groq: "groq",
  xai: "xai",
  siliconflow: "siliconcloud",
  stepfun: "stepfun",
  doubao: "doubao",
  baichuan: "baichuan",
  google: "google",
  openrouter: "openrouter",
  deepseek: "deepseek",
  qwen: "qwen",
  kimi: "moonshot",
  moonshot: "moonshot",
  minimax: "minimax",
  zhipu: "zhipu",
  glm: "zhipu",
  grok: "xai",
  cohere: "cohere",
  perplexity: "perplexity",
  ollama: "ollama",
  siliconcloud: "siliconcloud",
  together: "together",
  fireworks: "fireworks",
  cerebras: "cerebras",
  huggingface: "huggingface",
  microsoft: "microsoft",
  azure: "azure",
  bedrock: "bedrock",
  vertexai: "vertexai",
};

export function ProviderIcon({
  providerId,
  size = 20,
}: {
  providerId: string;
  size?: number;
}) {
  // Empty/undefined ids surface from freshly created accounts before the
  // provider row exists; the neutral fallback must still render.
  const prefix = (providerId ?? "").split(":")[0].trim().toLowerCase();
  const lobeProvider = prefix ? PROVIDER_PREFIXES[prefix] : undefined;

  if (!lobeProvider) {
    return (
      <span
        className="provider-icon provider-icon--fallback"
        style={{ width: size, height: size }}
        aria-hidden="true"
      >
        <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.6">
          <path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" />
        </svg>
      </span>
    );
  }

  return (
    <span className="provider-icon" style={{ width: size, height: size }} aria-hidden="true">
      <LobeProviderIcon provider={lobeProvider} size={size} type="color" />
    </span>
  );
}
