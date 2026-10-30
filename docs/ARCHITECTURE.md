# DFL 架构设计

轻量级 Minecraft Java 版启动器（Windows 优先，单 exe，无第三方运行时）。

## 1. 技术选型

| 层 | 选择 | 理由 |
|---|---|---|
| 语言 | Rust 2021 | 单文件静态链接、无 GC、启动快、内存占用低；错误处理与并发模型适合网络/文件密集型任务 |
| UI | egui 0.32 (eframe/glow) | Immediate-mode，代码量小；OpenGL 由系统提供，不引入 WebView/Chromium；空闲时零重绘 |
| 网络 | reqwest (rustls) + 线程池 | 不需要 OpenSSL DLL；支持镜像切换、断点续传、并发下载 |
| 序列化 | serde / serde_json | 直接映射 Mojang 的 version JSON / manifest |
| 打包 | `cargo build --release` | `opt-level=z + lto + strip + panic=abort`，产物约 3 MB，双击即用 |

对比：
- **C#/WPF**：本机无 .NET SDK，且 self-contained WPF 体积 60MB+，违背“轻量”；
- **Electron/Tauri**：内存 80–200MB，且依赖 WebView2；
- **Qt/C++**：需要携带 Qt DLL，单文件困难；
- **Win32 C**：体积最小，但 JSON/HTTPS/ZIP 全部手写，维护成本高。

## 2. 目录结构

```
dfl/
├── Cargo.toml
├── src/
│   ├── main.rs          # 入口、窗口参数
│   ├── app.rs           # 界面：侧边栏 + 页面路由 + 主题/字体
│   ├── config.rs        # 配置（JSON，便携式 data/ 目录）
│   ├── util.rs          # 路径、目录初始化
│   ├── net/             # 后续：HTTP 客户端、镜像、断点续传
│   ├── meta/            # 后续：version_manifest、libraries、rules、assets
│   ├── auth/            # 后续：离线 / 微软 OAuth2 / Yggdrasil
│   ├── launch/          # 后续：classpath、natives、JVM 参数、进程
│   ├── java/            # 后续：Java 探测与下载
│   └── content/         # 后续：mods / resourcepacks / shaderpacks
├── docs/
└── data/                # 运行时生成：config.json、versions/、libraries/、assets/、instances/
```

## 3. 模块与职责

| 模块 | 职责 | 状态 |
|---|---|---|
| `app` | 页面路由（首页/版本/实例/资源/账号/设置/关于）、主题、字体 | ✅ 已完成 |
| `config` | 读写 `data/config.json`，镜像选择、内存、JVM 参数 | ✅ 已完成 |
| `util` | 便携式数据目录探测、目录初始化 | ✅ 已完成 |
| `net` | BMCLAPI/官方镜像、下载、SHA1 校验、重试与回退 | ✅ |
| `meta` | 版本清单、版本 JSON 解析、依赖库与 rules、资源索引 | ✅ 基础 |
| `auth` | 离线 UUID、微软设备码登录、外置登录 | ⏳ |
| `java` | 扫描注册表/常见路径、`java -version` 解析、Adoptium 下载 | ⏳ |
| `launch` | 组装 classpath/natives/JVM 参数、启动并捕获日志 | ⏳ |
| `content` | 模组/资源包/光影扫描、启用禁用、下载安装 | ⏳ |

## 4. 底层原理

### 4.1 版本获取
1. `GET {mirror}/mc/game/version_manifest_v2.json` → 版本列表（id、type、url、sha1）。
2. 下载所选版本的 `{id}.json`，其中包含 `mainClass`、`libraries`、`assetIndex`、`downloads.client`、`arguments`。
3. 依 `libraries[].rules` 过滤当前系统（Windows x64），把 Maven 坐标转为路径
   `group/artifact/version/artifact-version.jar`；`natives` 中的 classifier（如 `natives-windows`）解压到 `versions/{id}/natives`。
4. 下载 `client.jar`、全部库、`assets`（objects 哈希分片目录）并逐个校验 SHA1。

### 4.2 启动参数
- `-Djava.library.path=...natives`
- `-cp <libraries + client.jar 以 ; 连接>`
- 主类 `mainClass`，随后拼接 `arguments.game`（替换 `${auth_player_name}`、`${version_name}`、`${game_directory}`、`${assets_root}`、`${assets_index_name}`、`${auth_uuid}`、`${auth_access_token}`、`${user_type}` 等占位符）。
- 内存：`-Xms{min}M -Xmx{max}M`；追加用户 JVM 参数。
- 进程使用 `CREATE_NO_WINDOW`，stdout/stderr 写入 `logs/latest.log`。

### 4.3 版本隔离
每个实例目录 `instances/{name}/` 自带 `mods/`、`saves/`、`config/`、`resourcepacks/`、`shaderpacks/`，
启动时 `--gameDir` 指向实例目录，`--assetsDir` 与 `--versionType` 共享全局资源，避免重复下载。

### 4.4 账号
- 离线：`UUID = md5("OfflinePlayer:" + name)` 并格式化为带连字符的 UUID，`accessToken` 置空。
- 微软：OAuth2 设备码 → XBL → XSTS → Minecraft services → profile（UUID + name）。
- 外置：Yggdrasil `authenticate` 接口。

### 4.5 国内镜像
默认 `https://bmclapi2.bangbang93.com`，映射规则：版本清单/版本 JSON/库/资源均替换官方域名；
可在设置中切换官方或自定义地址。

## 5. 性能目标

- 冷启动 < 1.5s（字体加载约 100ms，首次绘制即完成）
- 空闲 CPU ≈ 0%，内存 < 60MB
- 单 exe < 8MB，无 DLL 依赖
