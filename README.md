# 🎵 私人音乐中枢 · Music Hub (Rust 原生重构版 v2.0)

一个专为 **Windows 多播放器 (网易云音乐 / QQ音乐 / 汽水 / 酷狗 / 酷我 / Apple Music / Spotify 等) + Last.fm** 打造的原生桌面音乐中枢与动态灵动岛挂件。

本项目已完成从 Python 到 **纯 Rust 原生架构** 的全量重构，体积仅 **6.9 MB**，冷启动 **< 10ms**，运行时内存从 150MB 骤降至 **~10MB**，并采用 **Win32 Layered Window 原生亚像素级真透明渲染**，彻底消灭了以往基于网页套壳（WebView2/CEF/Electron）的所有白边、黑框与拖拽卡顿缺陷！

---

## ✨ 核心特性

1. **极致原生与真透明灵动岛桌面挂件**
   - **彻底告别浏览器套壳**：放弃 pywebview / WebView2，直接基于 Windows 核心 **`UpdateLayeredWindow` + `tiny-skia` + `fontdue`** 绘制 32-bit premultiplied ARGB 亚像素级透明画布。
   - **真正零瑕疵悬浮**：完美平滑抗锯齿圆角、柔和外发光投影，在任何深色/浅色/动漫壁纸上均无任何白色底框或黑边残留。
   - **144Hz 系统级平滑拖拽**：通过操作系统原生 `WM_NCLBUTTONDOWN` / `HTCAPTION` 硬件加速拖动，跟手毫无延迟。
   - **精致微交互**：内置黑胶旋转封面、4 柱式荧光绿律动音频动画、实时进度条、👍 喜欢 / 🚫 避雷 / ▶ 客户端播放 / ⤢ 放大控制台 / ✕ 隐藏挂件。

2. **攻克国内音乐播放器 SMTC 的三大痛点**
   - **补全真实时长与专辑**：网易云向 Windows SMTC 汇报的时长恒为 0，本程序通过网易云官方接口自动反查歌曲 ID、精确时长、专辑名与高清封面。
   - **精准识别单曲循环**：单曲循环时不触发系统换歌事件，状态机自动按真实时长分割每一遍播放并独立打卡。
   - **QQ 音乐窗口标题兜底**：当播放器未开启系统 SMTC 时，自动通过 Win32 零开销窗口标题捕获当前曲目。
   - **过滤视频流与非正式音频**：自动拦截视频内部编号流（如 `811514297-1-16`）。

3. **比 Last.fm 更懂你的本地 SQLite 听歌库**
   - 自动计算 **完整听完率 (≥85%)** 与 **切歌跳过率 (<30%)**。
   - 严格保留原汁原味的版本区分（`Live` / `Acoustic` / `伴奏` 各自独立统计）；仅在跨库推荐时智能回退剥离 `(Remaster)` / `(Soundtrack)` 等纯发行标注。
   - 内置 **常听歌手 Top 10**、**24 小时听歌生物钟**、**真爱榜 vs 避雷榜**，支持单条记录手动补打卡或删除。

4. **双引擎相似雷达 & 专属全盘漫游**
   - **单曲雷达**：同时并发拉取 **Last.fm 全球协同过滤 (`track.getSimilar`)** 与 **网易云相似推荐 (`simiSong`)**，结合本地历史（自动排重已听、对常跳过的歌大幅降权）。
   - **专属全盘漫游**：从你本地听完率最高的曲目与 Last.fm 常听榜单中抽取灵感种子（支持 **🎲 随机换一批种子**），一键合成 50 首从未听过的同频新歌。
   - **客户端零弹窗直播**：由本地后端直接唤起 `orpheus://` 协议，在网易云客户端内秒开播放。

5. **推荐闭环自我验证系统**
   - **命中率自动追踪**：自动记录推荐给你的曲目，并在后台持续监测后续播放行为——是听完了（完播率 ≥ 85%）还是切歌了（< 30%）。
   - **闭环看板**：在控制台呈现「试听转化率」与「完播满意度」，推荐质量一目了然。
   - **👍 喜欢加权与 🚫 避雷黑名单**：在推荐卡片上一键表态，不想听的歌永久屏蔽过滤，喜欢的歌优先吸纳为未来漫游种子。

6. **数据资产主权与一键备份**
   - **CSV 导出**：一键导出带 UTF-8 BOM 的完整播放流水账（Windows Excel 双击打开绝不乱码）。
   - **数据库备份**：一键下载完整的 SQLite `hub.db` 数据库备份文件，换电脑或重装系统无缝还原。

---

## 🚀 运行与构建

### 直接运行单文件版
1. 双击根目录下的 `MusicHub.exe`（仅约 6.9MB，无任何外部运行库依赖）。
2. 右下角系统托盘会出现音符图标 `🎵`，灵动岛悬浮挂件与 Web 控制台均已就绪。
3. 浏览器控制台：**http://127.0.0.1:8765**。

### 命令行参数
- `MusicHub.exe --mini`：启动并直接显示桌面灵动岛悬浮挂件。
- `MusicHub.exe --no-browser`：纯静默启动，不自动弹出浏览器。
- `MusicHub.exe --port=8765`：指定本地服务端口。

### 从源码编译 (Rust 1.80+)
```bash
cargo build --release
# 生成的单文件二进制位于 target/release/MusicHub.exe
```

---

## 📂 架构目录

```text
music-hub/
├── Cargo.toml         # 依赖与发布优化（LTO、体积裁切、Win32 API）
├── build.rs           # 编译期自动嵌入 Windows 图标与元信息
├── src/
│   ├── main.rs        # 单实例 Mutex、CLI 参数解析、Tokio 运行时
│   ├── config.rs      # data/config.json 线程安全配置与原子写入
│   ├── db.rs          # SQLite WAL 模式，听歌/推荐/反馈表与统计查询
│   ├── normalize.rs   # Unicode 字符清洗、歌手别名拆分、版本剥离
│   ├── ncm.rs         # 网易云音乐搜索、详情、相似与客户端唤起
│   ├── lastfm.rs      # Last.fm MD5 签名授权、Scrobble 打卡与协同过滤
│   ├── scrobbler.rs   # 后台常驻离线打卡队列（14 天超期保护）
│   ├── monitor.rs     # Windows SMTC 监听状态机 + QQ 音乐窗口兜底
│   ├── server.rs      # Axum 高性能 HTTP 服务，内嵌 index.html
│   ├── tray.rs        # Win32 原生托盘图标 (Shell_NotifyIconW)
│   └── widget.rs      # 原生 Win32 灵动岛悬浮挂件 (UpdateLayeredWindow + tiny-skia)
├── web/
│   ├── index.html     # 控制台单文件前端（已编译期嵌入）
│   └── mini.html      # 备用 Web 挂件网页
└── legacy_python/     # 历史 Python 代码归档（已打 Git 标签 python-legacy-v1.2）
```

## 💡 灵感来源与致谢 (Inspiration & Acknowledgements)

本项目的诞生直接启发自开源项目 **[dagkan/lastfm-scrobbler](https://github.com/dagkan/lastfm-scrobbler)**：
- 最初在使用 `dagkan/lastfm-scrobbler` 时，其利用 **Windows SMTC（系统级媒体传输控制）** 无需注入客户端即可无感抓取播放状态并同步至 Last.fm 的优雅思路，给了本项目最核心的启发。
- 在此灵感基础上，**Music Hub** 进一步针对国内 **网易云音乐 Windows 客户端** 的特性重新设计实现了整套 Python 状态机（自动反查补全 SMTC 缺失的歌曲时长与专辑封面、单曲循环切分、视频流过滤），并将“单纯的后台打卡器”升级为集 **本地 SQLite 听歌/切歌行为库 + Last.fm & 网易云双源相似雷达 + 专属漫游** 于一体的私人音乐中枢。

感谢 [@dagkan](https://github.com/dagkan) 以及 [Last.fm](https://www.last.fm/) 社区对开放音乐生态的贡献！

---

## 📄 开源协议

本项目基于 [MIT License](./LICENSE) 开源。所有个人配置与听歌数据库均保存在本地 `data/` 目录下（已被 `.gitignore` 排除，不会上传）。
