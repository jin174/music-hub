"""一键将 Music Hub 打包为免安装单文件 MusicHub.exe（内置图标与网页资源）"""
import os
import subprocess
import sys

from PIL import Image, ImageDraw

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
ICO_PATH = os.path.join(BASE_DIR, "icon.ico")


def make_ico():
    img = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.ellipse((8, 8, 248, 248), fill=(232, 65, 66, 255))
    d.ellipse((64, 152, 128, 208), fill="white")
    d.rectangle((112, 56, 128, 184), fill="white")
    d.polygon([(128, 56), (192, 80), (192, 108), (128, 88)], fill="white")
    img.save(ICO_PATH, format="ICO", sizes=[(256, 256), (64, 64), (32, 32), (16, 16)])


def main():
    make_ico()
    cmd = [
        sys.executable, "-m", "PyInstaller",
        "--noconfirm",
        "--clean",
        "--onefile",
        "--windowed",
        "--name", "MusicHub",
        "--icon", ICO_PATH,
        "--add-data", f"{os.path.join(BASE_DIR, 'web')};web",
        "--hidden-import", "pystray._win32",
        "--exclude-module", "numpy",
        "--exclude-module", "scipy",
        "--exclude-module", "pandas",
        "--exclude-module", "matplotlib",
        "--exclude-module", "tkinter",
        os.path.join(BASE_DIR, "hub.py"),
    ]
    print("Building MusicHub.exe ...")
    subprocess.check_call(cmd, cwd=BASE_DIR)
    print("\n[OK] Build complete:", os.path.join(BASE_DIR, "dist", "MusicHub.exe"))


if __name__ == "__main__":
    main()
