use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::sync::mpsc::{channel, Sender};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, GetSystemMetrics, GetWindowInfo,
    GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible, SetWindowPos,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, EVENT_OBJECT_HIDE,
    EVENT_OBJECT_SHOW, EVENT_OBJECT_DESTROY, EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_FOREGROUND,
    GWL_EXSTYLE, GWL_STYLE, HHOOK, MSG,
    MSLLHOOKSTRUCT, OBJID_WINDOW, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOZORDER,
    WH_MOUSE_LL, WINDOWINFO, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_MOUSEWHEEL,
    WS_CHILD, WS_EX_TOOLWINDOW, WS_POPUP, WS_MINIMIZE, EnumWindows, GetClassNameW,
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos
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
    current_offset_x: i32,
    target_offset_x: i32,
    screen_width: i32,
    screen_height: i32,
    config: WmConfig,
}

impl WmState {
    fn new() -> Self {
        Self {
            windows: Vec::new(),
            current_offset_x: 0,
            target_offset_x: 0,
            screen_width: 1920,
            screen_height: 1080,
            config: WmConfig::default(),
        }
    }

    fn add_window(&mut self, hwnd: HWND) {
        let shwnd = SendHwnd::new(hwnd);
        if self.windows.iter().any(|w| w.hwnd == shwnd) {
            return;
        }

        let width = if self.config.column_sizing_mode == "percent" {
            (self.screen_width as f32 * (self.config.column_sizing_value / 100.0)) as i32
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
            ).is_ok() {
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
        self.apply_layout();
    }

    fn remove_window(&mut self, hwnd: HWND) {
        let shwnd = SendHwnd::new(hwnd);
        self.windows.retain(|w| w.hwnd != shwnd);
        self.apply_layout();
    }

    fn max_offset(&self) -> i32 {
        let total_width = self.config.gap + self.windows.iter().map(|w| w.width + self.config.gap).sum::<i32>();
        std::cmp::max(0, total_width - self.screen_width)
    }

    fn update_window_size(&mut self, hwnd: HWND) {
        let shwnd = SendHwnd::new(hwnd);
        if let Some(w) = self.windows.iter_mut().find(|w| w.hwnd == shwnd) {
            unsafe {
                let mut bounds = RECT::default();
                if DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut bounds as *mut _ as *mut _,
                    std::mem::size_of::<RECT>() as u32,
                ).is_ok() {
                    let new_width = bounds.right - bounds.left;
                    if new_width > 0 {
                        w.width = new_width;
                    }

                    let mut win_info = WINDOWINFO::default();
                    win_info.cbSize = std::mem::size_of::<WINDOWINFO>() as u32;
                    if GetWindowInfo(hwnd, &mut win_info).is_ok() {
                        w.border_left = bounds.left - win_info.rcWindow.left;
                        w.border_top = bounds.top - win_info.rcWindow.top;
                        w.border_right = win_info.rcWindow.right - bounds.right;
                        w.border_bottom = win_info.rcWindow.bottom - bounds.bottom;
                    }
                }
            }
        }
    }

    fn apply_layout(&mut self) {
        if !self.config.enabled {
            return;
        }
        self.target_offset_x = self.target_offset_x.clamp(0, self.max_offset());
        if !self.config.smooth_scrolling {
            self.current_offset_x = self.target_offset_x;
        }
        self.current_offset_x = self.current_offset_x.clamp(0, self.max_offset());

        if self.windows.is_empty() {
            return;
        }

        unsafe {
            let hdwp = match BeginDeferWindowPos(self.windows.len() as i32) {
                Ok(h) if !h.is_invalid() => h,
                _ => {
                    // Fallback to regular SetWindowPos if DeferWindowPos fails
                    let mut current_x = self.config.gap - self.current_offset_x;
                    for w in &self.windows {
                        let target_x = current_x - w.border_left;
                        let target_y = self.config.gap - w.border_top;
                        let target_w = w.width + w.border_left + w.border_right;
                        let target_h = (self.screen_height - self.config.gap * 2) + w.border_top + w.border_bottom;

                        let _ = SetWindowPos(
                            w.hwnd.get(),
                            Some(HWND::default()),
                            target_x,
                            target_y,
                            target_w,
                            target_h,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                        current_x += w.width + self.config.gap;
                    }
                    return;
                }
            };

            let mut current_hdwp = hdwp;
            let mut current_x = self.config.gap - self.current_offset_x;

            for w in &self.windows {
                let hwnd = w.hwnd.get();
                let target_x = current_x - w.border_left;
                let target_y = self.config.gap - w.border_top;
                let target_w = w.width + w.border_left + w.border_right;
                let target_h = (self.screen_height - self.config.gap * 2) + w.border_top + w.border_bottom;

                if let Ok(next_hdwp) = DeferWindowPos(
                    current_hdwp,
                    hwnd,
                    Some(HWND::default()),
                    target_x,
                    target_y,
                    target_w,
                    target_h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                ) {
                    current_hdwp = next_hdwp;
                }
                current_x += w.width + self.config.gap;
            }

            let _ = EndDeferWindowPos(current_hdwp);
        }
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
                let view_center = self.target_offset_x + self.screen_width / 2;
                let dist = (center_x - view_center).abs();
                if dist < min_dist {
                    min_dist = dist;
                    nearest_idx = i;
                }
                acc_x += w.width + self.config.gap;
            }

            let target_idx = (nearest_idx as i32 + direction).clamp(0, (self.windows.len() as i32 - 1).max(0)) as usize;

            let mut target_acc_x = 0;
            for (i, w) in self.windows.iter().enumerate() {
                if i == target_idx {
                    let center_x = target_acc_x + w.width / 2;
                    self.target_offset_x = center_x - self.screen_width / 2;
                    break;
                }
                target_acc_x += w.width + self.config.gap;
            }
        } else {
            let actual_delta = if delta > 0 { self.config.scroll_speed } else { -self.config.scroll_speed };
            self.target_offset_x -= actual_delta;
        }
        self.target_offset_x = self.target_offset_x.clamp(0, self.max_offset());

        if !self.config.smooth_scrolling {
            self.current_offset_x = self.target_offset_x;
        }
        // apply_layout is handled by the smooth scrolling thread or at the end of batched scroll events
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
                    self.target_offset_x = window_left;
                } else {
                    let center_x = window_left + w.width / 2;
                    self.target_offset_x = center_x - self.screen_width / 2;
                }
                self.target_offset_x = self.target_offset_x.clamp(0, self.max_offset());

                if !self.config.smooth_scrolling {
                    self.current_offset_x = self.target_offset_x;
                }
                self.apply_layout();
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
        state.apply_layout();
    }
}

static STATE: Lazy<Mutex<WmState>> = Lazy::new(|| Mutex::new(WmState::new()));
static RUNNING: AtomicBool = AtomicBool::new(false);
static mut MOUSE_HOOK: HHOOK = HHOOK(std::ptr::null_mut());
static SCROLL_TX: Lazy<Mutex<Option<Sender<i32>>>> = Lazy::new(|| Mutex::new(None));

fn is_manageable(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        if style & WS_CHILD.0 != 0 {
            return false;
        }
        if style & WS_POPUP.0 != 0 {
            return false;
        }
        if style & WS_MINIMIZE.0 != 0 {
            return false;
        }
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }

        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
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

    if event == EVENT_OBJECT_HIDE || event == EVENT_OBJECT_DESTROY || event == EVENT_SYSTEM_MINIMIZESTART {
        if let Ok(mut state) = STATE.lock() {
            state.remove_window(hwnd);
        }
        return;
    }

    if !is_manageable(hwnd) {
        return;
    }

    match event {
        EVENT_OBJECT_SHOW | EVENT_SYSTEM_MINIMIZEEND => {
            if let Ok(mut state) = STATE.lock() {
                state.add_window(hwnd);
            }
        }
        EVENT_SYSTEM_MOVESIZEEND => {
            if let Ok(mut state) = STATE.lock() {
                state.update_window_size(hwnd);
                state.apply_layout();
            }
        }
        EVENT_SYSTEM_FOREGROUND => {
            if let Ok(mut state) = STATE.lock() {
                state.focus_window(hwnd);
            }
        }
        _ => {}
    }
}

unsafe extern "system" fn mouse_hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode >= 0 {
        if wparam.0 as u32 == WM_MOUSEWHEEL {
            let alt_state = GetAsyncKeyState(VK_MENU.0 as i32);
            if (alt_state as u16 & 0x8000) != 0 {
                let msll = *(lparam.0 as *const MSLLHOOKSTRUCT);
                let mouse_data = msll.mouseData;
                let delta = (mouse_data >> 16) as i16;

                // Send without blocking on the main state mutex
                if let Ok(guard) = SCROLL_TX.lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(delta as i32);
                    }
                }
                return LRESULT(1); // Consume the event
            }
        }
    }
    CallNextHookEx(Some(MOUSE_HOOK), ncode, wparam, lparam)
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, _: LPARAM) -> windows::core::BOOL {
    if is_manageable(hwnd) {
        if let Ok(mut state) = STATE.lock() {
            state.add_window(hwnd);
        }
    }
    true.into()
}

pub fn start_wm() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    let (tx, rx) = channel::<i32>();
    if let Ok(mut guard) = SCROLL_TX.lock() {
        *guard = Some(tx);
    }

    // Smooth scrolling and event processing thread
    thread::spawn(move || {
        loop {
            let mut processed = false;

            // Process all pending scroll events from the queue
            while let Ok(delta) = rx.try_recv() {
                if let Ok(mut state) = STATE.lock() {
                    state.scroll(delta);
                }
                processed = true;
            }

            if let Ok(mut state) = STATE.lock() {
                if state.config.smooth_scrolling && state.current_offset_x != state.target_offset_x {
                    let diff = state.target_offset_x - state.current_offset_x;
                    if diff.abs() <= 2 {
                        state.current_offset_x = state.target_offset_x;
                    } else {
                        let step = (diff as f32 * 0.25).trunc() as i32;
                        let step = if step == 0 { diff.signum() } else { step };
                        state.current_offset_x += step;
                    }
                    state.apply_layout();
                } else if processed && !state.config.smooth_scrolling {
                    // Update layout once after all immediate scroll events are processed
                    state.apply_layout();
                }
            }
            thread::sleep(std::time::Duration::from_millis(16));
        }
    });

    thread::spawn(move || {
        unsafe {
            let cx = GetSystemMetrics(SM_CXSCREEN);
            let cy = GetSystemMetrics(SM_CYSCREEN);
            if let Ok(mut state) = STATE.lock() {
                state.screen_width = cx;
                state.screen_height = cy - 40; // Approx taskbar space
            }

            // Collect existing windows
            let _ = EnumWindows(Some(enum_windows_proc), LPARAM(0));
        }

        unsafe {
            MOUSE_HOOK = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_hook_proc),
                None,
                0,
            )
            .expect("Failed to set mouse hook");

            let win_hook_create_destroy = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_HIDE,
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

            let win_hook_movesizeend = SetWinEventHook(
                EVENT_SYSTEM_MOVESIZEEND,
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
            while GetMessageW(&mut msg, Some(HWND::default()), 0, 0).into() {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }

            let _ = UnhookWindowsHookEx(MOUSE_HOOK);
            let _ = UnhookWinEvent(win_hook_create_destroy);
            let _ = UnhookWinEvent(win_hook_minimize);
            let _ = UnhookWinEvent(win_hook_movesizeend);
            let _ = UnhookWinEvent(win_hook_foreground);
        }
    });
}
