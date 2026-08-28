use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetForegroundWindow};
use windows::core::PCWSTR;

fn main() {
    unsafe {
        let hwnd = GetForegroundWindow();
        let cloak: u32 = 1;
        let res = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK,
            &cloak as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );
        println!("Cloak result: {:?}", res);
    }
}
