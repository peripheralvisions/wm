use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmFlush, DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, CallNextHookEx, DeferWindowPos, DispatchMessageW, EndDeferWindowPos,
    EnumWindows, GetClassNameW, GetForegroundWindow, GetMessageW, GetParent, GetWindowInfo,
    GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, SetWindowPos,
    SetWindowsHookExW, SystemParametersInfoW, TranslateMessage, UnhookWindowsHookEx,
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GWL_EXSTYLE, GWL_STYLE, MSG,
    MSLLHOOKSTRUCT, OBJID_WINDOW, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOCOPYBITS,
    SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WH_MOUSE_LL, WINDOWINFO, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_MOUSEWHEEL,
    WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MINIMIZE, WS_POPUP, WS_THICKFRAME,
};

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
    is_updating_layout: bool,
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
            is_updating_layout: false,
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

    fn add_window(&mut self, hwnd: HWND) {
        if self.add_window_internal(hwnd) {
            self.apply_layout(true);
        }
    }

    fn remove_window(&mut self, hwnd: HWND) {
        let shwnd = SendHwnd::new(hwnd);
        if let Some(pos) = self.windows.iter().position(|w| w.hwnd == shwnd) {
            self.windows.remove(pos);

            if !self.windows.is_empty() {
                let next_idx = if pos < self.windows.len() {
                    pos
                } else {
                    self.windows.len() - 1
                };
                let next_hwnd = self.windows[next_idx].hwnd.get();
                self.focus_window(next_hwnd);

                unsafe {
                    let _ = SetForegroundWindow(next_hwnd);
                }
            } else {
                self.apply_layout(true);
            }
        }
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
        if !self.config.enabled || self.is_updating_layout {
            return;
        }
        self.is_updating_layout = true;

        let max_off = self.max_offset() as f32;
        self.target_offset_x = self.target_offset_x.clamp(0.0, max_off);
        if !self.config.smooth_scrolling {
            self.current_offset_x = self.target_offset_x;
        }
        self.current_offset_x = self.current_offset_x.clamp(0.0, max_off);

        if self.windows.is_empty() {
            self.is_updating_layout = false;
            return;
        }

        let current_offset_int = self.current_offset_x.round() as i32;
        self.last_rendered_int_offset = current_offset_int;

        unsafe {
            let mut current_x = self.screen_x + self.config.gap - current_offset_int;
            let window_count = self.windows.len() as i32;

            let hdwp_res = BeginDeferWindowPos(window_count);
            let mut hdwp = match hdwp_res {
                Ok(h) if !h.is_invalid() => Some(h),
                _ => None,
            };

            let base_flags = SWP_NOZORDER
                | SWP_NOACTIVATE
                | SWP_NOCOPYBITS
                | SWP_NOSENDCHANGING;

            let flags = if size_changed {
                base_flags
            } else {
                base_flags | SWP_NOSIZE
            };

            for w in &self.windows {
                let hwnd = w.hwnd.get();

                // Skip repositioning a window while the user is actively dragging its borders
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

        self.is_updating_layout = false;
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
            self.apply_layout(false);
        }
    }

    fn focus_window(&mut self, hwnd: HWND) {
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
                self.apply_layout(false);
                break;
            }
            acc_x += w.width + self.config.gap;
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
        state.apply_layout(true);
    }
}

static STATE: Lazy<Mutex<WmState>> = Lazy::new(|| Mutex::new(WmState::new()));
static RUNNING: AtomicBool = AtomicBool::new(false);
static SCROLL_ACCUM: AtomicI32 = AtomicI32::new(0);

// Zero-overhead condition variable for waking the physics thread immediately
static WAKE_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static WAKE_CONDVAR: Lazy<Condvar> = Lazy::new(Condvar::new);

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

    if event == EVENT_OBJECT_HIDE
        || event == EVENT_OBJECT_DESTROY
        || event == EVENT_SYSTEM_MINIMIZESTART
    {
        if let Ok(mut state) = STATE.lock() {
            state.remove_window(hwnd);
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
            if state.update_window_size(hwnd) {
                state.apply_layout(true);
            }
        }
        return;
    }

    if !is_manageable(hwnd) {
        return;
    }

    match event {
        EVENT_OBJECT_SHOW | EVENT_OBJECT_CREATE | EVENT_SYSTEM_MINIMIZEEND => {
            if let Ok(mut state) = STATE.lock() {
                state.add_window(hwnd);
                if GetForegroundWindow() == hwnd {
                    state.focus_window(hwnd);
                }
            }
        }
        EVENT_SYSTEM_FOREGROUND => {
            if let Ok(mut state) = STATE.lock() {
                state.add_window(hwnd);
                state.focus_window(hwnd);
            }
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
        if (alt_state as u16 & 0x8000) != 0 {
            let msll = *(lparam.0 as *const MSLLHOOKSTRUCT);
            let mouse_data = msll.mouseData;
            let delta = (mouse_data >> 16) as i16;

            SCROLL_ACCUM.fetch_add(delta as i32, Ordering::Relaxed);
            WAKE_CONDVAR.notify_one();

            return LRESULT(1);
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

pub fn start_wm() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    // 1. High-refresh rate VSync-locked physics loop
    thread::spawn(move || {
        let mut last_tick = Instant::now();

        loop {
            // Process any accumulated scroll wheel inputs
            let delta = SCROLL_ACCUM.swap(0, Ordering::Relaxed);
            if delta != 0 {
                if let Ok(mut state) = STATE.lock() {
                    state.scroll(delta);
                }
            }

            // Check if animation is active
            let mut is_animating = false;
            if let Ok(state) = STATE.lock() {
                if state.config.enabled && state.config.smooth_scrolling {
                    is_animating = (state.target_offset_x - state.current_offset_x).abs() > 0.25;
                }
            }

            if !is_animating {
                // Settle exact target offset when near zero
                if let Ok(mut state) = STATE.lock() {
                    if state.config.enabled
                        && state.config.smooth_scrolling
                        && state.current_offset_x != state.target_offset_x
                    {
                        state.current_offset_x = state.target_offset_x;
                        state.apply_scroll_frame();
                    }
                }

                // Wait for next scroll input with zero-latency wakeup via Condvar
                if let Ok(lock) = WAKE_MUTEX.lock() {
                    if SCROLL_ACCUM.load(Ordering::Relaxed) == 0 {
                        let _ = WAKE_CONDVAR.wait_timeout(lock, Duration::from_millis(50));
                    }
                }

                last_tick = Instant::now();
                continue;
            }

            // Calculate precise delta-time between monitor VSync frames
            let now = Instant::now();
            let dt = (now - last_tick).as_secs_f32().clamp(0.0005, 0.05);
            last_tick = now;

            // Frame-rate independent exponential smoothing
            if let Ok(mut state) = STATE.lock() {
                if state.config.smooth_scrolling {
                    let diff = state.target_offset_x - state.current_offset_x;
                    if diff.abs() > 0.25 {
                        let factor = 1.0 - (-16.0 * dt).exp();
                        state.current_offset_x += diff * factor;
                        state.apply_scroll_frame();
                    } else {
                        state.current_offset_x = state.target_offset_x;
                        state.apply_scroll_frame();
                    }
                }
            }

            // Sync with DWM compositor VSync (adapts to 60Hz, 144Hz, 240Hz, etc.)
            unsafe {
                let _ = DwmFlush();
            }
        }
    });

    // 2. Dedicated Mouse Hook Thread (Isolated from WinEvents and Layout to prevent timeouts)
    thread::spawn(move || {
        unsafe {
            let hinstance = match GetModuleHandleW(None) {
                Ok(h) => HINSTANCE(h.0),
                Err(_) => HINSTANCE::default(),
            };

            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), Some(hinstance), 0)
                .expect("Failed to set low-level mouse hook");

            let mut msg: MSG = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }

            let _ = UnhookWindowsHookEx(mouse_hook);
        }
    });

    // 3. Dedicated Window Management and WinEvent Hook Thread
    thread::spawn(move || {
        if let Ok(mut state) = STATE.lock() {
            state.update_work_area();
        }

        unsafe {
            let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));
            if let Ok(mut state) = STATE.lock() {
                state.apply_layout(true);
            }

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
