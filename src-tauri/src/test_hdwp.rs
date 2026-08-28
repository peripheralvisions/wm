pub fn test_hdwp() {
    use windows::Win32::UI::WindowsAndMessaging::{BeginDeferWindowPos, HDWP};
    let hdwp: HDWP = unsafe { BeginDeferWindowPos(1) };
    if hdwp.is_invalid() {}
}
