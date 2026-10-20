const MODIFIER_CODES = new Set([
  'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight', 'OSLeft', 'OSRight',
]);

/** Builds an accelerator like `Ctrl+Alt+N` from a key event, or null while only modifiers are held. */
export function fromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push('Super');
  const key = keyName(e.code);
  const isFn = /^F\d+$/.test(key);
  if (!parts.length && !isFn) return null;
  parts.push(key);
  return parts.join('+');
}

function keyName(code: string): string {
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  return code;
}

/** Normalises case and modifier order so two spellings of a combo compare equal. */
export function normalize(accel: string): string {
  const order = ['ctrl', 'alt', 'shift', 'super'];
  const alias: Record<string, string> = { control: 'ctrl', commandorcontrol: 'ctrl', cmdorctrl: 'ctrl', meta: 'super', win: 'super' };
  const parts = accel.split('+').map((p) => p.trim().toLowerCase()).map((p) => alias[p] ?? p);
  const mods = parts.filter((p) => order.includes(p)).sort((a, b) => order.indexOf(a) - order.indexOf(b));
  const keys = parts.filter((p) => !order.includes(p)).map((k) => k.replace(/^(key|digit)/, ''));
  return [...mods, ...keys].join('+');
}

export function keycaps(accel: string): string[] {
  const label: Record<string, string> = { Super: navigator.userAgent.includes('Windows') ? 'Win' : 'Super',ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', Period: '.', Comma: ',' };
  return accel ? accel.split('+').map((p) => label[p] ?? p) : [];
}
