import type { ReactNode } from "react";
import { motion } from "motion/react";
import { IconAlert, IconShield } from "../../../components/icons";

const springPanel = { type: "spring", bounce: 0, duration: 0.42 } as const;
const fadeVariants = {
  hidden: { opacity: 0, y: -6 },
  show: { opacity: 1, y: 0, transition: springPanel },
  hide: { opacity: 0, y: -4, transition: { duration: 0.16, ease: [0.23, 1, 0.32, 1] as const } },
};

export function Notice({
  tone,
  icon,
  children,
  actions,
}: {
  tone?: "warning" | "danger" | "accent";
  icon?: boolean;
  children: ReactNode;
  /** Rendered as a trailing flex child, so a dismiss button sits on the
   * banner's own line at its trailing edge instead of wrapping below. */
  actions?: ReactNode;
}) {
  const Icon = tone === "danger" || tone === "warning" ? IconAlert : IconShield;
  return (
    <motion.p
      className={`notice${tone ? ` notice--${tone}` : ""}`}
      role="status"
      variants={fadeVariants}
      initial="hidden"
      animate="show"
      exit="hide"
    >
      {icon !== false && (
        <span className="notice__icon">
          <Icon size={14} />
        </span>
      )}
      <span className="notice__text">{children}</span>
      {actions && <span className="notice__actions">{actions}</span>}
    </motion.p>
  );
}
