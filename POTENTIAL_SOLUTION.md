To fix the swaying effect and achieve perfectly smooth 144Hz scrolling in your Rust window manager, here is the summarized solution:

1. Isolate the Animation Loop from Tauri
The Tauri Inter-Process Communication (IPC) bridge introduces unpredictable latency that ruins 144Hz frame pacing. Keep Tauri for your settings and UI, but move all input hooking, physics calculations, and window positioning entirely into a dedicated, high-priority native Rust thread.

2. Batch Window Movements
Do not use SetWindowPos to move multiple windows sequentially, as this causes vertical tearing. Batch all your window coordinate updates together using the BeginDeferWindowPos, DeferWindowPos, and EndDeferWindowPos APIs. This ensures the Desktop Window Manager (DWM) commits all spatial changes in a single atomic frame.  

3. Apply the SWP_NOCOPYBITS Flag (The Core Fix)
The swaying effect is caused by a legacy Windows feature where the OS attempts to predictively copy (BitBlt) the old application pixels to the new window location before Chromium finishes rendering its new hardware-accelerated frame. You can completely disable this behavior by passing the SWP_NOCOPYBITS flag into your DeferWindowPos calls. For the best results, combine it like this: SWP_NOCOPYBITS | SWP_NOSENDCHANGING | SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE.  

4. Synchronize to Hardware VBlank
Standard sleep commands (like std::thread::sleep) rely on the Windows OS timer, which operates at a 15.6ms resolution and will cap your animation at around 64 FPS. To achieve true 144Hz synchronization, you have two native options:  

The DXGI Route: After committing your window positions, call DwmFlush() to force the OS compositor to update, and then use IDXGIOutput::WaitForVBlank to block your Rust thread until the exact microsecond the monitor hardware refreshes.  

The High-Resolution Timer Route: Use the windows-sys Rust crate to invoke CreateWaitableTimerExW utilizing the CREATE_WAITABLE_TIMER_HIGH_RESOLUTION flag. This allows you to schedule your loop to wake up precisely every 6.94 milliseconds without relying on VSync blocks.  

5. Use Critically Damped Spring Physics
To make the movement feel organic and responsive to sudden scroll changes at 144Hz, calculate the intermediate window coordinates using a critically damped spring equation. You can use existing Rust crates like damped-springs to integrate this efficiently. This prevents the motion from overshooting the target while maintaining fluid deceleration.