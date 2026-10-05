/**
 * Turns off the webview's own right-click menu.
 *
 * WebView2's default menu offers Back, Refresh, Save as, Print and Inspect —
 * browser actions that make no sense in a desktop app, and "Refresh" reloads
 * the whole UI and drops the session's state. Every surface that has actions
 * now provides its own menu, so anywhere else a right-click does nothing, as
 * it does on the empty parts of any native window.
 *
 * Text fields keep the native menu: Cut, Copy and Paste on a search box are
 * what a right-click there is for. Selected text keeps it too, so copying a
 * value read off the screen still works.
 */
export function suppressNativeContextMenu(target: Document = document): () => void {
  const onContextMenu = (event: MouseEvent): void => {
    if (event.defaultPrevented) return;
    if (keepsNativeMenu(event.target)) return;
    event.preventDefault();
  };
  target.addEventListener('contextmenu', onContextMenu);
  return () => target.removeEventListener('contextmenu', onContextMenu);
}

function keepsNativeMenu(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target.closest('input, textarea, [contenteditable=""], [contenteditable="true"]')) {
    return true;
  }
  const selection = target.ownerDocument.getSelection();
  return selection !== null && !selection.isCollapsed && selection.toString().trim() !== '';
}
