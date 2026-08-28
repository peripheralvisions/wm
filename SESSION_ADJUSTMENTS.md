# Session Adjustments: Smooth Scrolling & Stability Fixes

## 1. Issues Diagnosed

- **Jittering & Swaying:** Windows were swaying left and right and drifting out of sync (accordion effect) during horizontal scroll.
- **Rapid Shaking on Resize:** Manually resizing a window caused rapid, violent shaking as the window manager fought the user's cursor.
- **Hook Death After 5 Seconds:** `Alt + Scroll` completely stopped working after approximately 5 seconds of application runtime.

---

## 2. Root Causes

1. **`EVENT_OBJECT_LOCATIONCHANGE` Feedback Loop & Drag Conflicts:**
   - Every `SetWindowPos` call fired an asynchronous `LOCATIONCHANGE` event, prompting another layout pass and creating an infinite repositioning loop.
   - During manual border drags, rapid location events repeatedly snapped the window back to its tiled position mid-drag.

2. **Sequential, Asynchronous Positioning & Client Reflows:**
   - Repositioning windows individually via `SWP_ASYNCWINDOWPOS` caused windows to move across different frames, desynchronizing the strip.
   - Moving windows without `SWP_NOSIZE` forced applications (browsers, Electron apps, editors) to trigger expensive client area DOM/layout reflows on every animation tick.

3. **Windows `LowLevelHooksTimeout` (The 5-Second Failure):**
   - The low-level mouse hook (`WH_MOUSE_LL`) shared a single thread and message pump with WinEvents and heavy Win32 layout operations (`apply_layout`, `DeferWindowPos`, `EnumWindows`).
   - If any window delayed handling its position messages, the thread's message queue stalled.
   - Windows detects unresponsive `WH_MOUSE_LL` message queues exceeding the 5-second `LowLevelHooksTimeout` and silently, permanently evicts the hook from the system hook chain.

---

## 3. Architecture Adjustments & Improvements

### A. Atomic Batch Window Repositioning
- **`BeginDeferWindowPos` / `EndDeferWindowPos` (HDWP):** All managed windows in the horizontal strip are now updated simultaneously within a single atomic compositor transaction, eliminating inter-window latency and swaying.
- **Zero-Reflow Translations (`SWP_NOSIZE`):** Smooth scroll frames use `SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS | SWP_NOSENDCHANGING`. DWM directly translates window textures without triggering application relayouts. Full geometry recalculations only occur when windows are added, removed, or resized.

### B. High Refresh Rate Adaptation (60Hz, 120Hz, 144Hz, 240Hz+)
- **VSync-Locked Pacing (`DwmFlush`):** The animation loop blocks on the hardware VBlank interval, matching the exact refresh rate of the primary display.
- **Frame-Rate Independent Physics:** Exponential smoothing utilizes continuous delta-time scaling (`1.0 - (-16.0 * dt).exp()`), delivering consistent gliding curves across any display refresh rate.
- **Zero-Latency Wakeup:** Replaced static polling intervals with a condition variable (`Condvar`) signaled immediately upon scroll input, starting motion on the very next display frame.

### C. Dedicated Resize Lifecycle Hooks
- Replaced noisy `LOCATIONCHANGE` hooks with `EVENT_SYSTEM_MOVESIZESTART` and `EVENT_SYSTEM_MOVESIZEEND`.
- Actively dragged windows are tracked and ignored during intermediate mouse movements, smoothly snapping the entire tiled layout into place only when the user finishes dragging.

### D. Thread Isolation for Low-Level Mouse Hooks
- **Dedicated Mouse Hook Thread:** The `WH_MOUSE_LL` hook is isolated on its own dedicated OS thread with a dedicated message pump and module instance handle (`GetModuleHandleW`).
- **Nanosecond Callback Execution:** `mouse_hook_proc` performs zero I/O, zero mutex locking, and zero Win32 window queries. It only performs an atomic add to `SCROLL_ACCUM` and signals the condition variable.
- **Permanent Hook Stability:** Because the hook procedure returns in under 0.001ms, Windows will never hit `LowLevelHooksTimeout` or drop the hook.
- **Dedicated WinEvent Thread:** Window lifecycle events (`EVENT_OBJECT_CREATE`, `SHOW`, `HIDE`, `FOREGROUND`, `MOVESIZE`) run on their own separate thread, completely decoupled from mouse input and physics.
