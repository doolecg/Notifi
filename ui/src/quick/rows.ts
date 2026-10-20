import { invoke } from '@tauri-apps/api/core';

export interface Row {
  id: string;
  label: string;
  hint: string;
  kind: 'switch' | 'action' | 'tool';
  on: boolean;
  disabled: boolean;
  note: string;
  confirm: boolean;
}

export const GLYPHS: Record<string, string> = {
  darkMode: 'M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5Z',
  hiddenFiles: 'M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6Z M12 9.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z',
  fileExtensions: 'M6 3.5h8l4 4v13H6z M14 3.5v4h4 M9 14h6',
  desktopIcons: 'M4 5h7v6H4z M13 5h7v6h-7z M4 13h7v6H4z M13 13h7v6h-7z',
  nightLight: 'M12 3v2 M12 19v2 M3 12h2 M19 12h2 M5.6 5.6 7 7 M17 17l1.4 1.4 M18.4 5.6 17 7 M7 17l-1.4 1.4 M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8Z',
  dnd: 'M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17Z M8 12h8',
  muteMic: 'M12 4a3 3 0 0 1 3 3v4a3 3 0 0 1-6 0V7a3 3 0 0 1 3-3Z M6 11a6 6 0 0 0 12 0 M12 17v3.5 M4 4l16 16',
  keepAwake: 'M5 9h11v5a5 5 0 0 1-5 5h-1a5 5 0 0 1-5-5V9Z M16 10.5h1.5a2.5 2.5 0 0 1 0 5H16',
  ejectAll: 'M12 5l7 8H5l7-8Z M5 18h14',
  emptyTrash: 'M5 7h14 M9 7V4.5h6V7 M7 7l1 13h8l1-13 M10 11v5 M14 11v5',
  lockScreen: 'M6.5 11h11v8.5h-11z M8.5 11V8a3.5 3.5 0 0 1 7 0v3',
  'tool:scratchpad': 'M5 4.5h14v15H5z M8.5 9h7 M8.5 12.5h7 M8.5 16h4',
  'tool:clipboardHistory': 'M9 4.5h6v3H9z M6.5 6h11v14h-11z M9.5 12h5 M9.5 15.5h5',
  'tool:textSnippets': 'M8 8l-4 4 4 4 M16 8l4 4-4 4 M13.5 6l-3 12',
  'tool:settings': 'M4 7h9 M17 7h3 M4 17h3 M11 17h9 M15 5v4 M9 15v4',
};

export function rowGlyph(id: string): string {
  return GLYPHS[id] ?? GLYPHS.dnd;
}

/** Runs a row. Rows that ask first (emptying the bin) return `'confirm'` on the first call. */
export async function runRow(row: Row, confirmed: boolean): Promise<'confirm' | 'done'> {
  if (row.confirm && !confirmed) return 'confirm';
  await invoke('quick_run', { id: row.id, on: row.kind === 'switch' ? !row.on : null });
  return 'done';
}
