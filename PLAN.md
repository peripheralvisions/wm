# Implementation Plan: Niri-style Window Manager for Windows 11

## Context
Build a modern tiling window manager for Windows 11 replicating `niri-wm` (scrollable-tiling Wayland compositor). Built with **Rust** (low-level Windows API) and **Tauri + React + Tailwind + shadcn/ui** (Configuration GUI).

User requirements:
- Mouse-oriented control.
- Alt+Scroll for navigation.
- Taskbar GUI (System Tray) for config.
- Manual mouse resizing of individual windows.
- Full workspace support.

## 1. Niri-WM Paradigm & Core Mechanics
**Scrollable Tiling:**
- **Infinite Strip:** Windows arranged in columns on infinite horizontal strip.
- **No Auto-Resizing:** New window appends column to right, existing windows do not shrink.
- **Scrolling Navigation:** Pan left/right across strip.
- **Vertical Workspaces:** Dynamic vertical workspaces.

*Benefit:* Avoids frequent resizing operations, eliminating standard "gray area" artifacts in traditional Windows tiling WMs.

## 2. Proposed Architecture (Rust + Tauri)

### A. Core Engine (Rust `windows-rs`)
- **Event Hooking:** `SetWinEventHook` (`EVENT_OBJECT_CREATE`, `EVENT_OBJECT_DESTROY`, `EVENT_SYSTEM_MOVESIZEEND`) for app lifecycle and manual resizes.
- **Input Hooks:** `WH_MOUSE_LL` and `WH_KEYBOARD_LL` capture **Alt + Scroll** to pan view.
- **Layout Manager:** State machine for horizontal strip. Captures manual resizes, recalculates boundaries, repositions neighbors.
- **Workspace Support:** Custom internal logic. Toggle window visibility (`ShowWindow`) to switch workspaces (bypassing fragile Windows 11 Virtual Desktop COM APIs).
- **Window Filtering:** Ignore `WS_POPUP`, `WS_EX_TOOLWINDOW`, and specific classes (`Progman`) so tooltips/dialogs don't break layout.

### B. Presentation Layer (Tauri)
- **System Tray:** App runs in background with Taskbar icon.
- **Settings GUI:** Click tray icon -> open full GUI (React/shadcn). Configure default sizes, gaps, scroll speeds.

## 3. Mitigating Windows Resizing Artifacts & Issues

1. **Gray Area Resizing Lag:**
   - *Niri Advantage:* Moving windows (panning) doesn't trigger redraws.
   - *Mouse-Driven Resizing:* OS handles render during drag. WM observes `MOVESIZEEND` and shifts neighbors.
   - *DWM Cloaking:* For programmatic resizes, use `DwmSetWindowAttribute` (`DWMWA_CLOAK`). Cloak -> resize -> wait -> uncloak.
   - *Batch Updates:* Use `DeferWindowPos` to batch position updates atomically.
2. **Invisible Borders & Gaps:**
   - Win11 adds invisible grab borders. Use `DwmGetWindowAttribute` with `DWMWA_EXTENDED_FRAME_BOUNDS` for true visual bounds to achieve flush tiling.
3. **Focus Stealing:**
   - Windows blocks focus changes during panning. Implement `AllowSetForegroundWindow` bypass or dummy `Alt` keystroke injection.

## Next Steps for New Session
1. Scaffold Rust backend in `src-tauri/src/`.
2. Add `windows-rs` dependencies.
3. Implement `SetWinEventHook` for window lifecycle tracking.
4. Implement `WH_MOUSE_LL` hook for Alt+Scroll.