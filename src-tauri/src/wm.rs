use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED,
    DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS,
    MonitorFromWindow, GetMonitorInfoW, MONITORINFOEXW, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::Media::timeBeginPeriod;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU, VK_SHIFT, VK_LEFT, VK_RIGHT};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, CallNextHookEx, DeferWindowPos, DispatchMessageW, EndDeferWindowPos,
    EnumWindows, GetClassNameW, GetForegroundWindow, GetMessageW, GetParent, GetWindowInfo,
    GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible, SetWindowPos,
    SetWindowsHookExW, SystemParametersInfoW, TranslateMessage, UnhookWindowsHookEx,
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GWL_EXSTYLE, GWL_STYLE, MSG,
    MSLLHOOKSTRUCT, OBJID_WINDOW, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOCOPYBITS,
    SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SWP_ASYNCWINDOWPOS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WH_KEYBOARD_LL, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN, WH_MOUSE_LL, WINDOWINFO, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_MOUSEWHEEL,
    WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MINIMIZE, WS_POPUP, WS_THICKFRAME,
};


#[derive(Clone, Debug)]
pub enum WmAction {
    MoveWindow(i32),
    ResizeWindow(&'static str),
}

pub static ACTIONS: Lazy<Mutex<Vec<WmAction>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WmConfig {
    pub enabled: bool,
    pub gap: i32,
    pub scroll_speed: i32,
    pub snap_to_window: bool,
    pub column_sizing_mode: String,
    pub column_sizing_value: f32,
    pub smooth_scrolling: bool,
}

impl Default for WmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            gap: 16,
            scroll_speed: 100,
            snap_to_window: false,
            column_sizing_mode: "percent".to_string(),
            column_sizing_value: 50.0,
            smooth_scrolling: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SendHwnd(isize);
unsafe impl Send for SendHwnd {}
unsafe impl Sync for SendHwnd {}

impl SendHwnd {
    fn new(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }
    fn get(&self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ManagedWindow {
    hwnd: SendHwnd,
    width: i32,
    border_left: i32,
    border_top: i32,
    border_right: i32,
    border_bottom: i32,
}

struct WmState {
    windows: Vec<ManagedWindow>,
    current_offset_x: f32,
    target_offset_x: f32,
    screen_x: i32,
    screen_y: i32,
    screen_width: i32,
    screen_height: i32,
    config: WmConfig,
    resizing_hwnd: Option<SendHwnd>,
    last_rendered_int_offset: i32,
}

impl WmState {
    fn new() -> Self {
        Self {
            windows: Vec::new(),
            current_offset_x: 0.0,
            target_offset_x: 0.0,
            screen_x: 0,
            screen_y: 0,
            screen_width: 1920,
            screen_height: 1040,
            config: WmConfig::default(),
            resizing_hwnd: None,
            last_rendered_int_offset: i32::MIN,
        }
    }

    fn update_work_area(&mut self) {
        unsafe {
            let mut work_area = RECT::default();
            if SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work_area as *mut _ as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .is_ok()
            {
                self.screen_x = work_area.left;
                self.screen_y = work_area.top;
                self.screen_width = (work_area.right - work_area.left).max(640);
                self.screen_height = (work_area.bottom - work_area.top).max(480);
            }
        }
    }

    fn add_window_internal(&mut self, hwnd: HWND) -> bool {
        let shwnd = SendHwnd::new(hwnd);
        if self.windows.iter().any(|w| w.hwnd == shwnd) {
            return false;
        }

        let width = if self.config.column_sizing_mode == "percent" {
            (self.screen_width as f32 * (self.config.column_sizing_value / 100.0)).round() as i32
        } else {
            self.config.column_sizing_value as i32
        };

        let mut border_left = 0;
        let mut border_top = 0;
        let mut border_right = 0;
        let mut border_bottom = 0;

        unsafe {
            let mut bounds = RECT::default();
            if DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut bounds as *mut _ as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
            .is_ok()
            {
                let mut win_info = WINDOWINFO::default();
                win_info.cbSize = std::mem::size_of::<WINDOWINFO>() as u32;
                if GetWindowInfo(hwnd, &mut win_info).is_ok() {
                    border_left = bounds.left - win_info.rcWindow.left;
                    border_top = bounds.top - win_info.rcWindow.top;
                    border_right = win_info.rcWindow.right - bounds.right;
                    border_bottom = win_info.rcWindow.bottom - bounds.bottom;
                }
            }
        }

        self.windows.push(ManagedWindow {
            hwnd: shwnd,
            width,
            border_left,
            border_top,
            border_right,
            border_bottom,
        });

        true
    }

    // Returns true if the window was present and removed.
    fn remove_window_internal(&mut self, hwnd: HWND) -> Option<usize> {
        let shwnd = SendHwnd::new(hwnd);
        if let Some(pos) = self.windows.iter().position(|w| w.hwnd == shwnd) {
            self.windows.remove(pos);
            return Some(pos);
        }
        None
    }

    fn max_offset(&self) -> i32 {
        let total_width =
            self.config.gap + self.windows.iter().map(|w| w.width + self.config.gap).sum::<i32>();
        std::cmp::max(0, total_width - self.screen_width)
    }

    fn update_window_size(&mut self, hwnd: HWND) -> bool {
        let shwnd = SendHwnd::new(hwnd);
        if let Some(w) = self.windows.iter_mut().find(|w| w.hwnd == shwnd) {
            unsafe {
                let mut bounds = RECT::default();
                if DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut bounds as *mut _ as *mut _,
                    std::mem::size_of::<RECT>() as u32,
                )
                .is_ok()
                {
                    let new_width = bounds.right - bounds.left;
                    if new_width > 0 && new_width != w.width {
                        w.width = new_width;

                        let mut win_info = WINDOWINFO::default();
                        win_info.cbSize = std::mem::size_of::<WINDOWINFO>() as u32;
                        if GetWindowInfo(hwnd, &mut win_info).is_ok() {
                            w.border_left = bounds.left - win_info.rcWindow.left;
                            w.border_top = bounds.top - win_info.rcWindow.top;
                            w.border_right = win_info.rcWindow.right - bounds.right;
                            w.border_bottom = win_info.rcWindow.bottom - bounds.bottom;
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Layout repositioning and sizing.
    /// Uses BeginDeferWindowPos/EndDeferWindowPos for atomic multi-window update.
    fn apply_layout(&mut self, size_changed: bool) {
        if !self.config.enabled {
            return;
        }

        let max_off = self.max_offset() as f32;
        self.target_offset_x = self.target_offset_x.clamp(0.0, max_off);
        if !self.config.smooth_scrolling {
            self.current_offset_x = self.target_offset_x;
        }
        self.current_offset_x = self.current_offset_x.clamp(0.0, max_off);

        if self.windows.is_empty() {
            return;
        }

        let current_offset_int = self.current_offset_x.round() as i32;
        self.last_rendered_int_offset = current_offset_int;

        unsafe {
            let mut current_x = self.screen_x + self.config.gap - current_offset_int;

            let base_flags = SWP_NOZORDER
                | SWP_NOACTIVATE
                | SWP_NOSENDCHANGING
                | SWP_ASYNCWINDOWPOS; // Critical for not blocking the animation thread

            let flags = if size_changed {
                base_flags | SWP_NOCOPYBITS
            } else {
                base_flags | SWP_NOSIZE
            };

            // Only use DeferWindowPos for resizing, as it blocks scrolling animation even with SWP_ASYNCWINDOWPOS
            let use_defer = size_changed;
            let mut hdwp = if use_defer {
                let window_count = self.windows.len() as i32;
                match BeginDeferWindowPos(window_count) {
                    Ok(h) if !h.is_invalid() => Some(h),
                    _ => None,
                }
            } else {
                None
            };

            for w in &self.windows {
                let hwnd = w.hwnd.get();

                // Skip a window while the user is actively dragging its borders
                if let Some(ref resizing) = self.resizing_hwnd {
                    if resizing.0 == w.hwnd.0 {
                        current_x += w.width + self.config.gap;
                        continue;
                    }
                }

                let target_x = current_x - w.border_left;
                let target_y = self.screen_y + self.config.gap - w.border_top;
                let target_w = w.width + w.border_left + w.border_right;
                let target_h = (self.screen_height - self.config.gap * 2)
                    + w.border_top
                    + w.border_bottom;

                if let Some(h) = hdwp {
                    match DeferWindowPos(
                        h,
                        hwnd,
                        Some(HWND::default()),
                        target_x,
                        target_y,
                        target_w,
                        target_h,
                        flags,
                    ) {
                        Ok(new_h) if !new_h.is_invalid() => {
                            hdwp = Some(new_h);
                        }
                        _ => {
                            let _ = SetWindowPos(
                                hwnd,
                                Some(HWND::default()),
                                target_x,
                                target_y,
                                target_w,
                                target_h,
                                flags,
                            );
                        }
                    }
                } else {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND::default()),
                        target_x,
                        target_y,
                        target_w,
                        target_h,
                        flags,
                    );
                }

                current_x += w.width + self.config.gap;
            }

            if let Some(h) = hdwp {
                let _ = EndDeferWindowPos(h);
            }
        }
    }

    /// Fast translation-only update for smooth scrolling frames.
    fn apply_scroll_frame(&mut self) {
        let current_offset_int = self.current_offset_x.round() as i32;
        if current_offset_int == self.last_rendered_int_offset {
            return;
        }
        self.apply_layout(false);
    }

    fn scroll(&mut self, delta: i32) {
        if !self.config.enabled {
            return;
        }

        if self.config.snap_to_window {
            let direction = if delta > 0 { -1 } else { 1 };

            let mut nearest_idx = 0;
            let mut min_dist = i32::MAX;

            let mut acc_x = 0;
            for (i, w) in self.windows.iter().enumerate() {
                let center_x = acc_x + w.width / 2;
                let view_center = self.target_offset_x as i32 + self.screen_width / 2;
                let dist = (center_x - view_center).abs();
                if dist < min_dist {
                    min_dist = dist;
                    nearest_idx = i;
                }
                acc_x += w.width + self.config.gap;
            }

            let target_idx =
                (nearest_idx as i32 + direction).clamp(0, (self.windows.len() as i32 - 1).max(0))
                    as usize;

            let mut target_acc_x = 0;
            for (i, w) in self.windows.iter().enumerate() {
                if i == target_idx {
                    let center_x = target_acc_x + w.width / 2;
                    self.target_offset_x = (center_x - self.screen_width / 2) as f32;
                    break;
                }
                target_acc_x += w.width + self.config.gap;
            }
        } else {
            let actual_delta = (delta as f32 / 120.0) * self.config.scroll_speed as f32;
            self.target_offset_x -= actual_delta;
        }
        self.target_offset_x = self.target_offset_x.clamp(0.0, self.max_offset() as f32);

        if !self.config.smooth_scrolling {
            self.current_offset_x = self.target_offset_x;
        }
    }

    // Updates target_offset_x to center the given window. Does not call apply_layout.
    fn focus_window_offset(&mut self, hwnd: HWND) {
        if !self.config.enabled {
            return;
        }
        let shwnd = SendHwnd::new(hwnd);
        let mut acc_x = 0;
        for w in &self.windows {
            if w.hwnd == shwnd {
                let window_left = acc_x;
                if w.width >= self.screen_width {
                    self.target_offset_x = window_left as f32;
                } else {
                    let center_x = window_left + w.width / 2;
                    self.target_offset_x = (center_x - self.screen_width / 2) as f32;
                }
                self.target_offset_x = self.target_offset_x.clamp(0.0, self.max_offset() as f32);

                if !self.config.smooth_scrolling {
                    self.current_offset_x = self.target_offset_x;
                }
                break;
            }
            acc_x += w.width + self.config.gap;
        }
    }

    fn move_active_window(&mut self, direction: i32) {
        unsafe {
            let fw = GetForegroundWindow();
            let shwnd = SendHwnd::new(fw);
            if let Some(pos) = self.windows.iter().position(|w| w.hwnd == shwnd) {
                let new_pos = (pos as i32 + direction).clamp(0, (self.windows.len().saturating_sub(1)) as i32) as usize;
                if new_pos != pos {
                    let w = self.windows.remove(pos);
                    self.windows.insert(new_pos, w);
                    self.focus_window_offset(fw);
                }
            }
        }
    }

    fn resize_active_window(&mut self, size_type: &str) {
        unsafe {
            let fw = GetForegroundWindow();
            let shwnd = SendHwnd::new(fw);
            if let Some(w) = self.windows.iter_mut().find(|win| win.hwnd == shwnd) {
                let screen_w = self.screen_width as f32;
                if size_type == "full" {
                    w.width = screen_w as i32;
                } else if size_type == "cycle" {
                    let options = [0.20, 0.25, 0.33, 0.50, 0.60, 0.80, 1.0];
                    let current_pct = w.width as f32 / screen_w;
                    let mut closest_idx = 0;
                    let mut min_diff = f32::MAX;
                    for (i, &opt) in options.iter().enumerate() {
                        let diff = (opt - current_pct).abs();
                        if diff < min_diff {
                            min_diff = diff;
                            closest_idx = i;
                        }
                    }
                    let next_idx = (closest_idx + 1) % options.len();
                    w.width = (screen_w * options[next_idx]).round() as i32;
                }
            }
            self.focus_window_offset(fw);
        }
    }
}


pub fn get_config() -> WmConfig {
    if let Ok(state) = STATE.lock() {
        state.config.clone()
    } else {
        WmConfig::default()
    }
}

pub fn set_config(config: WmConfig) {
    if let Ok(mut state) = STATE.lock() {
        state.config = config;
    }
    LAYOUT_DIRTY.store(true, Ordering::Relaxed);
    LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
    WAKE_CONDVAR.notify_one();
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DebugSnapshot {
    pub dt_ms: f32,
    pub fps: f32,
    pub current_offset: f32,
    pub target_offset: f32,
    pub smoothing_factor: f32,
}

pub static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);
pub static DEBUG_SNAPSHOT: Lazy<Mutex<DebugSnapshot>> = Lazy::new(|| Mutex::new(DebugSnapshot::default()));

static STATE: Lazy<Mutex<WmState>> = Lazy::new(|| Mutex::new(WmState::new()));
static RUNNING: AtomicBool = AtomicBool::new(false);
static SCROLL_ACCUM: AtomicI32 = AtomicI32::new(0);

// Dirty flags: set by hook threads, consumed by the animation loop.
// This ensures layout passes only happen at the animation loop's cadence,
// coalescing any burst of WinEvents (e.g. browser opening dozens of sub-windows)
// into a single layout pass per frame.
static LAYOUT_DIRTY: AtomicBool = AtomicBool::new(false);
static LAYOUT_SIZE_CHANGED: AtomicBool = AtomicBool::new(false);

// Zero-overhead condition variable for waking the physics thread immediately
static WAKE_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static WAKE_CONDVAR: Lazy<Condvar> = Lazy::new(Condvar::new);

// Cache of HWNDs that failed is_manageable() — avoids re-running the full check
// (multiple Win32 API calls) for the same non-manageable window on every event.
// Evicted when the window is destroyed.
static REJECTED_CACHE: Lazy<Mutex<HashSet<isize>>> = Lazy::new(|| Mutex::new(HashSet::new()));

fn is_manageable(hwnd: HWND) -> bool {
    unsafe {
        if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }

        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
        {
            if cloaked != 0 {
                return false;
            }
        }

        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        if style & WS_CHILD.0 != 0 {
            return false;
        }
        if style & WS_MINIMIZE.0 != 0 {
            return false;
        }

        if (ex_style & WS_EX_TOOLWINDOW.0 != 0) && (ex_style & WS_EX_APPWINDOW.0 == 0) {
            return false;
        }

        if let Ok(parent) = GetParent(hwnd) {
            if !parent.is_invalid()
                && parent.0 != std::ptr::null_mut()
                && (ex_style & WS_EX_APPWINDOW.0 == 0)
            {
                return false;
            }
        }

        if (style & WS_POPUP.0 != 0)
            && (style & (WS_THICKFRAME.0 | WS_CAPTION.0) == 0)
            && (ex_style & WS_EX_APPWINDOW.0 == 0)
        {
            return false;
        }

        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return false;
        }

        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buf);
        let class_name = String::from_utf16_lossy(&buf[..len as usize]);

        let ignored_classes = [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "Windows.UI.Core.CoreWindow",
            "SearchHost",
            "LockScreenBackdropWindow",
            "ScreenClippingWindow",
            "SystemTray_Main",
        ];

        if ignored_classes.contains(&class_name.as_str()) {
            return false;
        }
    }
    true
}

unsafe extern "system" fn win_event_hook(
    _hwineventhook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    idobject: i32,
    _idchild: i32,
    _ideventthread: u32,
    _dwmseventtime: u32,
) {
    if idobject != OBJID_WINDOW.0 {
        return;
    }

    if event == EVENT_OBJECT_DESTROY {
        // Evict from rejected cache so it can be re-evaluated if the handle is reused.
        if let Ok(mut cache) = REJECTED_CACHE.lock() {
            cache.remove(&(hwnd.0 as isize));
        }
        if let Ok(mut state) = STATE.lock() {
            state.remove_window_internal(hwnd);
        }
        LAYOUT_DIRTY.store(true, Ordering::Relaxed);
        LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
        WAKE_CONDVAR.notify_one();
        return;
    }

    if event == EVENT_OBJECT_HIDE || event == EVENT_SYSTEM_MINIMIZESTART {
        if let Ok(mut state) = STATE.lock() {
            if state.remove_window_internal(hwnd).is_some() {
                LAYOUT_DIRTY.store(true, Ordering::Relaxed);
                LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
                WAKE_CONDVAR.notify_one();
            }
        }
        return;
    }

    // Capture resize start so smooth scrolling/layout won't fight the user's manual dragging
    if event == EVENT_SYSTEM_MOVESIZESTART {
        if let Ok(mut state) = STATE.lock() {
            state.resizing_hwnd = Some(SendHwnd::new(hwnd));
        }
        return;
    }

    // Resize/move finished: update width and re-tile cleanly
    if event == EVENT_SYSTEM_MOVESIZEEND {
        if let Ok(mut state) = STATE.lock() {
            state.resizing_hwnd = None;
            state.update_window_size(hwnd);
        }
        LAYOUT_DIRTY.store(true, Ordering::Relaxed);
        LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
        WAKE_CONDVAR.notify_one();
        return;
    }

    // Check the rejected cache first to avoid redundant Win32 calls for known
    // non-manageable windows (browser internal sub-windows fire many events).
    let hwnd_key = hwnd.0 as isize;
    if let Ok(cache) = REJECTED_CACHE.lock() {
        if cache.contains(&hwnd_key) {
            return;
        }
    }

    if !is_manageable(hwnd) {
        if let Ok(mut cache) = REJECTED_CACHE.lock() {
            cache.insert(hwnd_key);
        }
        return;
    }

    match event {
        EVENT_OBJECT_SHOW | EVENT_OBJECT_CREATE | EVENT_SYSTEM_MINIMIZEEND => {
            let added = if let Ok(mut state) = STATE.lock() {
                state.add_window_internal(hwnd)
            } else {
                false
            };
            if added {
                // Check foreground outside the STATE lock to avoid deadlock
                let is_fg = unsafe { GetForegroundWindow() == hwnd };
                if is_fg {
                    if let Ok(mut state) = STATE.lock() {
                        state.focus_window_offset(hwnd);
                    }
                }
                LAYOUT_DIRTY.store(true, Ordering::Relaxed);
                LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
                WAKE_CONDVAR.notify_one();
            }
        }
        EVENT_SYSTEM_FOREGROUND => {
            if let Ok(mut state) = STATE.lock() {
                let added = state.add_window_internal(hwnd);
                state.focus_window_offset(hwnd);
                if added {
                    LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
                }
            }
            LAYOUT_DIRTY.store(true, Ordering::Relaxed);
            WAKE_CONDVAR.notify_one();
        }
        _ => {}
    }
}

// Dedicated, non-blocking low-level mouse hook procedure.
// Must never perform I/O, heavy computation, or mutex locking.
unsafe extern "system" fn mouse_hook_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode >= 0 && wparam.0 as u32 == WM_MOUSEWHEEL {
        let alt_state = GetAsyncKeyState(VK_MENU.0 as i32);
        let shift_state = GetAsyncKeyState(VK_SHIFT.0 as i32);
        let is_alt = (alt_state as u16 & 0x8000) != 0;
        let is_shift = (shift_state as u16 & 0x8000) != 0;

        if is_alt {
            let msll = *(lparam.0 as *const MSLLHOOKSTRUCT);
            let mouse_data = msll.mouseData;
            let delta = (mouse_data >> 16) as i16;

            if is_shift {
                // scroll down (delta < 0) = move left/back (-1)
                // scroll up (delta > 0) = move right/forward (+1)
                let dir = if delta > 0 { 1 } else { -1 };
                if let Ok(mut lock) = ACTIONS.lock() {
                    lock.push(WmAction::MoveWindow(dir));
                }
                WAKE_CONDVAR.notify_one();
            } else {
                SCROLL_ACCUM.fetch_add(delta as i32, Ordering::Relaxed);
                WAKE_CONDVAR.notify_one();
            }

            return LRESULT(1);
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

unsafe extern "system" fn keyboard_hook_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode >= 0 && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN) {
        let alt_state = GetAsyncKeyState(VK_MENU.0 as i32);
        let is_alt = (alt_state as u16 & 0x8000) != 0;

        if is_alt {
            let kbd = *(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = kbd.vkCode;
            let mut handled = false;

            if vk == VK_LEFT.0 as u32 {
                if let Ok(mut lock) = ACTIONS.lock() {
                    lock.push(WmAction::MoveWindow(-1));
                }
                handled = true;
            } else if vk == VK_RIGHT.0 as u32 {
                if let Ok(mut lock) = ACTIONS.lock() {
                    lock.push(WmAction::MoveWindow(1));
                }
                handled = true;
            } else if vk == 0x53 { // 'S'
                if let Ok(mut lock) = ACTIONS.lock() {
                    lock.push(WmAction::ResizeWindow("cycle"));
                }
                handled = true;
            } else if vk == 0x46 { // 'F'
                if let Ok(mut lock) = ACTIONS.lock() {
                    lock.push(WmAction::ResizeWindow("full"));
                }
                handled = true;
            }

            if handled {
                WAKE_CONDVAR.notify_one();
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}


unsafe extern "system" fn enum_windows_proc(hwnd: HWND, _: LPARAM) -> windows::core::BOOL {
    if is_manageable(hwnd) {
        if let Ok(mut state) = STATE.lock() {
            state.add_window_internal(hwnd);
        }
    }
    true.into()
}

fn get_refresh_rate(hwnd: Option<HWND>) -> u32 {
    unsafe {
        let hmonitor = if let Some(h) = hwnd {
            MonitorFromWindow(h, MONITOR_DEFAULTTOPRIMARY)
        } else {
            MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY)
        };

        if !hmonitor.is_invalid() {
            let mut monitor_info = MONITORINFOEXW::default();
            monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let lpmi = &mut monitor_info as *mut MONITORINFOEXW as *mut MONITORINFO;
            if GetMonitorInfoW(hmonitor, lpmi).as_bool() {
                let mut devmode = DEVMODEW::default();
                devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
                let device_name = windows::core::PCWSTR::from_raw(monitor_info.szDevice.as_ptr());
                if EnumDisplaySettingsW(device_name, ENUM_CURRENT_SETTINGS, &mut devmode).as_bool() {
                    if devmode.dmDisplayFrequency > 0 {
                        return devmode.dmDisplayFrequency;
                    }
                }
            }
        }

        // Fallback to primary display using None
        let mut devmode = DEVMODEW::default();
        devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut devmode).as_bool() {
            if devmode.dmDisplayFrequency > 0 {
                return devmode.dmDisplayFrequency;
            }
        }
    }
    60 // Safe fallback
}

pub fn start_wm() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    // 1. High-precision physics/animation loop paced directly by the hardware refresh rate.
    thread::spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
            let _ = timeBeginPeriod(1);
        }
        let mut last_tick = Instant::now();
        let mut actions = Vec::with_capacity(8);

        // Cache QPC frequency (constant for the life of the process)
        let qpc_freq: u64 = unsafe {
            let mut freq = 0i64;
            let _ = QueryPerformanceFrequency(&mut freq);
            freq as u64
        };
        let mut next_target_qpc = 0u64;
        let mut target_period = 0u64;

        loop {
            let mut is_animating = false;
            let mut factor_used = 0.0;
            let mut current_offset = 0.0;
            let mut target_offset = 0.0;

            let now = Instant::now();
            let dt = (now - last_tick).as_secs_f32().clamp(0.0005, 0.05);
            last_tick = now;

            if let Ok(mut state) = STATE.lock() {
                actions.clear();
                if let Ok(mut lock) = ACTIONS.lock() {
                    std::mem::swap(&mut actions, &mut *lock);
                }
                for action in actions.drain(..) {
                    match action {
                        WmAction::MoveWindow(dir) => {
                            state.move_active_window(dir);
                            LAYOUT_DIRTY.store(true, Ordering::Relaxed);
                            LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
                        }
                        WmAction::ResizeWindow(t) => {
                            state.resize_active_window(t);
                            LAYOUT_DIRTY.store(true, Ordering::Relaxed);
                            LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
                        }
                    }
                }

                let delta = SCROLL_ACCUM.swap(0, Ordering::Relaxed);
                if delta != 0 {
                    state.scroll(delta);
                    LAYOUT_DIRTY.store(true, Ordering::Relaxed);
                }

                // Process dirty layout flag — coalesces all WinEvent-driven layout requests
                let dirty = LAYOUT_DIRTY.swap(false, Ordering::Relaxed);
                let size_changed = LAYOUT_SIZE_CHANGED.swap(false, Ordering::Relaxed);

                if state.config.enabled && state.config.smooth_scrolling {
                    let diff = state.target_offset_x - state.current_offset_x;
                    if diff.abs() > 0.5 {
                        is_animating = true;
                        factor_used = 1.0 - (-22.0 * dt).exp();
                        state.current_offset_x += diff * factor_used;
                        state.apply_scroll_frame();
                    } else if state.current_offset_x != state.target_offset_x {
                        state.current_offset_x = state.target_offset_x;
                        state.apply_scroll_frame();
                    } else if dirty {
                        // No animation in progress, but layout changed (window add/remove/resize)
                        state.apply_layout(size_changed);
                    }
                } else if dirty {
                    state.apply_layout(size_changed);
                }

                current_offset = state.current_offset_x;
                target_offset = state.target_offset_x;
            }

            if DEBUG_ENABLED.load(Ordering::Relaxed) {
                if let Ok(mut snap) = DEBUG_SNAPSHOT.lock() {
                    snap.dt_ms = dt * 1000.0;
                    snap.fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
                    snap.current_offset = current_offset;
                    snap.target_offset = target_offset;
                    snap.smoothing_factor = factor_used;
                }
            }

            if is_animating {
                unsafe {
                    let mut now_qpc = 0i64;
                    let _ = QueryPerformanceCounter(&mut now_qpc);
                    let now_qpc = now_qpc as u64;

                    if next_target_qpc == 0 {
                        // First frame of animation: query hardware refresh rate
                        let fw = GetForegroundWindow();
                        let refresh_rate = get_refresh_rate(if fw.0.is_null() { None } else { Some(fw) }).max(30);
                        target_period = qpc_freq / refresh_rate as u64;
                        next_target_qpc = now_qpc + target_period;
                    } else {
                        // Advance exactly by the hardware refresh period
                        next_target_qpc += target_period;
                    }

                    // If we fell behind by more than 2 frames, resync target to now + target_period
                    if now_qpc > next_target_qpc + target_period * 2 {
                        next_target_qpc = now_qpc + target_period;
                    }

                    let ticks_to_sleep = next_target_qpc.saturating_sub(now_qpc);
                    let nanos = ticks_to_sleep
                        .saturating_mul(1_000_000_000)
                        .checked_div(qpc_freq)
                        .unwrap_or(0);

                    if nanos > 0 {
                        // Sleep the bulk of the time (save CPU), stopping 1.5ms early
                        if nanos > 1_500_000 {
                            thread::sleep(Duration::from_nanos(nanos - 1_500_000));
                        }

                        // Spin-wait the final <1.5ms for perfect microsecond pacing
                        loop {
                            let mut current = 0i64;
                            let _ = QueryPerformanceCounter(&mut current);
                            if (current as u64) >= next_target_qpc {
                                break;
                            }
                            std::hint::spin_loop();
                        }
                    }
                }
            } else {
                next_target_qpc = 0; // Reset metronome when animation stops

                // Nothing to animate: sleep until woken by input or 50 ms timeout.
                // Reset last_tick after sleeping so dt doesn't include sleep time.
                if let Ok(lock) = WAKE_MUTEX.lock() {
                    let actions_empty = ACTIONS.lock().map(|a| a.is_empty()).unwrap_or(true);
                    if SCROLL_ACCUM.load(Ordering::Relaxed) == 0
                        && actions_empty
                        && !LAYOUT_DIRTY.load(Ordering::Relaxed)
                    {
                        let _ = WAKE_CONDVAR.wait_timeout(lock, Duration::from_millis(50));
                    }
                }
                last_tick = Instant::now();
            }
        }
    });

    // 2. Dedicated Input Hook Thread (Isolated from WinEvents and Layout to prevent timeouts)
    thread::spawn(move || {
        unsafe {
            let hinstance = match GetModuleHandleW(None) {
                Ok(h) => HINSTANCE(h.0),
                Err(_) => HINSTANCE::default(),
            };

            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), Some(hinstance), 0)
                .expect("Failed to set low-level mouse hook");

            let kbd_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), Some(hinstance), 0)
                .expect("Failed to set low-level keyboard hook");

            let mut msg: MSG = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }

            let _ = UnhookWindowsHookEx(mouse_hook);
            let _ = UnhookWindowsHookEx(kbd_hook);
        }
    });

    // 3. Dedicated Window Management and WinEvent Hook Thread
    thread::spawn(move || {
        if let Ok(mut state) = STATE.lock() {
            state.update_work_area();
        }

        unsafe {
            let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));
            LAYOUT_DIRTY.store(true, Ordering::Relaxed);
            LAYOUT_SIZE_CHANGED.store(true, Ordering::Relaxed);
            WAKE_CONDVAR.notify_one();

            let win_hook_create_destroy = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_HIDE,
                None,
                Some(win_event_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );

            let win_hook_create = SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_DESTROY,
                None,
                Some(win_event_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );

            let win_hook_minimize = SetWinEventHook(
                EVENT_SYSTEM_MINIMIZESTART,
                EVENT_SYSTEM_MINIMIZEEND,
                None,
                Some(win_event_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );

            let win_hook_movesize = SetWinEventHook(
                EVENT_SYSTEM_MOVESIZESTART,
                EVENT_SYSTEM_MOVESIZEEND,
                None,
                Some(win_event_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );

            let win_hook_foreground = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event_hook),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );

            let mut msg: MSG = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }

            let _ = UnhookWinEvent(win_hook_create_destroy);
            let _ = UnhookWinEvent(win_hook_create);
            let _ = UnhookWinEvent(win_hook_minimize);
            let _ = UnhookWinEvent(win_hook_movesize);
            let _ = UnhookWinEvent(win_hook_foreground);
        }
    });
}
