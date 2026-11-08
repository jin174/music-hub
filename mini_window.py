"""Music Hub 独立桌面灵动岛悬浮挂件

采用 pywebview (基于系统原生 Edge WebView2) 实现无边框、无地址栏、背景半透明、
置顶悬浮于桌面最上层、自由拖拽的真正桌面微挂件。
"""
import os
import sys

class MiniApi:
    def __init__(self, window):
        self._window = window

    def close(self):
        try:
            self._window.destroy()
        except Exception:
            pass

def open_mini_gui(port=8765):
    try:
        import webview
    except ImportError:
        import webbrowser
        webbrowser.open(f"http://127.0.0.1:{port}/mini")
        return

    url = f"http://127.0.0.1:{port}/mini"

    # 尺寸设定：包含药丸微量透明外边距
    width = 390
    height = 68

    window = webview.create_window(
        title="Music Hub · 动态灵动岛",
        url=url,
        width=width,
        height=height,
        frameless=True,       # 彻底移除浏览器标题栏、地址栏与边框
        on_top=True,          # 置顶悬浮，在玩游戏、敲代码、浏览网页时随时感知
        transparent=True,     # 原生透明窗口背景，仅渲染圆角灵动岛药丸本体
        resizable=False,      # 固定尺寸保持灵动岛比例
        easy_drag=True,       # 允许直接拖动挂件
        shadow=True,          # 柔和窗口阴影
        background_color="#000000",
    )

    api = MiniApi(window)
    window.js_api = api

    # 启动独立窗口
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
