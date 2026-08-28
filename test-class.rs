use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

unsafe fn get_class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = GetClassNameW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..len as usize])
}
