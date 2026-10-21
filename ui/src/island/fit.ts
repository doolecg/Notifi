/** The width a row of items needs: its padding, plus each item's width and margins. An item with an inner `.nat` is measured by that, so text that stretches to fill the row counts at its own length. */
export function naturalWidth(row: HTMLElement): number {
  const px = (v: string) => parseFloat(v) || 0;
  const rs = getComputedStyle(row);
  let w = px(rs.paddingLeft) + px(rs.paddingRight);
  for (const el of row.children) {
    const s = getComputedStyle(el);
    if (s.position === 'absolute') continue;
    w += (el.querySelector(':scope > .nat') ?? el).getBoundingClientRect().width + px(s.marginLeft) + px(s.marginRight);
  }
  return Math.ceil(w);
}

/** Svelte action: tells `report` how wide the row needs to be whenever it or anything in it changes size. */
export function fit(row: HTMLElement, report: ((width: number) => void) | null) {
  if (!report) return;
  const watcher = new ResizeObserver(() => report(naturalWidth(row)));
  watcher.observe(row);
  for (const el of row.querySelectorAll(':scope > *, :scope > * > .nat')) watcher.observe(el);
  report(naturalWidth(row));
  return { destroy: () => watcher.disconnect() };
}
