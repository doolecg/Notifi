// Run in the chat host's page by Notifi. The page has no way to call Notifi, so it talks by asking for an
// address Notifi stops (see CLOSE_URL), and Notifi drives the motion below with eval.
(() => {
  // Escape closes the Island, unless one of the host's own popups is open (checked before the host closes it).
  addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && !document.querySelector('[role=dialog],[role=menu],[role=listbox]')) location.href = '__CLOSE_URL__';
  }, true);

  // The chat is a native view over the menu and cannot be clipped or scaled with it, so its page moves itself:
  // it grows out of the resting pill and shrinks back into it on the menu's own spring, or just fades.
  let anim = null;
  let style = null;
  const clear = () => {
    anim?.cancel();
    anim = null;
    style?.remove();
    style = null;
    document.documentElement.style.opacity = '';
    for (const p of ['transform', 'opacity', 'filter']) document.body?.style.removeProperty(p);
  };
  /** A CSS `linear(a, b, c, ...)` easing (evenly spaced points, as the menu's springs are) as a function. */
  const curve = (ease) => {
    const pts = /^linear\((.*)\)$/.exec(ease.trim())?.[1].split(',').map(Number);
    if (!pts || pts.length < 2 || pts.some(Number.isNaN)) return (t) => t;
    return (t) => {
      const at = t * (pts.length - 1);
      const i = Math.min(pts.length - 2, Math.floor(at));
      return pts[i] + (pts[i + 1] - pts[i]) * (at - i);
    };
  };
  /** Runs `step(eased progress)` every frame for `ms`. Driven from the page rather than handed to the compositor:
   *  the host's sticky headers are placed on the page's own frames, and lag behind a compositor animation. */
  const tween = (ms, ease, step, done) => {
    const f = curve(ease);
    const t0 = performance.now();
    let id = 0;
    const tick = (now) => {
      const t = Math.min(1, (now - t0) / ms);
      step(f(t));
      if (t < 1) id = requestAnimationFrame(tick);
      else done?.();
    };
    step(f(0));
    id = requestAnimationFrame(tick);
    anim = { cancel: () => cancelAnimationFrame(id) };
  };
  const run = (el, frames, ms, easing, done) => {
    anim = el.animate(frames, { duration: ms, easing, fill: 'forwards' });
    if (done) anim.onfinish = clear;
  };
  window.__notifi = {
    /** Before showing: nothing visible until the motion starts. */
    hidden() {
      clear();
      document.documentElement.style.opacity = '0';
    },
    fade(into) {
      clear();
      run(document.documentElement, into ? [{ opacity: 0 }, { opacity: 1 }] : [{ opacity: 1 }, { opacity: 0 }], into ? 180 : 140, into ? 'cubic-bezier(0, 0, 0.2, 1)' : 'cubic-bezier(0.4, 0, 1, 1)', into);
    },
    /** m: origin (ox, oy) in page pixels, the pill's resting scale (sx, sy), duration, easing and blur. */
    morph(into, m) {
      clear();
      // The root's background paints the whole view whatever its transform, so the body carries it while moving.
      // The root keeps a background that is all but clear: a fully clear one would hand the body's to the whole view again.
      const bg = [document.documentElement, document.body].map((e) => getComputedStyle(e).backgroundColor).find((c) => c && c !== 'transparent' && c !== 'rgba(0, 0, 0, 0)') ?? '#000';
      style = document.createElement('style');
      style.textContent = `html{background:rgb(0 0 0 / 0.004)!important}body{background:${bg}!important;transform-origin:${m.ox}px ${m.oy}px;will-change:transform,opacity${m.blur ? ',filter' : ''}}`;
      document.head.append(style);
      const b = document.body.style;
      const clamp = (v) => Math.min(1, Math.max(0, v));
      // e runs 0 (resting pill) to 1 (full size), overshooting a little on the spring.
      const draw = (e) => {
        b.transform = `scale(${m.sx + (1 - m.sx) * e}, ${m.sy + (1 - m.sy) * e})`;
        // Seen while it is big: in, it is solid by half way; out, it is gone before it gets small.
        b.opacity = String(into ? clamp(e / 0.5) : clamp((e - 0.4) / 0.6));
        if (m.blur) b.filter = `blur(${(m.blur * (1 - clamp(e))).toFixed(2)}px)`;
      };
      document.documentElement.style.opacity = '';
      tween(m.ms, m.ease, (p) => draw(into ? p : 1 - p), into ? clear : null);
    },
  };
})();
