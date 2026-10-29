import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function WindowTitlebar() {
  return <header className="window-titlebar" data-tauri-frame-tb onDoubleClick={event => {
    if ((event.target as HTMLElement).closest('button')) return;
    if (isTauri()) void getCurrentWindow().toggleMaximize();
  }}>
    <span className="window-brand" data-tauri-drag-region><img src="/mark.svg" alt="" data-tauri-drag-region />Only Monitor</span>
    <div className="window-drag-space" data-tauri-drag-region />
  </header>;
}
