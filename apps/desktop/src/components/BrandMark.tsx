import { useId } from "react";

/**
 * The product mark: a geometric owl drawn as one continuous silhouette.
 *
 * The eyes and beak are cut out of the silhouette with a mask rather than
 * painted in a background colour, so the mark stays correct on the titlebar
 * tile, on the dashboard's light surface, and on the bubble's dark jewel
 * without a second variant. Filled shapes (not strokes) keep it crisp at the
 * 19 px titlebar size, where the previous line-art owl turned into noise.
 */
export function BrandMark({
  size = 19,
  className,
}: {
  size?: number;
  className?: string;
}) {
  // React ids may contain characters that are awkward inside url(#…).
  const maskId = `owl-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      <mask
        id={maskId}
        maskUnits="userSpaceOnUse"
        x="0"
        y="0"
        width="24"
        height="24"
      >
        <rect width="24" height="24" fill="#fff" />
        <ellipse cx="9.1" cy="11.4" rx="2.35" ry="2.5" fill="#000" />
        <ellipse cx="14.9" cy="11.4" rx="2.35" ry="2.5" fill="#000" />
        <path d="M12 14.1 L13.4 16.1 L12 17.55 L10.6 16.1 Z" fill="#000" />
      </mask>
      <path
        d="M12 19.9 C8 19.9 4.3 16.6 4.3 12.6 C4.3 10 5.4 7.9 7.1 6.5 L5.7 3 L10.2 4.9 C10.8 4.78 11.4 4.72 12 4.72 C12.6 4.72 13.2 4.78 13.8 4.9 L18.3 3 L16.9 6.5 C18.6 7.9 19.7 10 19.7 12.6 C19.7 16.6 16 19.9 12 19.9 Z"
        fill="currentColor"
        mask={`url(#${maskId})`}
      />
      <circle cx="9.1" cy="11.4" r="0.92" fill="currentColor" />
      <circle cx="14.9" cy="11.4" r="0.92" fill="currentColor" />
    </svg>
  );
}
