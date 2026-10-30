/**
 * The single GSAP entry point.
 *
 * GSAP owns exactly one slice of the animation surface: the quota rings'
 * `stroke-dashoffset`, their gradient stops, and the bubble's local
 * expand/collapse beat inside the WebView. Motion (`motion/react`) keeps the
 * page transitions, list animations, hover/tap feedback and number springs;
 * Rust keeps the native window region morph (`SetWindowRgn`) that clips the
 * acrylic material. Nothing here may write an attribute another engine owns.
 *
 * `useGSAP` registers its own plugin so `gsap.context()` cleanup survives
 * tree-shaking; importing GSAP through this module keeps that registration to
 * one place instead of scattering `registerPlugin` calls across features.
 */
import gsap from "gsap";
import { useGSAP } from "@gsap/react";

gsap.registerPlugin(useGSAP);

export { gsap, useGSAP };
