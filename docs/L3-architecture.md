#+title: L3 Technical Architecture
#+author: SOV710
#+date: 2026-04-12
#+project: quicksov
#+depends-on: L0 Primary Context, L1 Design Language, L2 Spatial Layout & Components

# L3 Technical Architecture

本文档定义 quicksov 项目的技术架构，是 L0 上下文、L1 设计语言、L2 组件清单之后的实现层规格。所有技术决策在此收口，后续的 ADR 只补充单点决策的详细论证，不覆盖本文档的整体结构。

---

## 1. 项目命名与产物

- **项目名**：quicksov
- **Git 仓库**：`~/proj/quicksov`
- **运行时根目录**：`~/.config/quicksov/`
- **Daemon 二进制**：`qsovd`（避开 QEMU `qsd` 冲突）
- **Daemon crate 名**：`qsovd`
- **Protocol spec 文档**：`protocol/spec.md` + `protocol/schema.json`

## 2. 宿主环境约束

以下约束来自 L0，在 L3 中作为技术选型的硬性前提：

- **OS**：Gentoo Linux
- **Init**：OpenRC（**无 systemd**）
- **Compositor**：Niri（Wayland）
- **网络栈**：wpa_supplicant + dhcpcd（**无 NetworkManager、无 iwd**）
- **音频栈**：PipeWire
- **网卡**：`wlo1`（WiFi）、`enp109s0`（Ethernet）
- **屏幕**：DP-1（主，P2418D 24" 1440p @1.25x）、eDP-1（副，笔记本 16" 1600p @2x）

任何与以上约束冲突的实现选择都必须被拒绝。

## 3. 进程模型与拓扑

### 3.1 特权模型

Daemon 作为**纯 user-space 单进程**运行，不拆分 privileged helper，不依赖 setuid 或 polkit helper 二进制。所有需要特权的操作通过以下方式达成：

- **wpa_supplicant ctrl socket**：通过配置 wpa_supplicant 的 `ctrl_interface_group` 为用户所在 group，使 daemon 能直接对话
- **DDC/i2c 亮度**：通过把用户加入 `i2c` group
- **其他偶发特权操作**：预留走外部命令 + pkexec 的后门，但初版不实现

### 3.2 进程拓扑

Daemon 与 Quickshell 是**两个独立进程**，都由 Niri 启动脚本拉起。Daemon 先于 qs 启动（或至少不晚于 qs 尝试连接的时刻），它在固定路径的 Unix domain socket 上监听；qs 通过 QML 的 IPC client 主动连接该 socket。

启动顺序与责任：

```
niri startup script
    ├── exec qsovd &      (daemon 先启动)
    └── exec qs &         (qs 随后启动, 读取 QS_BASE_PATH)
```

两者解耦的好处：

- Daemon 可以在运行时重启（例如修改 service 实现后），qs 无感知，通过重连恢复
- qs 可以独立热重载 QML 代码，不影响 daemon 的状态
- 任一端崩溃不拖垮另一端

qs 侧的 IPC client 必须实现连接失败时的**指数退避重试**（起始 500ms，上限 5s），直到 daemon 就绪。

### 3.3 启动方式

Daemon 不作为 OpenRC 系统 service 注册，也不作为 user init service。它由 Niri 的启动脚本 `exec ... &` 直接拉起，生命周期绑定到 Niri 会话。Niri 退出时 daemon 随之退出（通过父进程监听 SIGHUP 或 PR_SET_PDEATHSIG）。

## 4. IPC 协议

### 4.1 传输层

- **传输**：Unix Domain Socket (SOCK_STREAM)
- **路径**：`$XDG_RUNTIME_DIR/quicksov/daemon.sock`
- **启动时**：daemon 若发现 socket 文件已存在且无进程在 listen，删除后重建；否则报错退出
- **framing**：每条消息前 4 字节小端 `u32` 表示 payload 长度，随后是对应长度的 MessagePack payload

```
┌─────────────────┬──────────────────────────┐
│  u32 LE length  │  MessagePack payload     │
│   (4 bytes)     │    (length bytes)        │
└─────────────────┴──────────────────────────┘
```

单条消息最大 16 MiB（`length` 上限由 daemon 和 qs 双侧强制，超过则视为协议错误，立即断连）。

### 4.2 会话层

连接建立后必须先完成握手。双方任一侧若在 2 秒内未收到对方首个握手消息，视为超时并断连。

```
client → server:  Hello     { proto_version, client_name, client_version }
server → client:  HelloAck  { server_version, capabilities, session_id }
```

- `proto_version`：字符串形如 `"qsov/1"`，主版本号不匹配直接拒绝
- `capabilities`：server 当前启用的 topic 列表，例如 `["battery", "net.wifi", "net.link", ...]`
- `session_id`：u64，用于日志关联

握手后所有消息走统一的 envelope 格式（见 4.3）。

### 4.3 逻辑层消息格式

所有业务消息共享顶层 envelope：

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | u64 | 消息 ID；REQ/REP 配对；PUB 与 ONESHOT 固定为 0 |
| `kind` | u8 | 0=REQ, 1=REP, 2=ERR, 3=PUB, 4=ONESHOT, 5=SUB, 6=UNSUB |
| `topic` | string | 目标 topic，层级命名（见 4.5） |
| `action` | string | REQ/ONESHOT 时必填；REP/ERR/PUB 时为空字符串 |
| `payload` | any | MessagePack 任意值，schema 由 `(topic, action)` 决定 |

#### 消息模式

**REQ / REP**：同步请求-响应。qs 发 `kind=REQ`，daemon 回 `kind=REP`（成功）或 `kind=ERR`（失败），`id` 相同。

**ONESHOT**：单向命令，无响应。用于 "fire and forget" 场景（如 "播放下一首"）。

**SUB / UNSUB**：qs 订阅或退订某个 topic。Daemon 收到 SUB 后：
1. 立即向该 session 推送一次当前状态快照（kind=PUB）
2. 注册该 session 到 topic 的推送列表
3. 后续状态变化时向所有订阅者广播 PUB

**PUB**：daemon 主动推送，不期待响应。

### 4.4 错误码

错误响应的 `payload` 是结构化对象：

```json
{
  "code": "E_TOPIC_UNKNOWN",
  "message": "unknown topic: net.mesh",
  "details": { "topic": "net.mesh" }
}
```

**标准化错误码**（大写蛇形命名，E_ 前缀）：

| Code | 含义 |
|---|---|
| `E_PROTO_VERSION` | 协议主版本不匹配 |
| `E_PROTO_MALFORMED` | 消息格式错误（framing、msgpack 解析失败、envelope 缺字段） |
| `E_HANDSHAKE_TIMEOUT` | 握手超时 |
| `E_TOPIC_UNKNOWN` | topic 不存在或未启用 |
| `E_ACTION_UNKNOWN` | topic 不支持该 action |
| `E_ACTION_PAYLOAD` | action 的 payload schema 不符 |
| `E_SERVICE_INTERNAL` | service 内部错误（D-Bus 失败、netlink 错误等） |
| `E_SERVICE_UNAVAILABLE` | service 暂时不可用（后端未启动、正在重连） |
| `E_PERMISSION` | 权限不足（无法访问 wpa ctrl socket 等） |
| `E_RATE_LIMITED` | 请求频率过高（保留） |
| `E_CANCELED` | 请求被取消（daemon 关闭等） |

`details` 字段的 schema 与具体 `code` 一一对应，在 `protocol/spec.md` 中逐个定义。

### 4.5 Topic 命名

采用**层级命名**，用 `.` 分隔。一级定义业务域，二级及以下细化子域。

| Topic | 说明 |
|---|---|
| `battery` | 电池状态、电源配置 |
| `net.link` | 网络接口状态、IP、路由（netlink 驱动） |
| `net.wifi` | WiFi 扫描、连接管理（wpa_supplicant 驱动） |
| `bluetooth` | BlueZ 设备管理 |
| `audio` | PipeWire sink/source、音量、静音 |
| `mpris` | MPRIS 播放器发现与控制 |
| `notification` | Freedesktop notification server |
| `niri` | Niri workspace、window、action |
| `weather` | Open-Meteo 天气缓存 |
| `theme` | design-tokens.toml 的内容，启动时推送，热重载时更新 |
| `meta` | daemon 自身状态（版本、uptime、service 健康）；快照包含 `screens.roles` 字段（ADR-007），供 QML 侧无硬编码地分配主/副屏职责 |

### 4.6 订阅与初始状态

订阅粒度为**整个 topic**。qs 不能订阅子字段。Daemon 在收到 `SUB` 后立即通过 `PUB` 推送完整状态快照，随后只在状态变化时推送增量快照（但格式仍是完整快照，不做 diff patch——简化协议，代价是少量带宽）。

### 4.7 Schema 维护策略

**主文档**：`protocol/spec.md`  
用 Markdown 组织，每个 topic 一节，每个 action 一段。每段内嵌一个 JSON Schema 代码块描述该 action 的 request payload 和 response payload。

**机读副本**：`protocol/schema.json`  
独立维护的 JSON 文件，作为 "default options" 和 CI 校验的数据源。内容是一个对象，顶层 key 是 topic，value 包含该 topic 的 actions、state snapshot schema、默认值等。此文件与 `spec.md` 必须保持同步，更新流程要求同一个 commit 同时改两个文件。

**职责分工**：
- Rust daemon 侧：手写对应 `serde` 结构体，以 `spec.md` 为准
- qs 侧 JS：手写对应类型定义/校验函数，以 `spec.md` 为准
- 文档变更必须在 PR description 里列明影响的 topic 和 action

**不做代码生成**。对单人项目，维护 codegen 工具链的成本大于手动同步的收益。

## 5. Daemon 内部架构

### 5.1 并发运行时

**Tokio**。所有 I/O 走 async，包括 UDS listener、UDS session、D-Bus (`zbus`)、netlink (`rtnetlink`)、HTTP (`reqwest`)、文件 inotify (`notify`)。

### 5.2 Actor 式 Service 模型

Daemon 不定义 `trait Service`。每个 service 是一个 **tokio task**，对外只暴露一个具体结构体 `ServiceHandle`。Router 持有 `HashMap<String, ServiceHandle>`，所有通信走 channel，无共享内存。

#### ServiceHandle 的组成

```
ServiceHandle {
    request_tx:  mpsc::Sender<ServiceRequest>       # 向 service 发请求
    state_rx:    watch::Receiver<StateSnapshot>     # 接收最新状态快照
    events_tx:   Option<broadcast::Sender<Event>>   # 仅部分 service 有
}
```

- **`mpsc` 请求 channel**：router 向 service 发请求，每个请求带一个 `oneshot::Sender` 作为回复通道
- **`watch` 状态 channel**：service 持续发布最新状态；新订阅者 clone receiver 后立即可读当前值（天然支持"订阅时立即推送快照"）；自动合并中间态（电量快速变化时 qs 只看到最新值）
- **`broadcast` 事件 channel**（可选）：仅用于**离散、不可合并**的事件流，典型是 `notification` service——每条通知都必须送达，不能被后来的通知覆盖

#### Service 的内部循环

每个 service 的主 task 只有一个 tokio `select!` 循环，在其中同时处理：

- 从 `mpsc` 接收的请求
- 从后端（D-Bus / netlink / PipeWire / HTTP）接收的事件

所有状态都是 task 局部变量，不跨 await 被外部借用。不存在借用冲突，不需要 `Arc<Mutex<_>>`。

#### 为什么不用 trait

- `async fn in trait` + `dyn Trait` 有 object safety 问题
- 每个 service 的状态类型、请求枚举都不同，统一 trait 必然退化成 `Value` in / `Value` out，丢失类型信息
- 手写显式的 `start_services` 函数反而是清晰的单一事实来源

### 5.3 Service 注册

顶层模块 `services/mod.rs` 导出一个函数：

```
async fn start_services(cfg: &Config, bus: &Bus) -> HashMap<String, ServiceHandle>
```

此函数按配置逐个调用每个 service 模块的 `spawn(config) -> ServiceHandle` 函数，组装成 HashMap 返回。新增 service 必须在此函数中显式注册。

每个 service 模块（如 `services/battery/mod.rs`）对外只暴露：

- `pub fn spawn(config: BatteryConfig) -> ServiceHandle`
- 内部类型（Snapshot、Request action 枚举、Error 类型等）作为 module 私有或 pub(crate)

### 5.4 Router

Router 是一个瘦对象，持有 `services: HashMap<String, ServiceHandle>` 和当前所有 qs session 的注册表。它的职责是：

- 接受来自 qs session 的 `REQ` / `ONESHOT`：查 HashMap 找到 service，通过 `request_tx` 转发，等待 `oneshot` 回复，打包成 `REP` 或 `ERR` 发回 session
- 接受 `SUB`：调用 `state_rx.borrow().clone()` 立即推送一次快照；spawn 一个 forwarder task 持续监听 `watch::Receiver::changed()`，将新快照包装成 `PUB` 推给 session
- 接受 `UNSUB`：中止对应的 forwarder task
- Session 断开时：自动中止该 session 的所有 forwarder

Router 不持有任何 service 状态的 `&mut` 引用，可以被多个 session task 并发调用。

## 6. Daemon 模块划分

```
qsovd/                              # Rust crate
├── Cargo.toml
└── src/
    ├── main.rs                     # 入口, 启动 tokio runtime, 加载 config
    ├── config/
    │   ├── mod.rs                  # Config 结构, 加载与校验
    │   ├── schema.rs               # serde-derived 配置结构体
    │   └── watcher.rs              # inotify 热重载
    ├── ipc/
    │   ├── mod.rs
    │   ├── transport.rs            # UDS listener + length-prefixed framing
    │   ├── protocol.rs             # msgpack serde, envelope, message kinds
    │   ├── router.rs               # Router, session 管理, SUB forwarder
    │   └── session.rs              # 单个 qs 连接的生命周期
    ├── bus/
    │   └── mod.rs                  # ServiceHandle, ServiceRequest,
    │                               # ServiceError, 通用类型
    ├── services/
    │   ├── mod.rs                  # start_services 注册函数
    │   ├── battery/
    │   │   └── mod.rs              # UPower D-Bus, spawn, BatterySnapshot
    │   ├── network/
    │   │   ├── mod.rs              # 聚合 net.link 与 net.wifi 的 spawn
    │   │   ├── link.rs             # net.link: rtnetlink 接口/IP/路由
    │   │   └── wifi.rs             # net.wifi: wpa_supplicant ctrl socket
    │   ├── bluetooth/
    │   │   └── mod.rs              # BlueZ D-Bus
    │   ├── audio/
    │   │   └── mod.rs              # PipeWire client
    │   ├── mpris/
    │   │   └── mod.rs              # MPRIS D-Bus 多播放器追踪
    │   ├── notification/
    │   │   └── mod.rs              # 实现 o.f.Notifications D-Bus server
    │   ├── niri/
    │   │   └── mod.rs              # niri IPC event stream
    │   └── weather/
    │       └── mod.rs              # Open-Meteo HTTP client + 缓存
    └── platform/
        └── linux.rs                # 平台相关的小工具 (PR_SET_PDEATHSIG 等)
```

## 7. 配置系统

### 7.1 配置文件

两份文件都位于 `~/.config/quicksov/`：

- **daemon.toml**：daemon 运行参数与 service 启用清单
- **design-tokens.toml**：L1 产物，daemon 启动时读取并通过 `theme` topic 推送给 qs

### 7.2 daemon.toml 结构

```toml
[daemon]
log_level = "info"
socket_path = "$XDG_RUNTIME_DIR/quicksov/daemon.sock"

[screens]
[[screens.mapping]]
match_name = "DP-1"                 # 外接 P2418D
role = "main"

[[screens.mapping]]
match_name = "eDP-1"                # 笔记本内屏
role = "aux"

[services]
enabled = [
  "battery",
  "net.link",
  "net.wifi",
  "bluetooth",
  "audio",
  "mpris",
  "notification",
  "niri",
  "weather",
]

[services.weather]
backend = "open-meteo"
location_mode = "manual"
latitude = 51.5074
longitude = -0.1278
location_name = "London"
poll_interval_sec = 600
units = "metric"

[services.network]
wifi_backend = "wpa_supplicant"
wpa_ctrl_path = "/run/wpa_supplicant/wlo1"
interfaces = ["wlo1", "enp109s0"]

[services.audio]
backend = "pipewire"

[services.niri]
socket = "$NIRI_SOCKET"
```

### 7.3 环境变量展开

配置中出现的 `$VAR` 形式在加载时由 daemon 展开。未定义变量视为配置错误。

### 7.4 热重载策略

Daemon 用 inotify 监听两份 toml。变更按影响范围分三类：

- **可热重载**：log_level、轮询间隔、天气 location、字段级值
- **重启对应 service**：某个 service 的 backend 切换、接口白名单变化
- **需要重启 daemon**：socket_path、screens mapping、service enabled 列表变化

需要重启 daemon 的变化不会自动执行——daemon 记录日志、向 qs 推送一条 `meta` 事件让 qs 显示提示，但不自杀。用户手动重启。

`design-tokens.toml` 任何变化都触发一次 `theme` topic 的 PUB 推送。

## 8. 运行时目录

```
~/.config/quicksov/
├── daemon.toml
├── design-tokens.toml
├── shell.qml                       # qs 入口
├── Theme.qml                       # singleton, 从 daemon 拉 tokens
├── ipc/
│   ├── Client.qml                  # IPC client, 管理连接
│   └── protocol.js                 # msgpack 编解码 + 消息类型
├── services/                       # qs singleton, 每个对应一个 daemon service
│   ├── Battery.qml
│   ├── Network.qml
│   ├── Bluetooth.qml
│   ├── Audio.qml
│   ├── Mpris.qml
│   ├── Notification.qml
│   ├── Tray.qml
│   ├── Niri.qml
│   └── Weather.qml
├── bars/
│   ├── MainBar.qml                 # 主屏 top bar
│   └── AuxBar.qml                  # 副屏 auto-hide left bar
├── components/                     # 可复用 UI primitive
│   ├── Popup.qml
│   ├── IconButton.qml
│   ├── SvgIcon.qml
│   └── AnimatedValue.qml
├── widgets/                        # bar 内具体 widget
│   ├── Clock.qml
│   ├── WorkspaceStrip.qml
│   ├── WindowInfo.qml
│   ├── BatteryIndicator.qml
│   ├── NetworkIndicator.qml
│   ├── BluetoothIndicator.qml
│   ├── VolumeIndicator.qml
│   ├── NotificationButton.qml
│   └── TrayHost.qml
├── overlays/                       # 大面板
│   ├── PowerMenu.qml
│   ├── MusicPanel.qml
│   ├── ClockPopup.qml
│   └── NotificationCenter.qml
└── icons/
    ├── lucide/
    └── phosphor/
```

启动时设置 `QS_BASE_PATH=$HOME/.config/quicksov`，qs 从此目录读 `shell.qml`。

## 9. 开发仓库目录

```
~/proj/quicksov/
├── daemon/                         # Rust crate (qsovd)
│   ├── Cargo.toml
│   └── src/                        # 见 section 6
├── shell/                          # QML 源码, 对应运行时的 qs 部分
│   ├── shell.qml
│   ├── Theme.qml
│   ├── ipc/
│   ├── services/
│   ├── bars/
│   ├── components/
│   ├── widgets/
│   └── overlays/
├── config/                         # 配置模板
│   ├── daemon.toml.example
│   └── design-tokens.toml
├── icons/                          # SVG 源
│   ├── lucide/
│   └── phosphor/
├── protocol/                       # IPC 协议
│   ├── spec.md                     # 主文档 (Markdown + JSON Schema)
│   └── schema.json                 # 机读副本 + default options
├── docs/                           # 设计文档
│   ├── L0-context.md
│   ├── L1-design-language.md
│   ├── L2-components.md
│   ├── L3-architecture.md          # 本文档
│   └── adr/
│       ├── 001-rust-daemon.md
│       ├── 002-uds-msgpack-framing.md
│       ├── 003-actor-service-model.md
│       ├── 004-watch-vs-broadcast.md
│       ├── 005-runtime-vs-repo-layout.md
│       ├── 006-no-network-manager.md
│       └── 007-binary-naming-qsovd.md
├── scripts/
│   ├── install.sh                  # 安装到 ~/.config/quicksov
│   └── dev-link.sh                 # symlink 模式部署, 便于迭代
└── README.md
```

## 10. 部署策略

### 10.1 生产部署

`scripts/install.sh` 做两件事：

1. `cargo install --path daemon --bin qsovd`  安装二进制到 `~/.local/bin/`
2. `cp -r shell/* ~/.config/quicksov/`  拷贝 qs 配置
3. `cp config/daemon.toml.example ~/.config/quicksov/daemon.toml` 仅在目标不存在时
4. `cp config/design-tokens.toml ~/.config/quicksov/design-tokens.toml`
5. `cp -r icons/* ~/.config/quicksov/icons/`

### 10.2 开发迭代

`scripts/dev-link.sh` 用 symlink 代替 cp，让 repo 里的 QML 改动直接被 qs 热重载看到：

```
~/.config/quicksov/shell.qml  →  ~/proj/quicksov/shell/shell.qml
~/.config/quicksov/bars       →  ~/proj/quicksov/shell/bars
... (所有 shell/ 下的内容)
~/.config/quicksov/daemon.toml  →  ~/proj/quicksov/config/daemon.toml.example
~/.config/quicksov/design-tokens.toml → ~/proj/quicksov/config/design-tokens.toml
```

Daemon 用 `cargo run --bin qsovd` 启动（而不是 install 到 `~/.local/bin`），避免每次改动都要 reinstall。

### 10.3 Niri 启动脚本集成

在 niri 的 `spawn-at-startup` 或 session 启动脚本中：

```
export QS_BASE_PATH="$HOME/.config/quicksov"
exec qsovd &           # 或 dev 模式下: cargo run --manifest-path ~/proj/quicksov/daemon/Cargo.toml
sleep 0.2              # 让 daemon 先绑定 socket (可选, qs 会自动重试)
exec qs &
```

qs 侧的 IPC client 必须实现连接重试，使 daemon 实际启动顺序与 qs 启动顺序解耦。

## 11. 已锁定的决策汇总

| 维度 | 决策 |
|---|---|
| 项目名 | quicksov |
| Daemon 二进制 | qsovd |
| Daemon 语言 | Rust |
| 并发运行时 | Tokio |
| 进程特权模型 | 纯 user-space 单进程 |
| 进程拓扑 | Daemon 与 qs 独立进程，socket 寻址 |
| 启动方式 | Niri 启动脚本 spawn |
| IPC 传输 | Unix Domain Socket |
| IPC 序列化 | MessagePack |
| IPC framing | u32 LE length prefix |
| 握手 | Hello / HelloAck，版本协商 |
| Topic 命名 | 层级（`net.wifi` 风格） |
| 订阅粒度 | 整个 topic |
| 初始推送 | SUB 时立即推一次快照 |
| Schema 维护 | `spec.md` (Markdown + JSON Schema) + `schema.json` (机读) |
| Service 抽象 | Actor task + `ServiceHandle`（无 trait） |
| 状态广播机制 | `tokio::sync::watch` |
| 离散事件广播 | `tokio::sync::broadcast`（仅 notification） |
| 配置格式 | TOML |
| 配置位置 | `~/.config/quicksov/` |
| QS 基路径 | `QS_BASE_PATH=~/.config/quicksov` |
| 网络栈 | wpa_supplicant + dhcpcd，netlink 驱动 link 状态 |
| 网卡名 | `wlo1`, `enp109s0` |
| 音频栈 | PipeWire |
| 通知服务 | daemon 实现 `org.freedesktop.Notifications` server |
| 天气后端 | Open-Meteo |

## 12. 下一步

L3 到此收口。后续工作按顺序：

1. 逐条撰写 ADR（`docs/adr/001` 起），每条记录单一决策的 context / options / decision / consequences
2. 撰写 `protocol/spec.md` 的第一版骨架（每个 topic 的 state snapshot schema、action 列表）
3. 产出最终交付物：给 Claude Code 的 implementation prompt，分阶段（先 daemon skeleton → IPC 层 → 首个 service → qs skeleton → 首个 widget 端到端打通 → 其余 service / widget 批量实现）
