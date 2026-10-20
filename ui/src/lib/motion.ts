import { cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

// Values from spec/motion.md. Springs: Svelte `Spring` params are per 60 fps frame,
// so stiffness = k / 3600 and damping = c / 60 (damping ratio is preserved).
// Each spring token's `ease` is a CSS linear() sampled from the same spring over
// its duration (rescaled to end at 1), so CSS and script motion match.

export interface MotionToken {
  /** Tween and CSS duration in ms. */
  duration: number;
  /** CSS easing for transitions driven by stylesheets (cubic-bezier, linear, or linear() for springs). */
  ease: string;
  /** Svelte `Spring` options for values driven from script (stiffness/damping in Svelte's 0..1 terms). */
  spring: { stiffness: number; damping: number };
  /** Delay between items in a list, in ms. */
  stagger?: number;
  /** Duration under reduced motion in ms (spec section 3). */
  reduced: number;
}

const EASE_OUT = 'cubic-bezier(0.22, 1, 0.36, 1)';
const EASE_IN_OUT = 'cubic-bezier(0.4, 0, 0.2, 1)';
const LINEAR = 'linear';

/** S-snap k 420 c 34, S-soft k 300 c 25, S-lively k 260 c 20 (m 1). */
const S_SNAP = { stiffness: 0.1167, damping: 0.5667 };
const S_SOFT = { stiffness: 0.0833, damping: 0.4167 };
const S_LIVELY = { stiffness: 0.0722, damping: 0.3333 };

const curve = (duration: number, ease: string, reduced = 80, spring = S_SNAP): MotionToken => ({
  duration,
  ease,
  spring,
  reduced,
});
const sprung = (
  duration: number,
  ease: string,
  spring: { stiffness: number; damping: number },
  extra: Partial<MotionToken> = {},
): MotionToken => ({ duration, ease, spring, reduced: 80, ...extra });

const holdToQuitProgress = curve(900, LINEAR, 900);
const captureSelector = curve(100, LINEAR, 0);

export const motion = {
  panelOpen: sprung(180, 'linear(0, 0.016, 0.058, 0.117, 0.188, 0.265, 0.344, 0.423, 0.499, 0.571, 0.638, 0.699, 0.754, 0.802, 0.845, 0.883, 0.915, 0.942, 0.965, 0.984, 1)', S_SNAP),
  panelClose: curve(120, EASE_OUT),
  tabSwitch: curve(160, EASE_OUT),
  switcherShow: curve(60, EASE_OUT, 0),
  switcherHide: curve(300, EASE_IN_OUT),
  windowRaise: curve(150, EASE_OUT, 0),
  selectionMove: curve(70, EASE_OUT, 0),
  pillExpand: sprung(320, 'linear(0, 0.033, 0.114, 0.222, 0.342, 0.463, 0.575, 0.676, 0.762, 0.833, 0.89, 0.933, 0.965, 0.987, 1, 1.008, 1.011, 1.011, 1.008, 1.004, 1)', S_SOFT),
  pillCollapse: sprung(260, 'linear(0, 0.03, 0.105, 0.203, 0.311, 0.419, 0.522, 0.614, 0.695, 0.764, 0.821, 0.868, 0.905, 0.933, 0.955, 0.971, 0.982, 0.99, 0.995, 0.998, 1)', S_SNAP),
  toastIn: sprung(220, 'linear(0, 0.016, 0.059, 0.12, 0.194, 0.275, 0.358, 0.442, 0.522, 0.597, 0.666, 0.728, 0.783, 0.831, 0.873, 0.907, 0.936, 0.958, 0.976, 0.99, 1)', S_SOFT),
  toastOut: curve(180, EASE_OUT),
  commandBarOpen: sprung(220, 'linear(0, 0.016, 0.059, 0.12, 0.194, 0.275, 0.358, 0.442, 0.522, 0.597, 0.666, 0.728, 0.783, 0.831, 0.873, 0.907, 0.936, 0.958, 0.976, 0.99, 1)', S_SOFT),
  radialOpen: sprung(200, 'linear(0, 0.012, 0.043, 0.09, 0.149, 0.217, 0.289, 0.364, 0.439, 0.513, 0.583, 0.65, 0.711, 0.767, 0.818, 0.862, 0.9, 0.933, 0.96, 0.983, 1)', S_LIVELY),
  radialItemStagger: sprung(180, 'linear(0, 0.01, 0.037, 0.078, 0.13, 0.19, 0.255, 0.324, 0.394, 0.465, 0.534, 0.6, 0.663, 0.722, 0.776, 0.826, 0.871, 0.91, 0.945, 0.975, 1)', S_LIVELY, { stagger: 20 }),
  shelfIn: curve(120, EASE_OUT),
  shelfOut: curve(160, EASE_OUT),
  shelfDropHighlight: curve(80, EASE_OUT, 0),
  snapPreview: sprung(160, 'linear(0, 0.013, 0.049, 0.1, 0.162, 0.231, 0.303, 0.377, 0.45, 0.52, 0.587, 0.649, 0.707, 0.76, 0.808, 0.851, 0.889, 0.923, 0.953, 0.978, 1)', S_SNAP, { reduced: 0 }),
  captureSelector,
  /** Fill is linear for the hold time (900 ms default); release drains back in 150 ms with easeOut. */
  holdToQuitProgress,
  holdDrain: curve(150, EASE_OUT, 150),
  listReorder: sprung(220, 'linear(0, 0.016, 0.059, 0.12, 0.194, 0.275, 0.358, 0.442, 0.522, 0.597, 0.666, 0.728, 0.783, 0.831, 0.873, 0.907, 0.936, 0.958, 0.976, 0.99, 1)', S_SOFT, { reduced: 0 }),
  toggleSwitch: sprung(160, 'linear(0, 0.013, 0.049, 0.1, 0.162, 0.231, 0.303, 0.377, 0.45, 0.52, 0.587, 0.649, 0.707, 0.76, 0.808, 0.851, 0.889, 0.923, 0.953, 0.978, 1)', S_SNAP, { reduced: 0 }),
  hoverLift: curve(120, EASE_OUT),
  /** Fade time after a 400 ms dwell. */
  tooltipIn: curve(120, EASE_OUT),
  // Names kept from the earlier API (not separate tokens in the spec).
  commandBarClose: curve(140, EASE_OUT),
  captureSelect: captureSelector,
  holdProgress: holdToQuitProgress,
} satisfies Record<string, MotionToken>;

export type MotionName = keyof typeof motion;

/** Dwell before a tooltip shows, in ms. */
export const tooltipDwell = 400;

let userReduced = false;
const osQuery = typeof window === 'undefined' ? null : window.matchMedia('(prefers-reduced-motion: reduce)');

export function reducedMotion(): boolean {
  return userReduced || !!osQuery?.matches;
}

export function setReducedMotion(on: boolean) {
  userReduced = on;
  applyCssVars();
}

export function duration(name: MotionName): number {
  const t: MotionToken = motion[name];
  return reducedMotion() ? t.reduced : t.duration;
}

export function springOptions(name: MotionName) {
  return motion[name].spring;
}

/** Writes every token as `--m-<name>-dur` and `--m-<name>-ease` on :root. */
export function applyCssVars() {
  const style = document.documentElement.style;
  for (const name of Object.keys(motion) as MotionName[]) {
    style.setProperty(`--m-${name}-dur`, `${duration(name)}ms`);
    style.setProperty(`--m-${name}-ease`, reducedMotion() ? LINEAR : motion[name].ease);
  }
}

osQuery?.addEventListener('change', applyCssVars);

export function fadeSlide(
  _node: Element,
  { token, y = 6, delay = 0 }: { token: MotionName; y?: number; delay?: number },
): TransitionConfig {
  const reduced = reducedMotion();
  return {
    delay: reduced ? 0 : delay,
    duration: duration(token),
    easing: cubicOut,
    css: (t, u) => (reduced ? `opacity:${t}` : `opacity:${t};transform:translateY(${u * y}px)`),
  };
}
