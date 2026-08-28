# Performance Architecture & Troubleshooting

This document outlines the performance challenges encountered when building a scrolling tiling window manager on Windows 11 and how they were resolved.

## The Problem: FPS Drops During Smooth Scrolling
When users opened heavy applications like Google Chrome, Microsoft Edge, or Firefox, the window manager's smooth scrolling animation FPS would plummet. The scrolling would stutter, and the browser itself would become unresponsive during movement.

### Root Cause
The window manager used a high-precision animation loop running at the hardware refresh rate (e.g., 144Hz). During each frame of the scroll animation, it calculated new window positions and updated them using `DeferWindowPos` and `EndDeferWindowPos`. 

However, `EndDeferWindowPos` is a **synchronous, blocking API**. Even when combined with flags like `SWP_NOSENDCHANGING`, Windows waits for the target applications to process the `WM_WINDOWPOSCHANGING` and `WM_WINDOWPOSCHANGED` messages. Because modern browsers have heavy, multi-threaded GPU compositor pipelines, forcing them to recalculate their swapchains and layouts 144 times a second overloaded their message pumps. Consequently, the browser lagged, which in turn blocked the window manager's animation loop, destroying the smooth scrolling frame rate.

Additionally, when complex applications like browsers start up, they emit dozens of `EVENT_OBJECT_SHOW` and `EVENT_OBJECT_CREATE` WinEvents for their internal sub-windows, tabs, and tooltips. Processing a full layout pass for each of these events caused further stalling.

## The Solution

### 1. `SWP_ASYNCWINDOWPOS` for Non-Blocking Layout
To achieve hardware-rate animation loops (144Hz+) while continuously repositioning windows, the layout engine was restructured:

- **Resizing**: We still use `DeferWindowPos` so that all windows resize and snap together atomically without tearing.
- **Scrolling (Translation Only)**: We bypass `DeferWindowPos` completely. Instead, we call `SetWindowPos` directly on each window, injecting the **`SWP_ASYNCWINDOWPOS`** flag.

```rust
let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSENDCHANGING | SWP_ASYNCWINDOWPOS | SWP_NOSIZE;

SetWindowPos(hwnd, Some(HWND::default()), x, y, w, h, flags);
```

`SWP_ASYNCWINDOWPOS` guarantees that the system posts the sizing request to the target window's thread asynchronously. It returns immediately. This fully decouples the window manager's metronome thread from the rendering bottlenecks of the applications being moved.

### 2. Event Coalescing via Dirty Flags
Instead of triggering an immediate layout recalcalculation inside the WinEvent hook thread (which handles dozens of browser startup events instantly), the event hooks now simply set atomic "dirty flags" (`LAYOUT_DIRTY` and `LAYOUT_SIZE_CHANGED`) and signal a condition variable.

The 144Hz animation loop reads this dirty flag. This effectively coalesces bursts of 100+ WinEvents into a single layout pass synchronized perfectly with the next screen refresh.

### 3. Non-Manageable HWND Caching
Applications generate hundreds of events for hidden or unmanageable components (e.g., browser tooltip shadows, internal compositing surfaces). Evaluating `is_manageable` requires multiple costly `user32` / `dwmapi` calls (`GetWindowLongW`, `DwmGetWindowAttribute`, etc.).

A `REJECTED_CACHE` (`HashSet<isize>`) was implemented to cache HWNDs that fail the manageability checks. Subsequent events for these same irrelevant windows exit the hook instantly in `O(1)` time without stalling the event loop. The cache is evicted automatically upon receiving `EVENT_OBJECT_DESTROY`.

## The Problem: Windows "Sticking" to the Screen Edges
An early optimization attempted to skip `SetWindowPos` calls for windows that fell outside the physical screen boundaries during a scroll, to save rendering cycles:

```rust
// Buggy implementation
if win_screen_right < screen_x || win_screen_left > screen_right {
    continue; // Skip SetWindowPos
}
```

### Root Cause
Because `SetWindowPos` was skipped *before* the window was properly parked in its final off-screen location, the application was physically abandoned wherever its last valid `SetWindowPos` was called (typically resting exactly on the left or right edge of the monitor).

### The Fix
The off-screen skip optimization was removed. Because the system now uses `SWP_ASYNCWINDOWPOS`, updating off-screen windows is extremely cheap and non-blocking. Letting the math naturally push the application windows off-screen to `x: -3000` guarantees they stay aligned in the virtual scrolling strip without getting visually marooned on the user's screen edges.