"""配置读写：data/config.json（只保存在本机）"""
import json
import os
import sys
import threading

if getattr(sys, "frozen", False):
    # PyInstaller 打包成单文件 .exe 后：
    # - BASE_DIR 指向 .exe 所在目录（让 data/ 数据库与配置持久保存在 .exe 旁边，绝不随临时目录清空）
    # - BUNDLE_DIR 指向 PyInstaller 解包内置资源的目录（包含打包进去的 web/index.html）
    BASE_DIR = os.path.dirname(os.path.abspath(sys.executable))
    BUNDLE_DIR = getattr(sys, "_MEIPASS", BASE_DIR)
else:
    BASE_DIR = os.path.dirname(os.path.abspath(__file__))
    BUNDLE_DIR = BASE_DIR

DATA_DIR = os.path.join(BASE_DIR, "data")
# 如果 .exe 旁边放了外置 web/ 目录则优先用外置的，否则用打包内置的
WEB_DIR = (
    os.path.join(BASE_DIR, "web")
    if os.path.isdir(os.path.join(BASE_DIR, "web"))
    else os.path.join(BUNDLE_DIR, "web")
)
os.makedirs(DATA_DIR, exist_ok=True)

CONFIG_PATH = os.path.join(DATA_DIR, "config.json")

# 未配置个人 API Key 时，只读接口（相似歌曲等）使用原雷达页面里的公共 Key
FALLBACK_READ_KEY = "b25b959554ed76058ac220b7b2e0a026"

DEFAULT_APPS = [
    "cloudmusic.exe",
    "qqmusic.exe",
    "sodamusic.exe",
    "qishui",
    "musicfree",
    "kugou",
    "kwmusic",
    "yesplaymusic",
    "listen1",
    "applemusic",
    "spotify",
    "foobar2000",
]

DEFAULTS = {
    "api_key": "",
    "api_secret": "",
    "session_key": "",
    "username": "",
    # record = 只记录到本地，不打卡；scrobble = 记录 + 打卡到 Last.fm
    "mode": "record",
    "port": 8765,
    # 监听这些播放器（SMTC 的 AppUserModelId 或窗口兜底）
    "apps": list(DEFAULT_APPS),
    # 未在网易云正式曲库匹配到的声音（如播客/B站填词/电台），是否也打卡到 Last.fm
    "scrobble_unmatched": False,
}


class Config:
    def __init__(self):
        self._lock = threading.Lock()
        self.data = dict(DEFAULTS)
        if os.path.exists(CONFIG_PATH):
            try:
                with open(CONFIG_PATH, "r", encoding="utf-8") as f:
                    loaded = json.load(f)
                # 自动将早期默认仅含 cloudmusic.exe 的配置升级为多播放器默认列表
                if loaded.get("apps") == ["cloudmusic.exe"]:
                    loaded["apps"] = list(DEFAULT_APPS)
                self.data.update(loaded)
            except Exception:
                pass
        self.save()

    def get(self, key, default=None):
        if default is not None:
            return self.data.get(key, default)
        return self.data.get(key, DEFAULTS.get(key))

    def update(self, **kwargs):
        with self._lock:
            self.data.update(kwargs)
            self._save_locked()

    def save(self):
        with self._lock:
            self._save_locked()

    def _save_locked(self):
        tmp = CONFIG_PATH + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(self.data, f, ensure_ascii=False, indent=2)
        os.replace(tmp, CONFIG_PATH)

    @property
    def read_key(self):
        return self.get("api_key") or FALLBACK_READ_KEY

    @property
    def has_credentials(self):
        return bool(self.get("api_key") and self.get("api_secret"))

    @property
    def authorized(self):
        return self.has_credentials and bool(self.get("session_key"))

    @property
    def scrobbling(self):
        return self.get("mode") == "scrobble" and self.authorized

    def public(self):
        """给前端看的配置（不含 secret / session key）"""
        return {
            "api_key_set": bool(self.get("api_key")),
            "api_secret_set": bool(self.get("api_secret")),
            "authorized": self.authorized,
            "username": self.get("username"),
            "mode": self.get("mode"),
            "port": self.get("port"),
            "apps": self.get("apps"),
            "scrobble_unmatched": bool(self.get("scrobble_unmatched")),
        }
