use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use std::sync::mpsc::{self, Sender};
use tauri::{AppHandle, Emitter}; // Using Emitter for emit/emit_all
use windows::Win32::Foundation::{HWND, RECT, E_ACCESSDENIED};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsWindowVisible, SetWindowPos,
    SWP_ASYNCWINDOWPOS, SWP_NOZORDER, SWP_NOACTIVATE,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DwmRegisterThumbnail, DwmUpdateThumbnailProperties, DwmUnregisterThumbnail,
    DWMWA_CLOAK, DWM_THUMBNAIL_PROPERTIES,
};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};

// Helper to convert isize to HWND
#[inline]
fn isize_to_hwnd(ptr: isize) -> HWND {
    HWND(ptr as *mut _)
}

// -----------------------------------------------------------------------------
// System 1: The Asynchronous Shadow DOM (The Read Pipeline)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WindowState {
    pub hwnd: isize,
    pub rect: RECT,
    pub is_visible: bool,
}

pub struct WindowStateCache {
    pub states: Arc<RwLock<HashMap<isize, WindowState>>>,
}

impl WindowStateCache {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn start_watcher_thread(&self, managed_hwnds: Arc<RwLock<Vec<isize>>>) {
        let states_clone = self.states.clone();
        
        thread::Builder::new()
            .name("WatcherThread".to_string())
            .spawn(move || {
                loop {
                    // 60Hz loop (~16.6ms)
                    thread::sleep(Duration::from_millis(16));
                    
                    let hwnds = {
                        let lock = managed_hwnds.read().unwrap();
                        lock.clone()
                    };

                    let mut new_states = HashMap::new();
                    for hwnd_isize in hwnds {
                        let hwnd = isize_to_hwnd(hwnd_isize);
                        let mut rect = RECT::default();
                        let mut is_visible;
                        
                        unsafe {
                            // Non-blocking getters. If a window is hung, these might block briefly, 
                            // but doing this in a background thread protects the Tauri main thread.
                            let rect_result = GetWindowRect(hwnd, &mut rect);
                            match rect_result {
                                Ok(_) => {},
                                Err(e) if e.code() == E_ACCESSDENIED => {
                                    // Handle access denied gracefully
                                },
                                Err(_) => continue,
                            }
                            
                            is_visible = IsWindowVisible(hwnd).into();
                        }
                        
                        new_states.insert(hwnd_isize, WindowState {
                            hwnd: hwnd_isize,
                            rect,
                            is_visible,
                        });
                    }

                    if let Ok(mut lock) = states_clone.write() {
                        *lock = new_states;
                    }
                }
            })
            .expect("Failed to spawn WatcherThread");
    }
}

// -----------------------------------------------------------------------------
// System 2: The Dual Movement Pipelines (The Write Pipeline)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineType {
    StandardApp, // Pipeline A: DWM Thumbnails
    HeavyGame,   // Pipeline B: 1x1 Region Clipping
}

#[derive(Debug, Clone)]
pub enum MovementMsg {
    Init {
        hwnd: isize,
        pipeline: PipelineType,
        tauri_host_hwnd: isize,
    },
    Update {
        hwnd: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    Commit {
        hwnd: isize,
        final_x: i32,
        final_y: i32,
        final_width: i32,
        final_height: i32,
    },
}

#[derive(Clone, serde::Serialize)]
struct ProxyCardPayload {
    hwnd: isize,
    action: String, // "show", "update", "hide"
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

pub struct MovementWorker {
    sender: Sender<MovementMsg>,
}

impl MovementWorker {
    pub fn new(app_handle: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel::<MovementMsg>();

        thread::Builder::new()
            .name("MovementWorkerThread".to_string())
            .spawn(move || {
                let mut active_thumbnails: HashMap<isize, isize> = HashMap::new(); // hwnd -> hthumbnail ptr
                let mut active_pipelines: HashMap<isize, PipelineType> = HashMap::new();

                loop {
                    let mut latest_updates: HashMap<isize, MovementMsg> = HashMap::new();
                    let mut commits: Vec<MovementMsg> = Vec::new();
                    let mut inits: Vec<MovementMsg> = Vec::new();

                    // Block until we get at least one message
                    let msg = match rx.recv() {
                        Ok(m) => m,
                        Err(_) => break, // Channel closed (app shutting down)
                    };

                    let mut process_msg = |msg: MovementMsg| {
                        match msg {
                            MovementMsg::Init { hwnd, .. } => {
                                inits.push(msg);
                            },
                            MovementMsg::Update { hwnd, .. } => {
                                latest_updates.insert(hwnd, msg);
                            },
                            MovementMsg::Commit { hwnd, .. } => {
                                commits.push(msg);
                                latest_updates.remove(&hwnd); // Commit overrides pending update
                            },
                        }
                    };
                    
                    process_msg(msg);

                    // Drain the channel queue completely to avoid backlog and take only the most recent updates
                    while let Ok(msg) = rx.try_recv() {
                        process_msg(msg);
                    }

                    // --- Phase 1: Inits ---
                    for init in inits {
                        if let MovementMsg::Init { hwnd, pipeline, tauri_host_hwnd } = init {
                            active_pipelines.insert(hwnd, pipeline);
                            let hwnd_type = isize_to_hwnd(hwnd);
                            let host_hwnd = isize_to_hwnd(tauri_host_hwnd);

                            match pipeline {
                                PipelineType::StandardApp => {
                                    unsafe {
                                        // Cloak the window so it visually disappears but renders in background
                                        let cloak_val: i32 = 1;
                                        let _ = DwmSetWindowAttribute(
                                            hwnd_type,
                                            DWMWA_CLOAK,
                                            &cloak_val as *const _ as *const std::ffi::c_void,
                                            std::mem::size_of::<i32>() as u32,
                                        );

                                        match DwmRegisterThumbnail(host_hwnd, hwnd_type) {
                                            Ok(h_thumb) => {
                                                active_thumbnails.insert(hwnd, h_thumb);
                                            },
                                            Err(e) if e.code() == E_ACCESSDENIED => {
                                                // Handle restricted UIPI/admin windows
                                            },
                                            Err(_) => {}
                                        }
                                    }
                                },
                                PipelineType::HeavyGame => {
                                    unsafe {
                                        // 1x1 clip to safely freeze visual output without dropping focus or crashing MPO
                                        let empty_rgn = CreateRectRgn(0, 0, 0, 0);
                                        let _ = SetWindowRgn(hwnd_type, Some(empty_rgn), false);
                                    }
                                    // Proxy Card for 144Hz frontend tracking
                                    let _ = app_handle.emit("proxy-card", ProxyCardPayload {
                                        hwnd, action: "show".to_string(), x: 0, y: 0, width: 0, height: 0
                                    });
                                }
                            }
                        }
                    }

                    // --- Phase 2: Updates ---
                    for (hwnd, update) in latest_updates {
                        if let MovementMsg::Update { hwnd, x, y, width, height } = update {
                            if let Some(pipeline) = active_pipelines.get(&hwnd) {
                                match pipeline {
                                    PipelineType::StandardApp => {
                                        if let Some(&h_thumb_ptr) = active_thumbnails.get(&hwnd) {
                                            let mut props = DWM_THUMBNAIL_PROPERTIES::default();
                                            // 1: RECTDEST, 4: OPACITY, 8: VISIBLE, 16: SOURCECLIENTAREAONLY
                                            props.dwFlags = 1 | 4 | 8 | 16;
                                            props.fVisible = true.into();
                                            props.opacity = 255;
                                            props.fSourceClientAreaOnly = true.into();
                                            props.rcDestination = RECT {
                                                left: x,
                                                top: y,
                                                right: x + width,
                                                bottom: y + height,
                                            };
                                            
                                            unsafe {
                                                let _ = DwmUpdateThumbnailProperties(h_thumb_ptr, &props);
                                            }
                                        }
                                    },
                                    PipelineType::HeavyGame => {
                                        let _ = app_handle.emit("proxy-card", ProxyCardPayload {
                                            hwnd, action: "update".to_string(), x, y, width, height
                                        });
                                    }
                                }
                            }
                        }
                    }

                    // --- Phase 3: Commits ---
                    for commit in commits {
                        if let MovementMsg::Commit { hwnd, final_x, final_y, final_width, final_height } = commit {
                            let hwnd_type = isize_to_hwnd(hwnd);
                            
                            unsafe {
                                // Real async commit. The main layout engine does not block here.
                                let _ = SetWindowPos(
                                    hwnd_type,
                                    Some(HWND(0 as _)),
                                    final_x,
                                    final_y,
                                    final_width,
                                    final_height,
                                    SWP_ASYNCWINDOWPOS | SWP_NOZORDER | SWP_NOACTIVATE,
                                );
                            }

                            if let Some(pipeline) = active_pipelines.remove(&hwnd) {
                                match pipeline {
                                    PipelineType::StandardApp => {
                                        unsafe {
                                            // Uncloak
                                            let cloak_val: i32 = 0;
                                            let _ = DwmSetWindowAttribute(
                                                hwnd_type,
                                                DWMWA_CLOAK,
                                                &cloak_val as *const _ as *const std::ffi::c_void,
                                                std::mem::size_of::<i32>() as u32,
                                            );

                                            if let Some(h_thumb_ptr) = active_thumbnails.remove(&hwnd) {
                                                let _ = DwmUnregisterThumbnail(h_thumb_ptr);
                                            }
                                        }
                                    },
                                    PipelineType::HeavyGame => {
                                        unsafe {
                                            // Remove clip region
                                            let _ = SetWindowRgn(hwnd_type, None, true);
                                        }
                                        let _ = app_handle.emit("proxy-card", ProxyCardPayload {
                                            hwnd, action: "hide".to_string(), x: 0, y: 0, width: 0, height: 0
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .expect("Failed to spawn MovementWorkerThread");

        Self { sender: tx }
    }

    pub fn send(&self, msg: MovementMsg) {
        // Non-blocking send
        let _ = self.sender.send(msg);
    }
}

// -----------------------------------------------------------------------------
// System 3: Global State & Coordinator
// -----------------------------------------------------------------------------

pub struct WmMovementCoordinator {
    cache: WindowStateCache,
    worker: MovementWorker,
    tauri_host_hwnd: isize,
    managed_hwnds: Arc<RwLock<Vec<isize>>>,
}

impl WmMovementCoordinator {
    pub fn new(app_handle: AppHandle, tauri_host_hwnd: isize) -> Self {
        let cache = WindowStateCache::new();
        let managed_hwnds = Arc::new(RwLock::new(Vec::new()));
        
        cache.start_watcher_thread(managed_hwnds.clone());
        let worker = MovementWorker::new(app_handle);

        Self {
            cache,
            worker,
            tauri_host_hwnd,
            managed_hwnds,
        }
    }

    pub fn add_managed_window(&self, hwnd: isize) {
        if let Ok(mut hwnds) = self.managed_hwnds.write() {
            if !hwnds.contains(&hwnd) {
                hwnds.push(hwnd);
            }
        }
    }

    pub fn remove_managed_window(&self, hwnd: isize) {
        if let Ok(mut hwnds) = self.managed_hwnds.write() {
            hwnds.retain(|&h| h != hwnd);
        }
        if let Ok(mut states) = self.cache.states.write() {
            states.remove(&hwnd);
        }
    }

    // Heuristic categorization for the dual pipelines
    fn categorize_window(&self, _hwnd: isize) -> PipelineType {
        // TODO: Query window attributes or exe name to detect heavy MPO games (e.g. Valorant)
        // Defaulting to StandardApp for normal applications.
        PipelineType::StandardApp
    }

    /// Triggered globally on a scroll/pan start
    pub fn begin_scroll(&self) {
        let states = {
            if let Ok(lock) = self.cache.states.read() {
                lock.clone()
            } else {
                return;
            }
        };

        for (hwnd, state) in states {
            if state.is_visible {
                let pipeline = self.categorize_window(hwnd);
                self.worker.send(MovementMsg::Init {
                    hwnd,
                    pipeline,
                    tauri_host_hwnd: self.tauri_host_hwnd,
                });
            }
        }
    }

    /// Called rapidly (e.g., at 144Hz) by Tauri main thread during a scroll
    pub fn stream_scroll_update(&self, hwnd: isize, x: i32, y: i32, width: i32, height: i32) {
        self.worker.send(MovementMsg::Update { hwnd, x, y, width, height });
    }

    /// Triggered globally on scroll end to lock in final positions
    pub fn end_scroll(&self, hwnd: isize, final_x: i32, final_y: i32, final_width: i32, final_height: i32) {
        self.worker.send(MovementMsg::Commit { 
            hwnd, 
            final_x, 
            final_y, 
            final_width, 
            final_height 
        });
    }
}
