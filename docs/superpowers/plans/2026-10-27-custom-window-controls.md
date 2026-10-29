# Custom Window Controls Implementation Plan

> **For agentic workers:** use the executing-plans workflow to implement the steps.

**Goal:** Replace the Windows system titlebar with integrated Only Monitor window controls while preserving Windows 11 Snap Layouts.

**Architecture:** Disable Tauri decorations and add a 32 px React titlebar. Let `tauri-plugin-frame` add the caption controls and provide the Windows-native Snap hit target; preserve the current Rust close-request behavior.

**Tech Stack:** Tauri 2, React, TypeScript, Rust, existing CSS.

**Spec:** `docs/superpowers/specs/2026-10-27-custom-window-controls-design.md`

## Global Constraints

- Keep Only Monitor local-only and do not add shell or network capability.
- Keep a 450 px minimum width so the window can fit common Snap zones; preserve close-to-tray behavior.
- Keep the plugin's native hit-test dimensions synchronized with titlebar CSS.

## Review Focus

- Plugin overlay intercepts maximize hover/click: verify native Snap menu and maximize action.
- Frameless window geometry can regress: verify resize borders, drag, minimize, restore and close.
- Close must still route through the existing Rust close-request handler.

### Task 1: Native frame and React titlebar

**Files:** `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/lib.rs`, `src/App.tsx`, `src/styles.css`.

- [x] Add the pinned Tauri frame plugin and register its Snap overlay on the main window.
- [x] Disable system decorations and grant only window permissions needed by the titlebar.
- [x] Add the 32 px branded strip and style plugin-provided minimize, maximize/restore and close controls.
- [x] Match 46 px caption-button geometry and account for controls in the plugin overlay.
- [x] Build and launch the Windows Release app; verify the branded titlebar, one visible button set, and maximize/restore.
- [x] Verify minimize and restore, plus close-to-tray and reopening while the monitor process stays running.
- [x] Verify the Windows Snap layout flyout opens with Win+Z, the native Snap overlay pane is present, and Win+Left snaps the window while the dashboard adapts to the narrower width.
- [ ] Verify hovering the custom maximize button directly opens Snap, plus direct window dragging and edge resizing.
