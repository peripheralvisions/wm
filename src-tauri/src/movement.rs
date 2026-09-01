use std::ffi::c_void;
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::Instant;
use windows::Win32::Foundation::{HWND, RECT, TRUE, FALSE};
use windows::Win32::Graphics::Dwm::{
    DwmRegisterThumbnail, DwmSetWindowAttribute, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties, DWMWA_CLOAK, DWM_THUMBNAIL_PROPERTIES,
    DWM_TNP_RECTDEST, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, HWND_TOP, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};

pub enum MovementMsg {
    Update { x: i32, y: i32 },
    Stop,
}

/// The 3-phase state machine using Multithreaded DWM architecture.
pub enum WmMovementState {
    /// Phase 1: The window is static.
    Idle,

    /// Phase 2: The window is moving rapidly.
    Translating {
        current_x: i32,
        current_y: i32,
        thumbnail_handle: isize,
        tx: Sender<MovementMsg>,
    },

    /// Phase 3: The movement has stopped; syncing logical Win32 position to visual position.
    Snapping {
        final_x: i32,
        final_y: i32,
    },
}

pub struct ManagedWindow {
    pub hwnd: HWND,
    pub state: WmMovementState,
}

impl ManagedWindow {
    pub fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            state: WmMovementState::Idle,
        }
    }

    /// Handles the 3-phase pipeline.
    /// Call this inside your movement loop or event handler.
    pub unsafe fn update_movement(
        &mut self,
        host_overlay_hwnd: HWND,
        new_x: i32,
        new_y: i32,
        width: i32,
        height: i32,
    ) {
        match &mut self.state {
            WmMovementState::Idle => {
                // PHASE A: Initialization (When movement begins)
                println!("Phase A: Initiating Cloak & Threaded DWM Setup");

                // 1. Cloak the physical window to prevent ghosting
                let cloak_val: i32 = 1;
                DwmSetWindowAttribute(
                    self.hwnd,
                    DWMWA_CLOAK,
                    &cloak_val as *const _ as *const c_void,
                    std::mem::size_of::<i32>() as u32,
                )
                .expect("Phase A: DwmSetWindowAttribute (CLOAK=1) failed");

                // 2. Map target HWND to the host overlay via DwmRegisterThumbnail
                let mut thumb_handle: isize = 0;
                DwmRegisterThumbnail(host_overlay_hwnd, self.hwnd, &mut thumb_handle)
                    .expect("Phase A: DwmRegisterThumbnail failed");

                // Send initial thumbnail position
                let thumb_props = DWM_THUMBNAIL_PROPERTIES {
                    dwFlags: DWM_TNP_VISIBLE | DWM_TNP_RECTDEST | DWM_TNP_SOURCECLIENTAREAONLY,
                    fVisible: TRUE,
                    fSourceClientAreaOnly: FALSE,
                    rcDestination: RECT {
                        left: new_x,
                        top: new_y,
                        right: new_x + width,
                        bottom: new_y + height,
                    },
                    opacity: 255,
                    ..Default::default()
                };
                DwmUpdateThumbnailProperties(thumb_handle, &thumb_props)
                    .expect("Phase A: DwmUpdateThumbnailProperties failed");

                // 3. Spawn the dedicated background thread for DWM updates
                let (tx, rx) = channel::<MovementMsg>();

                thread::spawn(move || {
                    while let Ok(msg) = rx.recv() {
                        match msg {
                            MovementMsg::Update { x, y } => {
                                // Drain the channel to ensure we only process the very latest coordinate
                                // if the main thread is generating updates faster than DWM can process them.
                                let mut latest_x = x;
                                let mut latest_y = y;
                                while let Ok(m) = rx.try_recv() {
                                    if let MovementMsg::Update { x: next_x, y: next_y } = m {
                                        latest_x = next_x;
                                        latest_y = next_y;
                                    } else {
                                        return; // Got a Stop message while draining
                                    }
                                }

                                let start = Instant::now();
                                let props = DWM_THUMBNAIL_PROPERTIES {
                                    dwFlags: DWM_TNP_RECTDEST,
                                    rcDestination: RECT {
                                        left: latest_x,
                                        top: latest_y,
                                        right: latest_x + width,
                                        bottom: latest_y + height,
                                    },
                                    ..Default::default()
                                };

                                // Execute the slow IPC blocking call on this thread
                                if let Err(e) = unsafe { DwmUpdateThumbnailProperties(thumb_handle, &props) } {
                                    eprintln!("Background Thread: DwmUpdateThumbnailProperties failed: {:?}", e);
                                }

                                let elapsed = start.elapsed();
                                if elapsed.as_millis() > 1 {
                                    println!("Background Thread: DWM update took {:?}", elapsed);
                                }
                            }
                            MovementMsg::Stop => {
                                break;
                            }
                        }
                    }
                });

                // Transition to Translating
                self.state = WmMovementState::Translating {
                    current_x: new_x,
                    current_y: new_y,
                    thumbnail_handle: thumb_handle,
                    tx,
                };
            }

            WmMovementState::Translating {
                current_x,
                current_y,
                tx,
                ..
            } => {
                // PHASE B: The 144Hz Render Loop (During movement)
                // We do NOT block on Win32 IPC here. We just push the new coordinate to the thread.
                *current_x = new_x;
                *current_y = new_y;

                if let Err(e) = tx.send(MovementMsg::Update { x: new_x, y: new_y }) {
                    eprintln!("Phase B: Failed to send update to DWM background thread: {:?}", e);
                }
            }

            WmMovementState::Snapping { final_x, final_y } => {
                // PHASE C: The Commit & Snap (When movement ends)
                println!("Phase C: Snapping & Uncloaking");

                // 1. Single silent Win32 update to logical position
                SetWindowPos(
                    self.hwnd,
                    HWND_TOP,
                    *final_x,
                    *final_y,
                    0, 0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
                )
                .expect("Phase C: SetWindowPos failed");

                // 2. Remove the DWM Cloak
                let cloak_val: i32 = 0;
                DwmSetWindowAttribute(
                    self.hwnd,
                    DWMWA_CLOAK,
                    &cloak_val as *const _ as *const c_void,
                    std::mem::size_of::<i32>() as u32,
                )
                .expect("Phase C: DwmSetWindowAttribute (CLOAK=0) failed");

                // The background thread exits automatically when `tx` is dropped (or we explicitly send Stop)
                // But `stop_movement()` handles unregistering the thumbnail and stopping the thread.

                self.state = WmMovementState::Idle;
            }
        }
    }

    /// Trigger the Snapping phase when the user stops scrolling/moving
    pub unsafe fn stop_movement(&mut self) {
        if let WmMovementState::Translating { current_x, current_y, thumbnail_handle, tx } = &self.state {
            // Signal the background thread to exit cleanly
            let _ = tx.send(MovementMsg::Stop);

            // Unregister the thumbnail mapping
            DwmUnregisterThumbnail(*thumbnail_handle).expect("stop_movement: DwmUnregisterThumbnail failed");

            self.state = WmMovementState::Snapping {
                final_x: *current_x,
                final_y: *current_y,
            };
        }
    }
}
