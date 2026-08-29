import ctypes
import ctypes.wintypes
import time
import math

user32 = ctypes.windll.user32
dwmapi = ctypes.windll.dwmapi

class DWM_THUMBNAIL_PROPERTIES(ctypes.Structure):
    _fields_ = [
        ("dwFlags", ctypes.c_uint32),
        ("rcDestination", ctypes.wintypes.RECT),
        ("rcSource", ctypes.wintypes.RECT),
        ("opacity", ctypes.c_uint8),
        ("fVisible", ctypes.wintypes.BOOL),
        ("fSourceClientAreaOnly", ctypes.wintypes.BOOL),
    ]

DWM_TNP_RECTDESTINATION = 0x00000001
DWM_TNP_RECTSOURCE = 0x00000002
DWM_TNP_OPACITY = 0x00000004
DWM_TNP_VISIBLE = 0x00000008
DWM_TNP_SOURCECLIENTAREAONLY = 0x00000010

def get_chrome_hwnd():
    return user32.FindWindowW("Chrome_WidgetWin_1", None)

def main():
    chrome_hwnd = get_chrome_hwnd()
    if not chrome_hwnd:
        print("Chrome not found")
        return

    # Create our overlay window
    overlay_hwnd = user32.CreateWindowExW(
        0x00080000 | 0x00000020 | 0x00000080, # WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW
        "STATIC", "Overlay",
        0x80000000, # WS_POPUP
        0, 0, 1920, 1080,
        None, None, None, None
    )
    user32.SetLayeredWindowAttributes(overlay_hwnd, 0, 255, 0x00000002) # LWA_ALPHA
    user32.ShowWindow(overlay_hwnd, 5)

    thumb_handle = ctypes.c_void_p()
    res = dwmapi.DwmRegisterThumbnail(overlay_hwnd, chrome_hwnd, ctypes.byref(thumb_handle))
    if res != 0:
        print("DwmRegisterThumbnail failed")
        return

    rect = ctypes.wintypes.RECT()
    user32.GetWindowRect(chrome_hwnd, ctypes.byref(rect))
    w, h = rect.right - rect.left, rect.bottom - rect.top

    props = DWM_THUMBNAIL_PROPERTIES()
    props.dwFlags = DWM_TNP_VISIBLE | DWM_TNP_RECTDESTINATION | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY
    props.opacity = 255
    props.fVisible = True
    props.fSourceClientAreaOnly = False

    # Hide chrome temporarily to see just the thumbnail?
    # Actually, let's just move the thumbnail
    start = time.perf_counter()
    duration = 2.0
    
    print("Moving thumbnail...")
    while True:
        now = time.perf_counter()
        elapsed = now - start
        if elapsed > duration:
            break
            
        offset = int(math.sin(elapsed * math.pi * 2) * 200)
        props.rcDestination = ctypes.wintypes.RECT(offset, 0, offset + w, h)
        dwmapi.DwmUpdateThumbnailProperties(thumb_handle, ctypes.byref(props))
        time.sleep(1/144.0)

    dwmapi.DwmUnregisterThumbnail(thumb_handle)
    user32.DestroyWindow(overlay_hwnd)
    print("Done")

if __name__ == "__main__":
    main()
