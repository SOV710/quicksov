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
| 位置 | `qsov-wallpaper-renderer` 进程中，每个 output 一个独立 background layer-shell surface |
| 数据源 | daemon `wallpaper` service（目录扫描 + `source/view` 状态）；renderer 只消费协议快照并呈现 |
| 默认目录 | `$HOME/.config/quicksov/wallpapers` |
| 配置覆盖 | `daemon.toml.[services.wallpaper].directory` |
| 多屏策略 | 每个 output 通过 `views.<output>` 绑定一个 source；多个 output 可复用同一 source，也可各自使用不同 source |
| Layer | `zwlr_layer_shell_v1.background`，exclusive zone `-1`，全屏锚定，空 input region |
| 静态壁纸 | Qt `QImageReader` 解码，CPU `PreserveAspectCrop` 合成到 renderer present buffer |
| 视频壁纸 | renderer 复用 FFmpeg `VideoDecoder`；按 source 共享解码，多 output 只做各自裁切/合成，再提交到各自 present buffer |
| decode backend | renderer 消费 `renderer.decode_backend_order`，按顺序尝试 `vaapi/cuda/vulkan/...`，失败自动回退 `software`；其中 `cuda` 现在会把选中的 NVIDIA DRM render node 映射到精确 CUDA ordinal，映射失败则跳过 `cuda` 而不是误用默认 GPU |
| GPU 策略 | `render_device_policy` 默认 `same-as-compositor`；`decode_device_policy` 默认 `same-as-render`；`allow_cross_gpu = false` 时禁止解码/渲染主动跨 GPU 漂移 |
| present backend | daemon 暴露 `renderer.present_backend = auto|shm|dmabuf`；renderer 现已支持 GBM + `linux-dmabuf` 提交，`auto` 会优先走 `dmabuf`，失败时自动回退 `shm` |
| 切换动画 | 旧画面 snapshot overlay + fade，默认 `fade 320ms` |
| 失败回退 | 无可渲染图片或目录不可用时，提交纯黑/空背景，不阻塞其它桌面组件 |
| 输出裁切 | `views.<output>.crop = { x, y, width, height }`，normalized 0..1，便于双屏拼接同一视频 |
| audio 配置 | `daemon.toml.[services.wallpaper].video_audio` 控制视频是否放音，默认 false |

**niri 约束**：
- wallpaper 正确路径是 layer-shell `background` layer，而不是普通窗口
- output 生命周期由 renderer 的 `wl_registry` / `wl_output` 驱动
- wallpaper 状态由 daemon 统一归约，renderer 只消费 `wallpaper` topic
- 当 `render_device_policy` 选择独显而 compositor 主设备是另一块 GPU 时，renderer 默认会把视频解码 / libplacebo 渲染继续放在 render GPU 上，但把 dmabuf 的 GBM 分配与 Wayland present 放回 compositor 主 GPU，避免直接向 niri 提交跨 GPU 的 NVIDIA 分配 buffer
- renderer 每 5s 打印一次 source/output telemetry，用于观察实际 `hwdec`、GPU 设备选择、present backend 选择、decode fps、commit/present fps、buffer starvation
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
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ [workspace-container [•][━━][•][•][•]] [window-container [app | title]]
│                             [ 04/21 | 16:29 | Tue ] [tray-chip][tray-chip] [status-capsule]
└────────────────────────────────────────────────────────────────────────────────────────┘
  LEFT ─ WM 语境                           CENTER                                 RIGHT ─ 系统语境
```

- **LEFT**：`workspace-strip` 与 `window-info` 都不再直接贴 bar，而是各自有一层 group container
- **CENTER**：clock 重构为连续分段胶囊，**整体绝对居中于整个 bar**
- **RIGHT**：分成两组
  - `tray chip group`：应用图标，各自带独立半透明容器
  - `status capsule`：battery / network / bluetooth / volume / notification 五个系统状态 icon 收敛为一个统一胶囊

**为什么 CENTER 必须绝对居中**：保证三段时钟位置恒定，形成肌肉记忆。flex-center 会在 left/right 内容变化时漂移，破坏 glance 效率。

**mockup 对齐要求**：

- bar 必须是浅表面色，能明显从 wallpaper 上分离出来
- bar 高度固定为 32px，保持紧凑，不再使用放大的厚条形态
- 阴影只作为外部投影存在，不得让 bar 看起来多出一层实体厚度
- clock 必须是一个共享轮廓的 segmented capsule，而不是三颗分离胶囊
- bar shell 采用半透明 glass shell；稳态可读性依靠 shell fill、描边与内层容器，而不是整体降低文字透明度

### 2.2 交互层次

| 层 | 类别 | 示例 |
|---|---|---|
| 纯展示型 | 无交互或仅 hover 高亮 | workspace-strip、window-info |
| 触发型 | hover 显示 tooltip，click 展开 popup | clock、tray item、battery、net、bt、vol、notif |

**Popup / Dock 通用规则**：
- `MainBar` family 使用 `reservation window + full-screen overlay field` 双层拓扑
- `clock` family 与右上角 status family 都直接 dock 在 `MainBar` 底边，不再保留 bar gap
- popup content 是 `MainBarOverlayWindow` 内的 child item，不再是独立 panel window
- popup 几何由 `PanelGeometryModel` 统一计算；背景由 `PanelBackgroundField` 统一绘制
- `clock` 的 x 对齐 bar 中央 trigger；status family 默认沿 bar 右缘展开；超出可用宽度时向内钳制，至少保留 `panel_edge_inset`（24px）
- 同时最多一个 popup 打开；打开新的自动关闭旧的
- Esc 或点击外部关闭
- 展开：`popup_enter` (normal + decelerate)
- 收起：`popup_exit` (fast + accelerate)
- `battery` / `network` / `bluetooth` / `volume` / `notification` 统一使用紧凑型 `docked status panel family`
- 右上角这组 panel 不再各自独立定位；它们共享 `MainBarPanelScene` 中的 status panel slot
- 右上角 status panel geometry 固定挂在 `MainBar` 下方，整体沿 bar 右缘对齐
- 右上角这组 panel 的 reveal 是单一 drawer vertical expansion，不是各自独立的浮层滑入
- `clock` 使用更宽的 `clock panel family`
- `MainBar` family 的 blur 由 `MainBarOverlayWindow` 统一请求；popup 自己不重复附着协议
- blur region 是 `bar shell + 当前可见 popup shell / dock shell` 的并集
- blur region 只覆盖 shell geometry；outside-click 捕获区与 shadow 不参与

## 3. 主屏组件清单

### 3.1 workspace-strip

| 属性 | 值 |
|---|---|
| 位置 | LEFT 最左 |
| 数据源 | daemon `niri` service：订阅 `niri msg --json event-stream` |
| 更新频率 | 事件驱动 |
| 外层结构 | 必须包在独立 `workspace-container` 中，不直接贴 bar |
| 视觉 | `bar shell -> workspace-container -> strip leaf` 三层；leaf 默认是小圆点，当前 focus 展开为短胶囊 |
| 状态颜色 | 当前 focus 用同源 accent 实色；非 focus 且有窗口用**较浅的中等不透明 spot**；空工作区用更淡但仍清晰可见的 muted spot |
| 数据过滤 | 前端按当前输出设备名过滤，仅显示对应 output 的工作区 |
| 交互 | 点击工作区指示器 → daemon 执行 `focus_workspace { idx }`，切换到对应工作区 |
| 命中区 | 命中区按 `leaf chip` 而不是裸圆点计算，至少覆盖 24px 高度 |
| 实现要求 | container 与 strip spot 之间必须形成肉眼可见的 radius / color 递进；不能再是单层平铺；inactive spot 不得暗到与外层容器融为一体 |

### 3.2 window-info

| 属性 | 值 |
|---|---|
| 位置 | LEFT，workspace-strip 右侧，`spacing.lg` 间距 |
| 数据源 | daemon `niri` service 的 focused-window 事件（`app_id` + `title`） |
| 外层结构 | 必须包在独立 `window-info-container` 中，不直接贴 bar |
| 格式 | `<AppName> | <WindowTitle>`，AppName 用 `weight.medium` |
| 字号 | `small` (11px) |
| 颜色 | `color.on-surface-muted` |
| AppName 映射 | daemon 维护 `app_id → display_name`（`vivaldi-stable → Vivaldi`、`emacs → GNU Emacs`、`nvim → Neovim`），未知用原 app_id |
| 截断 | 超出可用宽度时 `…` 省略 |
| 视觉要求 | container 必须比 workspace 容器更适合承载文本，不允许直接把文字裸放在 bar surface 上；文本需严格竖直居中，不能视觉上上飘 |

### 3.3 clock

| 属性 | 值 |
|---|---|
| 位置 | CENTER 绝对居中 |
| 数据源 | QML 本地 `Timer { interval: 1000 }`，无需 daemon |
| 结构 | 连续 segmented capsule：`MM/DD` / `HH:MM` / `Tue` |
| 字号 | `small` (11px)，`tabular-nums` |
| 视觉语义 | 三段并列但共享外轮廓；默认无缝拼接；weekday 段允许使用 warm accent surface |
| 文本规则 | bar 上不再显示完整日期与时区；时区信息若需要，进入 popup 展示 |
| 交互 | click → 展开 Clock Popup |

**实现要求**：

- 三段高度一致
- 三段文本在各自 segment 内双轴居中
- 不能再用三个彼此分离的 `Rectangle { radius: ... }` 直接排开代替 segmented capsule

### 3.4 clock-popup

**几何**：
- 宽度目标 1040px；实际为 `min(1040, screen.width - 64)`
- 高度目标 520px；实际为 `min(520, screen.height - barHeight - 96)`
- 不再是轻量小 popup，而是从主屏顶部 `MainBar` 底边直接抽出的**大 docked panel**

**blur / shell 规则**：
- `clock popup` 不是独立 window，而是 `MainBarOverlayWindow` 内的一块 docked shell
- `MainBarOverlayWindow` 会把 `clock popup` 的外壳几何加入统一 blur region
- `clock popup` 外壳与 `bar shell` 使用同源玻璃材质；内部 calendar / weather cards 继续使用更实的 surface

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
- **WMO code → weather icon 映射**由 daemon 维护，但不与 top bar status icon 体系强耦合；天气卡可使用专门的 weather glyph 子集
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
| 位置 | RIGHT，位于 status capsule 左侧 |
| 数据源 | `Quickshell.Services.SystemTray` (StatusNotifierItem) |
| 视觉 | 每 item 不是裸 icon，而是 `tray chip container + native app icon`；chip 自身半透明、低对比、大圆角，但整体明度应明显高于 wallpaper |
| 交互 | 左键 → `item.activate()`；右键 → `item.menu`（通过 Quickshell `SystemTrayItem.menu` 句柄展示原生菜单；具体弹出由 `item.display(parentWindow, relX, relY)` 触发） |
| 统一策略 | 统一的是命中区、container、hover/pressed 反馈；不统一应用 icon 的绘制风格 |

### 3.6 status-capsule

| 属性 | 值 |
|---|---|
| 位置 | RIGHT 最右 |
| 结构 | 单一大胶囊，内部排列 5 个系统状态 slot |
| 顺序 | `battery -> network -> bluetooth -> volume -> notification` |
| icon 体系 | 全部使用 `icons/material/` 下的 Material assets |
| 文本规则 | bar 内不显示电量百分比、音量百分比、通知数量 |
| 默认颜色 | 日常 idle 时 5 个 icon 使用统一前景色 |
| 例外状态 | 充电时 battery 允许绿色；Wi-Fi / 蓝牙扫描时允许蓝色呼吸语义；通知使用红点 badge |
| 目标语义 | 把右侧从“五个孤立工具”变成“一个系统状态对象” |

**几何要求**：

- status capsule 维持 bar 内部嵌入式几何，不上下顶住 bar
- status capsule 是触发控件，不是 dock shell 的一部分
- docked panel 与 `MainBar` 连接，而不是与 `status capsule` 本体连接
- 打开 panel 时，视觉重心应落在“bar 下方抽出一个 panel”，而不是“capsule 自身被拉长”
- 这组 panel 的真实宿主是 `MainBarOverlayWindow` 内的共享 dock shell，而不是独立 popup family

### 3.7 battery

| 属性 | 值 |
|---|---|
| 数据源 | daemon `battery` service via UPower D-Bus |
| bar 显示逻辑 | status capsule 内只显示 icon，不显示 `%`；充电时进入绿色语义 |
| Icon | Material battery glyph family |
| 几何 | click docked panel；内容加载到 `MainBarPanelScene` 的 status panel slot；默认保持较低高度 |
| popup 头部 | 左侧大号 battery icon；右侧主读数 `87%` + 状态词 `Charging/Discharging/Fully charged` |
| popup 次级信息 | 第二行显示 `3h 12m remaining` / `54m until full` / `Time estimate unavailable` |
| popup 指标卡 | `Power Source`、`Battery Health`、`Charge Rate`、`Capacity` 四张信息卡 |
| power profile | 底部 3-way segmented selector：`Saver` / `Balanced` / `Performance` |
| 空状态 | 区分 `No battery detected` 与 `Battery backend unavailable`；前者仍可显示 power profile，后者整体禁用 |
| 交互 | click bar icon → 打开/关闭 docked panel；点击 panel 外关闭；Esc 关闭；click segmented selector → daemon `set_power_profile` |

**实现约束**：
- battery health 优先由 daemon 统一归约，不在 QML 端自行推导
- Power Profile 仅在 daemon 报告 `power_profile_available=true` 时允许交互
- 台式机 / 无电池设备仍允许展示 power profile 区，但必须弱化主状态区

**shell / blur 规则**：
- battery 页自己不绘制外壳；外壳由 `PanelBackgroundField` 统一负责
- shared panel background field 使用半透明 fill
- panel geometry 由 `MainBarOverlayWindow` 统一加入 blur region
- 内部 metric / profile card 不直接承担 blur 语义

### 3.8 network

| 属性 | 值 |
|---|---|
| 数据源 | daemon `net.link`（netlink，接口/IP/路由） + `net.wifi`（wpa_supplicant ctrl socket） |
| 监听接口 | `wlo1`、`enp109s0` |
| bar 视觉 | status capsule 内 icon-only；Wi-Fi 状态优先用 Material Wi-Fi glyph family；有线连接时允许切换为 ethernet glyph；扫描时允许蓝色呼吸语义 |
| 几何 | click docked panel；内容加载到 `MainBarPanelScene` 的 status panel slot；列表区按内容增长，不默认展开成宽大 panel |
| 头部 | 左侧 `Network` 标题 + 副标题；右侧 `Refresh`、`Wi-Fi On/Off`、`Flight` 三个 chip |
| 状态归约 | daemon 额外提供 `availability` / `availability_reason` / `rfkill_*` / `airplane_mode`，区分 ready / disabled / unavailable |
| 列表分组 | `Current` → `Saved` → `Available`；每行显示 SSID、状态副标题（Connected / Saved / Open / WPA2 / 频段 / 信号） |
| 交互 | click bar icon → 打开/关闭 docked panel；打开时按需自动 `scan`；secure 且未保存的网络 inline 输入密码；点击 panel 外关闭 |
| 首版范围 | 实现 Wi-Fi 扫描、连接、断开、忘记网络、Wi-Fi on/off、airplane-mode；**不实现 VPN 区块** |

**实现约束**：
- 扫描与连接由 `wpa_cli` 协议或直接 socket 通信实现
- 链路状态（载体、IP、路由）全部通过 netlink，不依赖 NetworkManager
- dhcpcd 的租约变化通过 netlink ADDR 消息观察

**shell / blur 规则**：
- network 页自己不绘制外壳；外壳由 `PanelBackgroundField` 统一负责
- 列表增长不改变 blur attachment owner，只改变当前 dock shell 几何

### 3.9 bluetooth

| 属性 | 值 |
|---|---|
| 数据源 | daemon `bluetooth` service via BlueZ D-Bus |
| bar 几何 | status capsule 内 icon-only；内容加载到 `MainBarPanelScene` 的 status panel slot |
| 视觉状态 | unavailable：`bluetooth-off`；disabled：`bluetooth-off`；enabled idle：`bluetooth`；已连：保持统一色但切到 connected glyph/state；扫描中：蓝色呼吸语义 |
| 头部 | 左侧 `Bluetooth` 标题 + 状态副标题；右侧 `Refresh/Stop` 与 `On/Off` 控件 |
| 列表分组 | `Connected` → `Paired` → `Available`；每行显示 name/address、状态文案、电量（若有） |
| 交互 | click bar icon → 打开/关闭 docked panel；`Refresh` 开始扫描、扫描中切为 `Stop`；不在打开时自动扫描；点击 panel 外关闭 |

**shell / blur 规则**：
- bluetooth 页自己不绘制外壳；外壳由 `PanelBackgroundField` 统一负责

### 3.10 volume

| 属性 | 值 |
|---|---|
| 数据源 | daemon `audio` service via PipeWire |
| bar 视觉 | status capsule 内 icon-only，不显示百分比 |
| Icon | Material volume glyph family |
| 几何 | click docked panel；内容加载到 `MainBarPanelScene` 的 status panel slot；Applications 列表区上限收回到紧凑规格，避免默认面板过长 |
| 交互 | click → docked panel：大音量 slider、默认 sink 切换、per-app 音量列表；hover 滚轮 → ±5% |

**shell / blur 规则**：
- volume 页自己不绘制外壳；外壳由 `PanelBackgroundField` 统一负责

### 3.11 notification-center

| 属性 | 值 |
|---|---|
| 位置 | status capsule 最右 slot |
| 数据源 | daemon `notification` service（实现 `org.freedesktop.Notifications` D-Bus server，完全取代 mako/dunst） |
| bar 视觉 | `bell` icon；有未读时右上角小红点；不显示数量 |
| Icon | Material notifications glyph family |
| 几何 | click docked panel；内容加载到 `MainBarPanelScene` 的 status panel slot；保持 content-only panel，不额外绘制外壳；通知列表区上限收回到紧凑规格 |
| 内容 | 纯 notification 列表，无标题、无 `Clear all`、无右上角关闭按钮；空态仅显示 muted `No notifications` |
| 卡片 | flat card；左侧大 icon tile（优先本地图标路径，失败回退 bell glyph），右侧只保留 title + details 两层文本；右上角相对时间使用 `now / 1m / 2h / 3d` |
| 展开 | click 卡片摘要区切换展开；同一时刻只允许一条展开；展开后显示完整 details、D-Bus action buttons、以及末尾固定 `I got it` 按钮 |
| 删除/已读 | 不提供 clear-all；删除只保留两种方式：展开态 `I got it`，或向右拖拽超过阈值后 dismiss；panel 打开即对全部未读发送 `mark_read {}`，panel 打开期间新通知也立即标记为已读 |
| 动效 | 右拖当前卡片时，前后相邻卡片按进度右移 `0..Theme.spaceMd`；释放或删除后通过 spring 回弹；本轮只做位移+弹簧，不做 shader/gooey 形变 |

**shell / blur 规则**：
- notification 页自己不绘制外壳；外壳由 `PanelBackgroundField` 统一负责

**Toast 行为**：toast 仍是后续工作；本轮只重构 NotificationCenter panel，不实现右上角 slide-in toast。

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
                  │ [主屏 top bar, 悬浮 outer=20px]   │
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
