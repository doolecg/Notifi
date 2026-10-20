/** Apple-style ease-out: fast start, long soft landing. */
const expoOut = (t: number) => (t === 1 ? 1 : 1 - Math.pow(2, -10 * t));
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
/** Softer, longer springs for the Apple-like feel: k 230 c 24, k 380 c 32, k 300 c 20, k 240 c 20, k 260 c 19. */
const A_SOFT = { stiffness: 0.0639, damping: 0.4000 };
const A_SNAP = { stiffness: 0.1056, damping: 0.5333 };
const A_BOUNCE = { stiffness: 0.0833, damping: 0.3333 };
const A_TOAST = { stiffness: 0.0667, damping: 0.3333 };
const A_LIVELY = { stiffness: 0.0722, damping: 0.3167 };
const APPLE = 'cubic-bezier(0.32, 0.72, 0, 1)';

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
  panelOpen: sprung(420, 'linear(0, 0.038, 0.125, 0.238, 0.359, 0.478, 0.59, 0.685, 0.766, 0.831, 0.884, 0.924, 0.954, 0.975, 0.989, 0.998, 1.004, 1.006, 1.006, 1.006, 1.004, 1.002, 1)', A_SOFT),
  panelClose: curve(240, APPLE),
  tabSwitch: curve(340, APPLE),
  switcherShow: curve(140, EASE_OUT, 0),
  switcherHide: curve(320, EASE_IN_OUT),
  windowRaise: curve(220, APPLE, 0),
  selectionMove: curve(170, APPLE, 0),
  pillExpand: sprung(560, 'linear(0, 0.062, 0.2, 0.363, 0.521, 0.66, 0.773, 0.859, 0.921, 0.963, 0.99, 1.005, 1.013, 1.016, 1.015, 1.013, 1.011, 1.008, 1.006, 1.004, 1.002, 1.001, 1)', A_SOFT),
  pillCollapse: sprung(380, 'linear(0, 0.05, 0.162, 0.296, 0.436, 0.562, 0.675, 0.765, 0.838, 0.893, 0.934, 0.962, 0.982, 0.994, 1.001, 1.005, 1.006, 1.006, 1.005, 1.004, 1.003, 1.001, 1)', A_SNAP),
  /** Island liquid-glass feel: k 210 c 17 (about 11% overshoot), and a calmer k 260 c 26 for the way back. */
  pillDrop: sprung(620, 'linear(0, 0.063, 0.21, 0.391, 0.58, 0.749, 0.885, 0.987, 1.055, 1.093, 1.108, 1.105, 1.093, 1.075, 1.056, 1.038, 1.023, 1.011, 1.003, 0.998, 0.996, 0.995, 0.996, 0.998, 1)', { stiffness: 0.0583, damping: 0.2833 }, { reduced: 80 }),
  pillRise: sprung(420, 'linear(0, 0.049, 0.16, 0.297, 0.438, 0.568, 0.68, 0.773, 0.845, 0.9, 0.94, 0.968, 0.986, 0.997, 1.003, 1.006, 1.006, 1.005, 1.004, 1.002, 1)', { stiffness: 0.0722, damping: 0.4333 }),
  /** Content melting in: blur to sharp. */
  glassIn: curve(480, 'cubic-bezier(0.16, 1, 0.3, 1)'),
  toastIn: sprung(480, 'linear(0, 0.051, 0.173, 0.325, 0.489, 0.641, 0.771, 0.878, 0.958, 1.013, 1.048, 1.066, 1.072, 1.07, 1.062, 1.052, 1.04, 1.03, 1.02, 1.013, 1.007, 1.003, 1)', A_TOAST),
  toastOut: curve(260, APPLE),
  commandBarOpen: sprung(420, 'linear(0, 0.038, 0.125, 0.238, 0.359, 0.478, 0.59, 0.685, 0.766, 0.831, 0.884, 0.924, 0.954, 0.975, 0.989, 0.998, 1.004, 1.006, 1.006, 1.006, 1.004, 1.002, 1)', A_SOFT),
  radialOpen: sprung(360, 'linear(0, 0.032, 0.113, 0.224, 0.347, 0.478, 0.603, 0.717, 0.814, 0.896, 0.961, 1.009, 1.042, 1.063, 1.073, 1.074, 1.07, 1.06, 1.049, 1.036, 1.023, 1.011, 1)', A_LIVELY),
  radialItemStagger: sprung(320, 'linear(0, 0.026, 0.09, 0.179, 0.284, 0.395, 0.506, 0.611, 0.707, 0.791, 0.862, 0.921, 0.967, 1.001, 1.024, 1.037, 1.044, 1.044, 1.039, 1.032, 1.022, 1.011, 1)', A_LIVELY, { stagger: 20 }),
  shelfIn: sprung(380, 'linear(0, 0.031, 0.106, 0.203, 0.313, 0.421, 0.526, 0.619, 0.703, 0.772, 0.831, 0.878, 0.915, 0.944, 0.965, 0.98, 0.99, 0.997, 1, 1.002, 1.002, 1.001, 1)', A_SOFT),
  shelfOut: curve(260, APPLE),
  shelfDropHighlight: curve(160, APPLE, 0),
  snapPreview: sprung(280, 'linear(0, 0.029, 0.095, 0.187, 0.286, 0.39, 0.487, 0.579, 0.659, 0.73, 0.789, 0.838, 0.88, 0.912, 0.938, 0.957, 0.972, 0.983, 0.99, 0.995, 0.998, 0.999, 1)', A_SNAP, { reduced: 0 }),
  captureSelector,
  /** Fill is linear for the hold time (900 ms default); release drains back in 150 ms with easeOut. */
  holdToQuitProgress,
  holdDrain: curve(150, EASE_OUT, 150),
  listReorder: sprung(420, 'linear(0, 0.038, 0.125, 0.238, 0.359, 0.478, 0.59, 0.685, 0.766, 0.831, 0.884, 0.924, 0.954, 0.975, 0.989, 0.998, 1.004, 1.006, 1.006, 1.006, 1.004, 1.002, 1)', A_SOFT, { reduced: 0 }),
  toggleSwitch: sprung(340, 'linear(0, 0.033, 0.116, 0.23, 0.36, 0.493, 0.621, 0.737, 0.837, 0.919, 0.983, 1.029, 1.06, 1.078, 1.086, 1.085, 1.077, 1.066, 1.052, 1.038, 1.024, 1.011, 1)', A_BOUNCE, { reduced: 0 }),
  hoverLift: curve(200, APPLE),
  /** Fade time after a 400 ms dwell. */
  tooltipIn: curve(200, APPLE),
  // Names kept from the earlier API (not separate tokens in the spec).
  commandBarClose: curve(240, APPLE),
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
  document.documentElement.toggleAttribute('data-reduced-motion', reducedMotion());
  for (const name of Object.keys(motion) as MotionName[]) {
    style.setProperty(`--m-${name}-dur`, `${duration(name)}ms`);
    style.setProperty(`--m-${name}-ease`, reducedMotion() ? LINEAR : motion[name].ease);
  }
}

osQuery?.addEventListener('change', applyCssVars);

export function fadeSlide(
  _node: Element,
  { token, y = 10, delay = 0 }: { token: MotionName; y?: number; delay?: number },
): TransitionConfig {
  const reduced = reducedMotion();
  return {
    delay: reduced ? 0 : delay,
    duration: duration(token),
    easing: expoOut,
    css: (t, u) =>
      reduced ? `opacity:${t}` : `opacity:${t};transform:translateY(${u * y}px) scale(${1 - u * 0.02});filter:blur(${u * 4}px)`,
  };
}
