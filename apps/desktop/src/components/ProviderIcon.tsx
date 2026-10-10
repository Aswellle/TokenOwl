/**
 * Official AI provider brand icons from @lobehub/icons.
 * Falls back to a generic key icon for unknown providers.
 */

import { ProviderIcon as LobeProviderIcon } from "@lobehub/icons";
import type { AccountHealth } from "../types";

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

export function HealthIcon({ health, size = 16 }: { health: AccountHealth; size?: number }) {
  const iconMap: Record<string, { color: string; path: string }> = {
    healthy: { color: "var(--success)", path: "M20 6L9 17l-5-5" },
    attention: { color: "var(--warning)", path: "M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" },
    critical: { color: "var(--error)", path: "M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" },
    stale: { color: "var(--muted)", path: "M12 8v4l3 3m6-3a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" },
    error: { color: "var(--error)", path: "M18 6L6 18M6 6l12 12" },
    syncing: { color: "var(--accent)", path: "M23 4v6h-6M1 20v-6h6M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" },
    unavailable: { color: "var(--muted)", path: "M18.36 6.64a9 9 0 1 1-12.73 0M12 2v10" },
    offline: { color: "var(--muted)", path: "M1 1l22 22M16.72 11.06A10.94 10.94 0 0 1 19 12.55M5 12.55a10.94 10.94 0 0 1 5.17-2.39M10.71 5.05A16 16 0 0 1 22.58 9M1.42 9a15.91 15.91 0 0 1 4.7-2.88M8.53 16.11a6 6 0 0 1-6.95 0M12 20h.01" },
  };

  const icon = iconMap[health] ?? iconMap.unavailable;

  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke={icon.color}
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={icon.path} />
    </svg>
  );
}
