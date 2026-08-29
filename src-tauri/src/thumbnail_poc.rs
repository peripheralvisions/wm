use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmRegisterThumbnail, DwmUpdateThumbnailProperties, DwmUnregisterThumbnail, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_VISIBLE, DWM_TNP_RECTDESTINATION, DWM_TNP_OPACITY, DWM_TNP_SOURCECLIENTAREAONLY};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, CreateWindowExW, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_TOOLWINDOW, WS_POPUP, ShowWindow, SetLayeredWindowAttributes, DestroyWindow, SW_SHOW, GetWindowRect};
use windows::core::PCWSTR;
use std::time::Instant;
use std::thread::sleep;
use std::time::Duration;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

fn to_pcwstr(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn main() {
    unsafe {
        let class_name = to_pcwstr("Chrome_WidgetWin_1");
        let chrome_hwnd = FindWindowW(PCWSTR(class_name.as_ptr()), PCWSTR(std::ptr::null()));
        if chrome_hwnd.0.is_null() {
            println!("Chrome not found");
            return;
        }

        let overlay_class = to_pcwstr("STATIC");
        let overlay_hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
            PCWSTR(overlay_class.as_ptr()),
            PCWSTR(to_pcwstr("Overlay").as_ptr()),
            WS_POPUP,
            0, 0, 1920, 1080,
            HWND::default(),
            None,
            None,
            None
        );

        SetLayeredWindowAttributes(overlay_hwnd, 0, 255, 0x00000002); // LWA_ALPHA
        ShowWindow(overlay_hwnd, SW_SHOW);

        let mut thumb_handle = 0;
        let res = DwmRegisterThumbnail(overlay_hwnd, chrome_hwnd, &mut thumb_handle);
        if res.is_err() {
            println!("DwmRegisterThumbnail failed");
            return;
        }

        let mut rect = RECT::default();
        let _ = GetWindowRect(chrome_hwnd, &mut rect);
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;

        let mut props = DWM_THUMBNAIL_PROPERTIES::default();
        props.dwFlags = DWM_TNP_VISIBLE | DWM_TNP_RECTDESTINATION | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY;
        props.opacity = 255;
        props.fVisible = true.into();
        props.fSourceClientAreaOnly = false.into();

        let start = Instant::now();
        println!("Moving thumbnail in Rust...");
        while start.elapsed().as_secs_f32() < 2.0 {
            let elapsed = start.elapsed().as_secs_f32();
            let offset = (elapsed * std::f32::consts::PI * 2.0).sin() * 200.0;
            props.rcDestination = RECT {
                left: offset as i32,
                top: 0,
                right: (offset as i32) + w,
                bottom: h,
            };
            let _ = DwmUpdateThumbnailProperties(thumb_handle, &props);
            sleep(Duration::from_millis(7)); // ~144Hz
        }

        let _ = DwmUnregisterThumbnail(thumb_handle);
        let _ = DestroyWindow(overlay_hwnd);
        println!("Done");
    }
}
