# Summary of Window Manager Scrolling & Performance Fixes

## Overview
This document summarizes the investigation, architectural fixes, and optimizations applied to eliminate window swaying, maintain exact padding/gaps, and achieve smooth 144Hz scrolling on Windows 11.

---

## 1. Root Cause Analysis

### A. The Chromium "Swaying / Shaking" Effect
- **Mechanism**: Chromium-based applications (Google Chrome, Microsoft Edge, VS Code, Discord, Slack) use internal DirectComposition GPU swapchains (`IDCompositionVisual`).
- **The Issue**: Moving windows sequentially via individual `SetWindowPos` calls caused multi-window vertical tearing and out-of-sync composition. Legacy Windows `BitBlt` predictive bitmap copying also attempted to copy old application pixels before new hardware-accelerated frames finished rendering.
- **Visual Artifacts**: Windows appeared to sway, shake, or lag behind during continuous motion.

### B. Failed Visual Proxy / Thumbnail Approach
- **Mechanism**: An attempt was made to cloak real windows and render DWM thumbnails (`DwmRegisterThumbnail`) on a transparent overlay during scrolling.
- **Failure Modes**:
  1. `DWMWA_CLOAK` returns `E_ACCESSDENIED` when called on third-party application windows without system privileges, leaving the real windows stationary on screen.
  2. The transparent overlay rendered moving thumbnails over stationary windows, causing visible stationary "ghost" windows in between gaps.
  3. Cloaking fired `EVENT_OBJECT_HIDE` / `EVENT_OBJECT_SHOW` WinEvents, creating an infinite window destruction and re-addition loop that destroyed performance.
  4. Thumbnail destination scaling distorted invisible drop-shadow borders, shrinking gaps from 16px to 2px.

---

## 2. Implemented Architecture & Fixes

### 1. Atomic Multi-Window Batching (`DeferWindowPos`)
All window repositioning (both resizing and translation) is batched atomically into a single Desktop Window Manager (DWM) frame pass:
```rust
let window_count = self.windows.len() as i32;
let mut hdwp = BeginDeferWindowPos(window_count)?;

for w in &self.windows {
    hdwp = DeferWindowPos(hdwp, hwnd, HWND::default(), target_x, target_y, target_w, target_h, flags)?;
}

EndDeferWindowPos(hdwp);
```
- Eliminates multi-window vertical tearing and ensures all tiled columns move synchronously in lockstep.

### 2. Precise `SWP_*` Flags & `SWP_NOCOPYBITS`
Applied optimized flag combination for scrolling:
```rust
let flags = SWP_NOZORDER 
    | SWP_NOACTIVATE 
    | SWP_NOSENDCHANGING 
    | SWP_ASYNCWINDOWPOS 
    | SWP_NOSIZE 
    | SWP_NOCOPYBITS;
```
- **`SWP_NOCOPYBITS`**: Disables legacy GDI BitBlt pixel copying artifacts.
- **`SWP_NOSENDCHANGING`**: Skips `WM_WINDOWPOSCHANGING` message overhead.
- **`SWP_ASYNCWINDOWPOS`**: Prevents the high-priority animation loop from blocking on application message queues.
- **`SWP_NOACTIVATE` & `SWP_NOZORDER`**: Prevents focus stealing and Z-order thrashing.

### 3. Native Hardware VBlank Synchronization (`DwmFlush`)
- Directly after `EndDeferWindowPos`, `DwmFlush()` is invoked to force DWM to commit the atomic batch and block until the monitor's VBlank refresh (e.g. 6.94ms at 144Hz).
- Removed redundant secondary timer sleeps that previously caused 72 FPS frame drops and micro-stutters.

### 4. Critically Damped Spring Physics
Replaced linear/exponential smoothing with an analytical $2^{\text{nd}}$-order critically damped oscillator ($\zeta = 1.0$, $\omega = 35.0$):
$$\ddot{y} + 2\omega \dot{y} + \omega^2 y = 0$$
- **Continuous Velocity**: Preserves momentum when new scroll wheel inputs arrive mid-flight without velocity jumps.
- **Snappy Settling**: Smooth deceleration that cleanly settles in ~120ms without long dragging tails or overshoot.

### 5. Exact Border Margin & Gap Geometry
Window bounds are calculated strictly using `DWMWA_EXTENDED_FRAME_BOUNDS` and `WINDOWINFO`:
- Real window dimensions account for Windows 11 invisible 7px drop shadow borders (`border_left`, `border_right`, `border_top`, `border_bottom`).
- Visible gaps between adjacent columns stay exactly at `self.config.gap` (e.g. 16px) throughout all scrolling and resizing operations.

---

## 3. Verification
- **Unit Tests**: `cargo test` passing.
- **Compilation**: Clean compilation with zero warnings.
- **Production Build**: Full Tauri MSI and NSIS installer release bundles (`npm run tauri build`) built and verified.
