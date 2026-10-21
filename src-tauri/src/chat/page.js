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
      const small = { transform: `scale(${m.sx}, ${m.sy})`, opacity: 0, ...(m.blur && { filter: `blur(${m.blur}px)` }) };
      const full = { transform: 'none', opacity: 1, ...(m.blur && { filter: 'blur(0px)' }) };
      // Seen while it is big: in, it is solid by half way; out, it is gone before it gets small.
      run(document.body, into ? [small, { opacity: 1, offset: 0.5 }, full] : [full, { opacity: 0, offset: 0.6 }, small], m.ms, m.ease, into);
      document.documentElement.style.opacity = '';
    },
  };
})();
