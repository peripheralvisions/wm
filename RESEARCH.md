# Research: Achieving Native Refresh Rate (144Hz+) for Window Movement on Windows 11

## The Core Problem
When a window manager attempts to move complex modern applications (especially Chromium-based browsers like Chrome, Edge, or Electron apps) using `SetWindowPos` or `DeferWindowPos` at a high refresh rate (e.g., 144Hz), it encounters two conflicting issues on Windows 11:

1. **Synchronous Blocking (`SetWindowPos` without `SWP_ASYNCWINDOWPOS`)**
   - **Mechanism:** The WM thread waits for the target application to process `WM_WINDOWPOSCHANGING` and `WM_WINDOWPOSCHANGED` in its UI thread.
   - **Result:** If the browser is under load (playing video, heavy DOM), it can take 10-20ms to process the message. This forcibly caps the window manager's loop to ~50-60 FPS, destroying the 144Hz smooth scroll.
   - **Multi-threaded Workaround:** Moving each window in a separate thread (as tried in commit `e5a428f`) keeps the WM loop at 144Hz, but the browser itself drops frames and updates at a stuttery 60Hz, making it look laggy compared to lightweight apps like Notepad.

2. **Asynchronous Tearing / Swaying (`SWP_ASYNCWINDOWPOS`)**
   - **Mechanism:** The WM posts the move request and immediately continues. DWM moves the window's non-client area (the border/shadow) instantly at 144Hz.
   - **Result:** Chromium uses its own internal DirectComposition swapchains. When it receives the delayed asynchronous `WM_WINDOWPOSCHANGED` message, it commits a new swapchain frame. Because the DWM border move and the Chromium content commit are out of sync, the content lags exactly 1 frame behind the border, creating a highly visible "jello" or "swaying" effect.

Flags like `SWP_NOREDRAW`, `SWP_NOCOPYBITS`, or `SWP_NOSENDCHANGING` mitigate some CPU overhead but do not prevent Chromium from internally reacting to the physical coordinate change and causing the 1-frame compositor desync.

---

## The Solution: Decoupled Visual Proxying

To make all windows visually operate at 144Hz without any internal application stutter or swaying, we must **completely decouple the visual movement from the application's message pump**. We cannot move the actual `HWND` during the animation.

### Implementation: DWM Thumbnails (`DwmRegisterThumbnail`)
This is the exact same API Windows uses for "Task View" (Win+Tab) to smoothly animate windows around the screen without lagging the applications.

**How it works:**
1. **Prepare:** Create a single, transparent, full-screen overlay window (`WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW`).
2. **On Scroll Start:** 
   - For every window visible in the viewport, call `DwmRegisterThumbnail` targeting the overlay window.
   - Set the thumbnail properties (`DWM_TNP_VISIBLE | DWM_TNP_RECTDESTINATION | DWM_TNP_SOURCECLIENTAREAONLY`).
   - Push the actual `HWND`s far off-screen (e.g., `x: -30000`) using a single `SetWindowPos` call, or apply `DWMWA_CLOAK` to hide them without moving them.
3. **During Scroll (144Hz Loop):**
   - Calculate the new positions.
   - Call `DwmUpdateThumbnailProperties` to update `rcDestination` for each thumbnail.
   - *Why this is perfect:* `DwmUpdateThumbnailProperties` is an ultra-fast IPC call directly to DWM. It sends **zero messages** to the browser. The browser continues rendering its internal content (videos, WebGL) at 144Hz, and DWM translates the composed texture on the screen at 144Hz. No swaying, no stuttering, no dropped frames.
4. **On Scroll End:**
   - Move the actual `HWND`s to their final resting positions via `DeferWindowPos`.
   - Call `DwmUnregisterThumbnail` and hide/clear the overlay window.

### Trade-offs
- **Hit-Testing:** Because the actual windows are hidden/off-screen during the animation, the user cannot click on buttons *inside* the browser while the scroll animation is actively playing. (This is generally acceptable for a fast workspace scroll/pan animation).
- **Z-Order Management:** The overlay window sits on top. If you have overlapping windows, you must register the thumbnails in the correct Z-order and update them accordingly. Since this is a tiling window manager without overlapping windows, this is trivial.

### Alternative (Not Recommended): Windows.Graphics.Capture
The modern UWP `Windows.Graphics.Capture` API can also capture window surfaces to a Direct3D swapchain, which can then be animated via DirectComposition. While powerful, it requires extensive DirectX setup and is overkill compared to the simplicity of `DwmRegisterThumbnail`.

---
## Conclusion
To perfectly resolve the Chromium swaying issue while maintaining true 144Hz native refresh rate rendering, the architecture must shift from **animating the HWNDs** to **animating DWM proxies (thumbnails)** during scroll operations.
