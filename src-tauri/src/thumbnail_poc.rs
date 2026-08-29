use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, FindWindowW, GetWindowRect, SetLayeredWindowAttributes,
    ShowWindow, LWA_ALPHA, SW_SHOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::PCWSTR;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn to_pcwstr(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn main() {
    unsafe {
        let class_name = to_pcwstr("Chrome_WidgetWin_1");
        let chrome_hwnd = match FindWindowW(PCWSTR(class_name.as_ptr()), PCWSTR(std::ptr::null())) {
            Ok(h) if !h.is_invalid() => h,
            _ => {
                println!("Chrome not found");
                return;
            }
        };

        let overlay_class = to_pcwstr("STATIC");
        let overlay_hwnd = match CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
            PCWSTR(overlay_class.as_ptr()),
            PCWSTR(to_pcwstr("Overlay").as_ptr()),
            WS_POPUP,
            0, 0, 1920, 1080,
            Some(HWND::default()),
            None,
            None,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                println!("CreateWindowExW failed: {:?}", e);
                return;
            }
        };

        let _ = SetLayeredWindowAttributes(overlay_hwnd, COLORREF(0), 255, LWA_ALPHA);
        let _ = ShowWindow(overlay_hwnd, SW_SHOW);

        let thumb_handle = match DwmRegisterThumbnail(overlay_hwnd, chrome_hwnd) {
            Ok(h) => h,
            Err(e) => {
                println!("DwmRegisterThumbnail failed: {:?}", e);
                let _ = DestroyWindow(overlay_hwnd);
                return;
            }
        };

        let mut rect = RECT::default();
        let _ = GetWindowRect(chrome_hwnd, &mut rect);
        let w = rect.right - rect.left;

        // Test DWMWA_CLOAK (13)
        println!("Testing DWMWA_CLOAK (13)...");
        let cloak: windows::core::BOOL = true.into();
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            chrome_hwnd,
            windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(13),
            &cloak as *const _ as *const _,
            std::mem::size_of::<windows::core::BOOL>() as u32,
        );

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
                left: (rect.left as f32 + offset) as i32,
                top: rect.top,
                right: (rect.left as f32 + offset) as i32 + w,
                bottom: rect.bottom,
            };
            let _ = DwmUpdateThumbnailProperties(thumb_handle, &props);
            sleep(Duration::from_millis(7)); // ~144Hz
        }

        let uncloak: windows::core::BOOL = false.into();
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            chrome_hwnd,
            windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(13),
            &uncloak as *const _ as *const _,
            std::mem::size_of::<windows::core::BOOL>() as u32,
        );

        let _ = DwmUnregisterThumbnail(thumb_handle);
        let _ = DestroyWindow(overlay_hwnd);
        println!("Done");
    }
}

