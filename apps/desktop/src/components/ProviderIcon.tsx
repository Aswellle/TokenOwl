/**
 * Official AI provider brand icons from @lobehub/icons.
 * Falls back to a generic key icon for unknown providers.
 */

import { ProviderIcon as LobeProviderIcon } from "@lobehub/icons";

export function ProviderIcon({
  providerId,
  size = 20,
}: {
  providerId: string;
  size?: number;
}) {
  const providerMap: Record<string, string> = {
    openai: "openai",
    anthropic: "anthropic",
    gemini: "google",
    openrouter: "openrouter",
    deepseek: "deepseek",
    qwen: "qwen",
    kimi: "kimi",
    minimax: "minimax",
  };

  const lobeProvider = providerMap[providerId.toLowerCase()];

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

