#+title: L2 Spatial Layout & Component Inventory
#+author: SOV710
#+date: 2026-04-12
#+project: quicksov
#+depends-on: L0 Primary Context, L1 Design Language Specification

# L2 Spatial Layout & Component Inventory

本文档定义所有 UI 组件的数据源、更新机制、交互模式、空间位置。所有数值引用 L1 token，不含裸像素。

## 1. 多屏拓扑与职责分配

| 维度 | 主屏 (DP-1 / P2418D) | 副屏 (eDP-1 / 笔记本) |
|---|---|---|
| 物理位置 | 视线正中 | 左侧 |
| 职责 | 编码主战场（Neovim、Claude Code） | 辅助信息消费（文档、IRC、笔记） |
| Bar 形态 | 悬浮 top bar | 贴边 auto-hide left bar |
| 信息层次 | L0 always-visible | 默认隐藏，hover 触发 |
| 交互频率 | 被动 glance 为主 | 主动触发为主 |
| 信息类型 | 系统状态 + 工作上下文 | 正在消费的媒体（音乐） |

**Niri 屏幕感知**：`Scope { Variants { model: Quickshell.screens; ... } }`，在 delegate 中读 `modelData.name` 决定加载哪个 bar 组件。映射由 `daemon.toml` 的 `[screens.mapping]` 驱动，daemon 通过 `meta` topic 的 `screens.roles` 字段推送给 qs（ADR-007）。QML 侧在 `Meta.qml` 单例中缓存 `screenRoles` 映射，MainBar/AuxBar 通过 `Meta.screenRoles[modelData.name]` 查询 role，不硬编码屏幕名。

### 1.1 wallpaper-layer

| 属性 | 值 |
|---|---|
| 位置 | 每个 output 一个独立 background layer-shell window |
| 数据源 | daemon `wallpaper` service（目录扫描 + 当前项状态）；QML 只负责渲染 |
| 默认目录 | `$HOME/.config/quicksov/wallpapers` |
| 配置覆盖 | `daemon.toml.[services.wallpaper].directory` |
| 多屏策略 | v1 所有屏幕共享同一张当前壁纸，但每个屏幕各自创建独立 `PanelWindow` / surface |
| Layer | `WlrLayer.Background`，`ExclusionMode.Ignore`，全屏锚定，不参与输入 |
| 静态壁纸 | `Image.PreserveAspectCrop`，按屏幕尺寸解码 |
| 视频壁纸 | `libmpv` 原生 QML plugin，单 decoder + 单 offscreen render target，多屏各自裁切采样 |
| 切换动画 | 旧画面 snapshot overlay + fade，默认 `fade 320ms` |
| 失败回退 | 无可渲染图片或目录不可用时，回退到 `Theme.bgCanvas` 纯色背景 |
| video 配置 | `daemon.toml.[services.wallpaper].video_enabled` 控制是否启用视频；`video_audio` 控制是否放音 |

**niri 约束**：
- wallpaper 正确路径是 layer-shell `background` layer，而不是普通窗口
- v1 不直接依赖 niri IPC；output 生命周期由 `Quickshell.screens` 驱动
- 若希望 wallpaper 固定在 overview/backdrop 中而非随 workspace 缩放，可在 niri config 中手动添加：

```kdl
layer-rule {
    match namespace="^quicksov-wallpaper"
    place-within-backdrop true
}
```

## 2. 主屏 Top Bar 空间布局

### 2.1 三区域划分

```
┌──────────────────────────────────────────────────────────────┐
│ [workspace-strip] [window-info]   [clock]    [tray] [battery]│
│                                                [net] [bt][vol]│
│                                                  [notif-btn] │
└──────────────────────────────────────────────────────────────┘
  LEFT ─ WM 语境                CENTER      RIGHT ─ 系统语境
```

- **LEFT**：workspace-strip（固定）+ window-info（弹性、超长省略号）
- **CENTER**：clock，**绝对居中于整个 bar**（x = barWidth/2 - clockWidth/2，不受 left 区域宽度影响）
- **RIGHT**：从右到左重要度递增，notification 按钮最右

**为什么 CENTER 必须绝对居中**：保证时钟位置恒定，形成肌肉记忆。flex-center 会在 left 内容变化时漂移，破坏 glance 效率。

### 2.2 交互层次

| 层 | 类别 | 示例 |
|---|---|---|
| 纯展示型 | 无交互或仅 hover 高亮 | workspace-strip 视觉、window-info |
| 触发型 | hover 显示 tooltip，click 展开 popup | clock、battery、net、bt、vol、notif、tray item |

**Popup 通用规则**：
- 从 bar 下方 `popup.gap_from_bar`（6px）处滑出
- x 对齐触发 widget 中心，超出屏幕时向内偏移至贴边 8px
- 同时最多一个 popup 打开；打开新的自动关闭旧的
- Esc 或点击外部关闭
- 展开：`popup_enter` (normal + decelerate)
- 收起：`popup_exit` (fast + accelerate)

## 3. 主屏组件清单

### 3.1 workspace-strip

| 属性 | 值 |
|---|---|
| 位置 | LEFT 最左 |
| 数据源 | daemon `niri` service：订阅 `niri msg --json event-stream` |
| 更新频率 | 事件驱动 |
| 视觉 | 水平圆角胶囊条；默认 `8×8`，当前 focus 展开为 `22×8`，圆角 `4`，间距 `spacing.xs` |
| 状态颜色 | 当前 focus 用 `color.accentBlue`；非 focus 且有窗口用 `color.fgSecondary`；空工作区用 `color.fgMuted` |
| 数据过滤 | 前端按当前输出设备名过滤，仅显示对应 output 的工作区 |
| 交互 | 点击工作区指示器 → daemon 执行 `focus_workspace { idx }`，切换到对应工作区 |
| 命中区 | 每个指示器高度 `24px`，宽度为视觉圆点/胶囊宽度加 `spacing.xs` |
| 过渡 | `width` 与 `color` 使用 `Theme.motionFast`(120ms) 过渡；当前无 hover/pressed 视觉反馈 |

### 3.2 window-info

| 属性 | 值 |
|---|---|
| 位置 | LEFT，workspace-strip 右侧，`spacing.lg` 间距 |
| 数据源 | daemon `niri` service 的 focused-window 事件（`app_id` + `title`） |
| 格式 | `<AppName> | <WindowTitle>`，AppName 用 `weight.medium` |
| 字号 | `small` (11px) |
| 颜色 | `color.on-surface-muted` |
| AppName 映射 | daemon 维护 `app_id → display_name`（`vivaldi-stable → Vivaldi`、`emacs → GNU Emacs`、`nvim → Neovim`），未知用原 app_id |
| 截断 | 超出可用宽度时 `…` 省略 |

### 3.3 clock

| 属性 | 值 |
|---|---|
| 位置 | CENTER 绝对居中 |
| 数据源 | QML 本地 `Timer { interval: 1000 }`，无需 daemon |
| 格式 | `2026-04-12 · 19:38 CST · Sun` |
| 字号 | `small` (11px)，`tabular-nums`，`weight.regular` |
| 分隔符 | middle dot `·`，不用 `|` |
| 时区显示 | 显式 UTC 缩写，减少跨时区切换成本（VPS 分布多时区） |
| 交互 | click → 展开 Clock Popup |

### 3.4 clock-popup

**几何**：
- 宽度目标 920px；实际为 `min(920, screen.width - 48)`
- 高度目标 440px；实际为 `min(440, screen.height - barHeight - 64)`
- 不再是轻量小 popup，而是从主屏顶部 bar 下方展开的**大 panel**

**结构**：
- 左卡：月份标题 + Today / 月份切换按钮 + 6×7 month grid + 当前日期 footer
- 右卡：地点 / 状态 / refresh header + 当前天气摘要 + 24h 温度曲线 + 底部 metrics
- 左右两卡独立视觉包络：外层 `radius.lg`，内卡 `radius.md`

**交互**：
- click bar clock → 打开 / 关闭 panel
- click panel 外区域或 `Esc` → 关闭
- 左卡滚轮 / 左右按钮切月
- 打开 panel 时默认回到当前月
- 右卡 refresh 按钮触发 daemon `weather.refresh`

**month grid 视觉规则**：
- 固定 6 行，避免月切换时高度跳变
- 非本月日期保留，但使用 `fgMuted`
- 今日使用 accent border + `surfaceActive`
- 不引入伪交互：v1 日期格仅 hover，不提供事件点击

**天气数据源**：
- Backend：**Open-Meteo**（免费、无 API key、隐私友好）
- Daemon `weather` service 职责：
  - 当前仅支持配置文件手动提供 `latitude` / `longitude` / `location_name`
  - 内部架构为 scheduler task + fetch worker，便于后续扩展多 provider
  - 请求 `/v1/forecast` 获取当前温度、WMO weather code、湿度、风速、未来小时级预报
  - 轮询间隔 600s（配置项）
  - 成功快照 canonical cache 持久化到 `~/.cache/quicksov/weather/current.json`
  - State snapshot 额外下发 `provider` / `status` / `ttl_sec` / `last_success_at` / `error`
- **WMO code → Lucide icon 映射**由 daemon 维护，但必须只使用本仓库实际存在的图标子集（如 `sun` / `cloud-sun` / `cloud-fog` / `cloud-drizzle` / `cloud-rain` / `cloud-snow` / `cloud-lightning` / `cloud`）
- 刷新失败不直接清空上一份成功数据；前端根据 `last_success_at + ttl_sec` 自行决定何时将旧数据视为过期

**weather 曲线规则**：
- 固定显示当日 `00:00 → 23:00` 的 24h 温度曲线
- x 轴固定，不围绕当前时间平移
- 当前时间在曲线上是一个实时移动的 accent marker（由本地 `Time.now` 驱动）
- y 轴仅显示简化的 3 个温度刻度，避免信息密度过高
- 当前温度、描述、体感温度放在曲线外围，不压到 plot 上

**weather 状态语义**：
- `loading` / `refreshing`：显示 loading 状态，不伪造曲线
- `ready`：显示完整天气卡
- `refresh_failed` 且 TTL 内：保留旧曲线，状态标签显示 `Stale`
- `init_failed` 或 TTL 过期：隐藏曲线，显示 unavailable 状态

### 3.5 tray

| 属性 | 值 |
|---|---|
| 位置 | RIGHT 最左 |
| 数据源 | `Quickshell.Services.SystemTray` (StatusNotifierItem) |
| 视觉 | 每 item 16×16 原生 icon，间距 `spacing.sm` |
| 交互 | 左键 → `item.activate()`；右键 → `item.menu`（通过 Quickshell `SystemTrayItem.menu` 句柄展示原生菜单；具体弹出由 `item.display(parentWindow, relX, relY)` 触发） |
| Hover 背景 | `radius.xs` 统一包裹（无法统一 item 风格，只能统一容器） |

### 3.6 battery

| 属性 | 值 |
|---|---|
| 数据源 | daemon `battery` service via UPower D-Bus |
| 显示逻辑 | `OnBattery=true` → icon + `87%`；充电中 → charging icon + `%`；充满插电 → plug icon |
| Icon | Lucide `battery` / `battery-low` / `battery-charging` / `plug` |
| 低电量 | < 20% icon 染 `color.danger` |
| 几何 | click popup；宽 420px；最大高 460px；锚定 MainBar battery icon，下方 `gap_from_bar` |
| popup 头部 | 左侧大号 battery icon；右侧主读数 `87%` + 状态词 `Charging/Discharging/Fully charged` |
| popup 次级信息 | 第二行显示 `3h 12m remaining` / `54m until full` / `Time estimate unavailable` |
| popup 指标卡 | `Power Source`、`Battery Health`、`Charge Rate`、`Capacity` 四张信息卡 |
| power profile | 底部 3-way segmented selector：`Saver` / `Balanced` / `Performance` |
| 空状态 | 区分 `No battery detected` 与 `Battery backend unavailable`；前者仍可显示 power profile，后者整体禁用 |
| 交互 | click bar icon → 打开/关闭 popup；点击 popup 外关闭；Esc 关闭；click segmented selector → daemon `set_power_profile` |

**实现约束**：
- battery health 优先由 daemon 统一归约，不在 QML 端自行推导
- Power Profile 仅在 daemon 报告 `power_profile_available=true` 时允许交互
- 台式机 / 无电池设备仍允许展示 power profile 区，但必须弱化主状态区

### 3.7 network

| 属性 | 值 |
|---|---|
| 数据源 | daemon `net.link`（netlink，接口/IP/路由） + `net.wifi`（wpa_supplicant ctrl socket） |
| 监听接口 | `wlo1`、`enp109s0` |
| 视觉 | WiFi 4 格信号 icon（按 RSSI 填充）；离线 `wifi-off`；以太网 `ethernet` |
| 几何 | click popup；宽 420px；最大高 560px；锚定 MainBar network icon，下方 `gap_from_bar` |
| 头部 | 左侧 `Network` 标题 + 副标题；右侧 `Refresh`、`Wi-Fi On/Off`、`Flight` 三个 chip |
| 状态归约 | daemon 额外提供 `availability` / `availability_reason` / `rfkill_*` / `airplane_mode`，区分 ready / disabled / unavailable |
| 列表分组 | `Current` → `Saved` → `Available`；每行显示 SSID、状态副标题（Connected / Saved / Open / WPA2 / 频段 / 信号） |
| 交互 | click bar icon → 打开/关闭 popup；打开时按需自动 `scan`；secure 且未保存的网络 inline 输入密码；点击 popup 外关闭 |
| 首版范围 | 实现 Wi-Fi 扫描、连接、断开、忘记网络、Wi-Fi on/off、airplane-mode；**不实现 VPN 区块** |

**实现约束**：
- 扫描与连接由 `wpa_cli` 协议或直接 socket 通信实现
- 链路状态（载体、IP、路由）全部通过 netlink，不依赖 NetworkManager
- dhcpcd 的租约变化通过 netlink ADDR 消息观察

### 3.8 bluetooth

| 属性 | 值 |
|---|---|
| 数据源 | daemon `bluetooth` service via BlueZ D-Bus |
| 几何 | click popup；宽 420px；最大高 560px；锚定 MainBar 蓝牙 icon，下方 `gap_from_bar` |
| 视觉状态 | unavailable：`bluetooth-off` + muted + "No Bluetooth adapter"；disabled：`bluetooth-off` + muted + "Bluetooth is off"；enabled idle：`bluetooth`；已连：`bluetooth` + accent；扫描中：icon 脉冲/refresh loading（周期 1200ms） |
| 头部 | 左侧 `Bluetooth` 标题 + 状态副标题；右侧 `Refresh/Stop` 与 `On/Off` 控件 |
| 列表分组 | `Connected` → `Paired` → `Available`；每行显示 name/address、状态文案、电量（若有） |
| 交互 | click bar icon → 打开/关闭 popup；`Refresh` 开始扫描、扫描中切为 `Stop`；不在打开时自动扫描；点击 popup 外关闭 |

### 3.9 volume

| 属性 | 值 |
|---|---|
| 数据源 | daemon `audio` service via PipeWire |
| 视觉 | icon (按音量分档 `volume-2/1/x/muted`) + 百分比 |
| 几何 | click popup；宽 420px；最大高 560px；Applications 列表区上限 260px |
| 交互 | click → popup：大音量 slider、默认 sink 切换、per-app 音量列表；hover 滚轮 → ±5% |

### 3.10 notification-center

| 属性 | 值 |
|---|---|
| 位置 | RIGHT 最右 |
| 数据源 | daemon `notification` service（实现 `org.freedesktop.Notifications` D-Bus server，完全取代 mako/dunst） |
| 视觉 | `bell` icon；有未读时右上角 6px `color.danger` 红点 |
| 几何 | click popup；宽 420px；最大高 560px；通知列表区上限 480px |
| 交互 | click → 展开 NotificationCenter popup；长按或右键 → 清空全部 |

**Toast 行为**：新 notification 到达时主屏右上角滑入 toast 卡片（`notification_in`），stay 5s 自动滑出，hover 暂停。最多堆叠 3 条。

## 4. 主屏底部 auto-hide Power Menu

| 属性 | 值 |
|---|---|
| 触发 | 屏幕底部中心 200px 宽 × 3px 高热区 hover |
| 展开 | 从底部向上滑出，宽 400px 高 120px 居中 |
| 内容 | 5 大按钮横排：Lock / Suspend / Logout / Reboot / Shutdown |
| 按钮 | 64×64，Phosphor Duotone icon + `body` 标签 |
| 危险操作二次确认 | Reboot / Shutdown 首次点击变红显示 "Click again" 3s 超时 |
| 退出 | 鼠标离开或 Esc |
| 动画 | 进入 `autohide_enter`，退出 `autohide_exit` |

## 5. 副屏组件清单

**设计哲学**：副屏最小化常驻信息。不需要 clock（视线切主屏）、不需要 tray（主屏已有）、不需要 workspace strip（Niri overview 代替）。副屏只有一个 auto-hide 音乐面板。

### 5.1 music-panel (auto-hide left)

| 属性 | 值 |
|---|---|
| 收起 | 宽 0，保留 3px 触发热区 |
| 触发 | hover 左边缘热区 > 200ms |
| 展开 | 宽 320px，从左滑出（`autohide_enter` spring） |
| 数据源 | daemon `mpris` service via IPC（SUB `mpris` topic） |
| 多播放器处理 | 优先选 `playback_status=Playing` 的；都暂停时选最近活动；右下角显示 source 切换器（通过 `select_active` action） |
| 退出 | 鼠标离开自动收起 |

**内容结构**：

```
┌───────────────────────────┐
│   [240×240 album art]     │
│                           │
│   Track Title             │  hero 32px, Editorial New
│   Artist Name             │  display 20px
│   Album · 2024            │  body 13px, muted
│                           │
│   ──●────────────  2:14   │  progress + elapsed
│                    4:32   │
│                           │
│   ⏮   ⏯   ⏭              │
│                           │
│   ♪ Next: Another Track   │  队列下一首预览
└───────────────────────────┘
```

专辑名用 Editorial New 做 hero 级别展示——这个面板是 Editorial New 字体选型的主要发挥场景，期待杂志封面的仪式感。

### 5.2 副屏无常驻 bar

默认不存在 bar。未来若需要临时看时间，设计全局快捷键（`Super+Shift+T`）在副屏中心 flash 大号时钟 2s 后消失。**初版不实现**，留作扩展。

## 6. 空间布局总图

```
                  ┌──────────────────────────────────┐
                  │ [主屏 top bar, 悬浮 outer=8px]    │
                  │                                  │
┌───────────────┐ │                                  │
│               │ │                                  │
│   [副屏]      │ │         主屏工作区               │
│               │ │     (Neovim, Claude Code)        │
│ ←music panel  │ │                                  │
│  auto-hide    │ │                                  │
│               │ │      [Power Menu auto-hide]      │
│               │ │      (底部中心热区)              │
└───────────────┘ │                                  │
                  └──────────────────────────────────┘
```

## 7. Daemon Service 依赖图

```
        UPower D-Bus         → battery-service    ──┐
        netlink (rtnetlink)  → net.link-service   ──┤
        wpa_supplicant ctrl  → net.wifi-service   ──┤
        BlueZ D-Bus          → bt-service         ──┤
        PipeWire             → audio-service      ──┤
        Niri IPC             → niri-service       ──┼─→ IPC Router ─→ QML
        wallpaper directory  → wallpaper-service  ──┤     (UDS+NDJSON)
        weather scheduler    → weather-service    ──┤     (UDS+NDJSON)
        Open-Meteo HTTP      → weather-worker     ──┤
        Freedesktop D-Bus    → notif-service      ──┤
        MPRIS D-Bus          → mpris-service      ──┘
```

每个 service 独立 tokio task，通过 `ServiceHandle` 向 router 暴露 mpsc 请求通道和 watch 状态通道。详见 L3-architecture.md。

## 8. 验收清单

L2 收口的标志：

- [x] 所有组件数据源明确到具体 D-Bus 接口 / HTTP 端点 / netlink 协议族 / 本地 API
- [x] 所有组件更新频率明确为事件驱动或轮询（轮询给出间隔）
- [x] 所有空间位置用 L1 token 表达，无裸像素
- [x] 所有状态变化可映射到 L1 motion rule
- [x] 任意组件的修改不需要改其他组件（松耦合）
- [x] 两屏 bar 完全独立设计，不共用组件布局
- [x] 网络组件明确使用 netlink + wpa_supplicant，不依赖 NetworkManager
- [x] 通知组件明确由 daemon 实现 freedesktop server，取代 mako/dunst
