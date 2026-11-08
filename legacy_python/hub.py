"""私人音乐中枢 Music Hub —— 入口

用法：
  pythonw hub.py            托盘模式（无黑窗口）
  python  hub.py --console  控制台模式（调试用，日志直接打印）
"""
import logging
import os
import sys
import threading
import webbrowser
from logging.handlers import RotatingFileHandler

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# PyInstaller 单文件打包环境下，必须在任何 clr/webview 导入之前显式加载 Python.NET 运行时
if getattr(sys, "frozen", False):
    try:
        from pythonnet import load
        load("netfx")
    except Exception:
        pass

from config import DATA_DIR, Config  # noqa: E402
from db import DB  # noqa: E402


def setup_logging(console):
    handlers = [RotatingFileHandler(os.path.join(DATA_DIR, "hub.log"), maxBytes=1_000_000,
                                    backupCount=2, encoding="utf-8")]
    if console and sys.stderr:
        handlers.append(logging.StreamHandler())
    logging.basicConfig(level=logging.INFO, handlers=handlers,
                        format="%(asctime)s %(levelname)s %(name)s: %(message)s")

    def handle_exc(exc_type, exc_value, exc_traceback):
        logging.critical("Uncaught exception", exc_info=(exc_type, exc_value, exc_traceback))

    sys.excepthook = handle_exc


def main():
    if "--mini" in sys.argv:
        from mini_window import open_mini_gui
        port = 8765
        for arg in sys.argv:
            if arg.startswith("--port="):
                try:
                    port = int(arg.split("=")[1])
                except ValueError:
                    pass
        open_mini_gui(port)
        return

    console = "--console" in sys.argv
    setup_logging(console)
    log = logging.getLogger("hub")

    cfg = Config()
    db = DB()

    from app import App
    from server import make_server
    app = App(cfg, db)

    try:
        httpd = make_server(app, cfg.get("port"))
    except OSError:
        # 端口被占用 = 已经在运行了，直接打开控制台
        log.info("already running, opening UI")
        webbrowser.open(app.base_url)
        return

    threading.Thread(target=httpd.serve_forever, daemon=True, name="http").start()
    mon_thread = threading.Thread(target=app.monitor.run, daemon=True, name="monitor")
    mon_thread.start()
    threading.Thread(target=app.scrobbler.run, daemon=True, name="scrobbler").start()
    log.info("Music Hub running at %s (mode=%s)", app.base_url, cfg.get("mode"))

    if "--no-browser" not in sys.argv:
        webbrowser.open(app.base_url)

    def shutdown():
        log.info("shutting down")
        app.monitor.stop()
        mon_thread.join(timeout=5)  # 等当前这首结算入库
        app.scrobbler.stop()
        httpd.shutdown()

    if console:
        print(f"Music Hub running at {app.base_url}  (Ctrl+C 退出)")
        try:
            threading.Event().wait()
        except KeyboardInterrupt:
            pass
        shutdown()
    else:
        from tray import run_tray
        run_tray(app, shutdown)


if __name__ == "__main__":
    main()
