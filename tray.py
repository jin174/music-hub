"""托盘图标 + 开机自启"""
import logging
import os
import sys
import threading
import time
import webbrowser

log = logging.getLogger("tray")

STARTUP_DIR = os.path.join(os.environ.get("APPDATA", ""), r"Microsoft\Windows\Start Menu\Programs\Startup")
STARTUP_FILE = os.path.join(STARTUP_DIR, "MusicHub.vbs")
HUB_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "hub.py")


def pythonw_path():
    exe = sys.executable
    cand = os.path.join(os.path.dirname(exe), "pythonw.exe")
    return cand if os.path.exists(cand) else exe


def launcher_vbs():
    if getattr(sys, "frozen", False):
        return f'CreateObject("WScript.Shell").Run """{sys.executable}""", 0, False\r\n'
    return (f'CreateObject("WScript.Shell").Run """{pythonw_path()}"" ""{HUB_PATH}""", 0, False\r\n')


def write_launcher(path):
    # UTF-16 带 BOM：路径里有中文时 WSH 也能正确识别
    with open(path, "w", encoding="utf-16") as f:
        f.write(launcher_vbs())


def autostart_enabled():
    return os.path.exists(STARTUP_FILE)


def set_autostart(on):
    if on:
        write_launcher(STARTUP_FILE)
    elif os.path.exists(STARTUP_FILE):
        os.remove(STARTUP_FILE)


def make_icon():
    from PIL import Image, ImageDraw
    img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.ellipse((2, 2, 62, 62), fill=(232, 65, 66, 255))
    # 一个简笔音符
    d.ellipse((16, 38, 32, 52), fill="white")
    d.rectangle((28, 14, 32, 46), fill="white")
    d.polygon([(32, 14), (48, 20), (48, 27), (32, 22)], fill="white")
    return img


def run_tray(app, on_quit):
    import pystray
    from pystray import Menu, MenuItem as Item

    def open_ui(icon=None, item=None):
        webbrowser.open(app.base_url)

    def open_mini(icon=None, item=None):
        app.open_mini_window()

    def set_mode(mode):
        def handler(icon, item):
            if mode == "scrobble" and not app.cfg.authorized:
                webbrowser.open(app.base_url + "/#settings")
                return
            app.cfg.update(mode=mode)
            if mode == "scrobble":
                app.scrobbler.kick()
        return handler

    def toggle_autostart(icon, item):
        set_autostart(not autostart_enabled())

    def quit_app(icon, item):
        icon.stop()

    menu = Menu(
        Item("打开控制台", open_ui, default=True),
        Item("打开迷你挂件", open_mini),
        Menu.SEPARATOR,
        Item("只记录（不打卡）", set_mode("record"),
             checked=lambda i: app.cfg.get("mode") == "record", radio=True),
        Item("记录 + 打卡到 Last.fm", set_mode("scrobble"),
             checked=lambda i: app.cfg.get("mode") == "scrobble", radio=True),
        Menu.SEPARATOR,
        Item("开机自启", toggle_autostart, checked=lambda i: autostart_enabled()),
        Item("退出", quit_app),
    )
    icon = pystray.Icon("MusicHub", make_icon(), "Music Hub", menu)

    def tooltip_loop():
        while icon.visible or not getattr(icon, "_stopped", False):
            try:
                now = app.monitor.snapshot()
                mode = "打卡中" if app.cfg.scrobbling else "只记录"
                text = f"Music Hub · {mode}"
                if now:
                    text += f"\n♪ {now['title']} - {now['artist']}"
                icon.title = text[:120]
            except Exception:
                pass
            time.sleep(3)

    def setup(icon):
        icon.visible = True
        threading.Thread(target=tooltip_loop, daemon=True).start()

    icon.run(setup)
    on_quit()
