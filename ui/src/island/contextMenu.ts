import { invoke } from '@tauri-apps/api/core';

/**
 * Right-clicking the Island or the agents bar shows Notifi's own menu instead of the web view's (Reload, Inspect).
 * A click that something inside already handled (a tile's own right-click action) is left alone, and so is one on
 * text being edited, where the cut and paste menu is wanted. Returns the function that removes the listener.
 */
export function installContextMenu(): () => void {
  const onMenu = (e: MouseEvent) => {
    if (e.defaultPrevented) return;
    if ((e.target as Element | null)?.closest('input, textarea, [contenteditable="true"]')) return;
    e.preventDefault();
    invoke('island_context_menu').catch(() => {});
  };
  window.addEventListener('contextmenu', onMenu);
  return () => window.removeEventListener('contextmenu', onMenu);
}
