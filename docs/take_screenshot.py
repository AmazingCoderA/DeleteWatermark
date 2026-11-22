"""Снимает скриншот десктопного GUI (не телефона) для README."""
import subprocess
import time
from pathlib import Path

import pygetwindow as gw
from PIL import Image, ImageGrab

root = Path(__file__).parent.parent
exe = root / "target" / "release" / "delete-watermark.exe"
out = root / "docs"
out.mkdir(exist_ok=True)

# Убираем остальные окна с экрана, иначе GUI оказывается под браузером.
minimized = []
for w in gw.getAllWindows():
    if w.title and w.visible:
        try:
            w.minimize()
            minimized.append(w)
        except Exception:
            pass

proc = subprocess.Popen([str(exe), "--mode", "gui"])
try:
    time.sleep(5)
    wins = [w for w in gw.getAllWindows() if w.title.strip() == "Удалитель водяных знаков v8.0"]
    if not wins:
        raise RuntimeError(f"GUI window not found. Windows: {[w.title for w in gw.getAllWindows()][:20]}")
    win = wins[0]
    print("window:", win.title, win.left, win.top, win.width, win.height)
    try:
        import ctypes
        hwnd = win._hWnd
        ctypes.windll.user32.ShowWindow(hwnd, 9)  # SW_RESTORE
        ctypes.windll.user32.SetForegroundWindow(hwnd)
        ctypes.windll.user32.BringWindowToTop(hwnd)
    except Exception as e:
        print("focus warning:", e)
    time.sleep(1.0)

    import ctypes
    for _ in range(12):
        if ctypes.windll.user32.GetForegroundWindow() == win._hWnd:
            break
        __import__("pyautogui").hotkey("alt", "tab")
        time.sleep(0.35)
    time.sleep(1.0)

    # PrintWindow снимает само окно, даже если оно прикрыто другими окнами.
    import win32gui
    import win32ui

    hwnd = win._hWnd
    left, top, right, bottom = win32gui.GetWindowRect(hwnd)
    w, h = right - left, bottom - top
    hwnd_dc = win32gui.GetWindowDC(hwnd)
    src_dc = win32ui.CreateDCFromHandle(hwnd_dc)
    mem_dc = src_dc.CreateCompatibleDC()
    bmp = win32ui.CreateBitmap()
    bmp.CreateCompatibleBitmap(src_dc, w, h)
    mem_dc.SelectObject(bmp)
    import ctypes
    ctypes.windll.user32.PrintWindow(hwnd, mem_dc.GetSafeHdc(), 2)
    info = bmp.GetInfo()
    shot = Image.frombuffer(
        "RGB",
        (info["bmWidth"], info["bmHeight"]),
        bmp.GetBitmapBits(True),
        "raw",
        "BGRX",
        0,
        1,
    ).convert("RGB")
    mem_dc.DeleteDC()
    src_dc.DeleteDC()
    win32gui.ReleaseDC(hwnd, hwnd_dc)
    win32gui.DeleteObject(bmp.GetHandle())

    # Лёгкая рамка, чтобы скрин выглядел аккуратно на белом фоне README.
    framed = Image.new("RGB", (shot.width + 2, shot.height + 2), (102, 126, 234))
    framed.paste(shot, (1, 1))
    target = out / "screenshot.png"
    framed.save(target, optimize=True)
    print(f"saved {target} {framed.size}")
finally:
    for w in minimized:
        try:
            w.restore()
        except Exception:
            pass
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()
