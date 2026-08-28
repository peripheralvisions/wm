use windows::Win32::UI::WindowsAndMessaging::{BeginDeferWindowPos, HDWP};
fn main() {
    let hdwp: HDWP = unsafe { BeginDeferWindowPos(1) };
    if hdwp.is_invalid() {}
}
