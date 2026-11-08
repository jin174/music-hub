"""Music Hub 独立桌面灵动岛悬浮挂件

采用 pywebview (基于系统原生 Edge WebView2) 实现极致微挂件：
- 严格单例运行（防止多开）
- 精准尺寸与原生透明裁剪（消除任何白色/矩形底框）
- 置顶悬浮、支持全区域拖拽
"""
import ctypes
from ctypes import wintypes
import logging
import os
import sys
import threading
import time

log = logging.getLogger("mini_window")

MUTEX_NAME = "Global\\MusicHub_Mini_Widget_SingleInstance_Mutex"
WINDOW_TITLE = "Music Hub · 动态灵动岛"

class MiniApi:
    def __init__(self, window):
        self._window = window

    def close(self):
        try:
            self._window.destroy()
        except Exception:
            pass

def ensure_single_instance():
    """保证挂件严格单例，防止重复开启多个挂件窗口"""
    kernel32 = ctypes.windll.kernel32
    user32 = ctypes.windll.user32
    mutex = kernel32.CreateMutexW(None, False, MUTEX_NAME)
    last_err = kernel32.GetLastError()
    if last_err == 183:  # ERROR_ALREADY_EXISTS
        # 挂件已经开启，找到旧窗口并激活到最前排
        hwnd = user32.FindWindowW(None, WINDOW_TITLE)
        if hwnd:
            user32.SetForegroundWindow(hwnd)
        return False, mutex
    return True, mutex

def open_mini_gui(port=8765):
    is_first, mutex = ensure_single_instance()
    if not is_first:
        log.info("mini widget already running, focused existing window")
        return

    try:
        import webview
    except ImportError:
        import webbrowser
        webbrowser.open(f"http://127.0.0.1:{port}/mini")
        return

    url = f"http://127.0.0.1:{port}/mini"

    # 精确匹配胶囊胶囊尺寸：宽 380，高 52
    width = 380
    height = 52

    # 预设穿透底色 (1, 1, 1)
    KEY_HEX = "#010101"

    window = webview.create_window(
        title=WINDOW_TITLE,
        url=url,
        width=width,
        height=height,
        frameless=True,       # 无任何标题栏
        on_top=True,          # 置顶桌面最前
        transparent=False,    # 关闭 pywebview 默认的虚假透明（避免白底 Form 显露）
        resizable=False,      # 固定尺寸
        easy_drag=True,       # 允许任意位置拖拽
        shadow=False,         # 必须关闭 Windows 矩形投影，由 CSS 自己绘制药丸柔和阴影
        background_color=KEY_HEX,
    )

    api = MiniApi(window)
    window.js_api = api

    # 后台线程：当 WinForms 窗体创建后，将其 BackColor 与 TransparencyKey 绑定为 KEY_HEX
    # 这样操作系统级 DWM/GDI 会把所有非胶囊的空白像素 100% 抠除穿透，彻底根除白色矩形底框！
    def hook_transparency():
        try:
            import clr
            clr.AddReference('System.Windows.Forms')
            clr.AddReference('System.Drawing')
            from System.Drawing import Color
            import webview.platforms.winforms as wf

            for _ in range(40):
                if 'master' in wf.BrowserView.instances:
                    form = wf.BrowserView.instances['master']
                    def _apply():
                        k = Color.FromArgb(255, 1, 1, 1)
                        form.BackColor = k
                        form.TransparencyKey = k
                    form.Invoke(clr.System.Action(_apply))
                    break
                time.sleep(0.05)
        except Exception as e:
            log.warning("hook transparency failed: %s", e)

    t = threading.Thread(target=hook_transparency, daemon=True)
    t.start()

    # 启动原生 WebView2 事件循环
    webview.start(gui="edgechromium", debug=False)

if __name__ == "__main__":
    port = 8765
    for arg in sys.argv:
        if arg.startswith("--port="):
            try:
                port = int(arg.split("=")[1])
            except ValueError:
                pass
    open_mini_gui(port)
