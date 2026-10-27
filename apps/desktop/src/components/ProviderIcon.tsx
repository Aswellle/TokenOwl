/**
 * Official AI provider brand icons from @lobehub/icons.
 *
 * Provider ids carry their brand as the prefix before the colon
 * (`openai:account-1`), so the brand is matched on the prefix and unknown
 * prefixes fall back to a neutral key glyph instead of guessing a brand.
 * The mapping tables live in `providerIconMap.ts`; the only built-in
 * providers still on the fallback are the `one-api` and `litellm` gateway
 * projects — the icon catalog does not carry them and their own project
 * logos are not bundled yet.
 */

import type { ReactNode } from "react";
import { Coze, Yi, ProviderIcon as LobeProviderIcon } from "@lobehub/icons";
import { DIRECT_BRANDS, PROVIDER_PREFIXES } from "./providerIconMap";

const DIRECT_COMPONENTS: Record<string, (props: { size: number }) => ReactNode> = {
  yi: ({ size }) => <Yi.Color size={size} />,
  coze: ({ size }) => <Coze size={size} />,
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
  const directBrand = prefix ? DIRECT_BRANDS[prefix] : undefined;

  if (directBrand) {
    const render = DIRECT_COMPONENTS[directBrand];
    return (
      <span className="provider-icon" style={{ width: size, height: size }} aria-hidden="true">
        {render({ size })}
      </span>
    );
  }

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
