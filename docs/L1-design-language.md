#+title: L1 Design Language Specification
#+author: SOV710
#+date: 2026-04-12
#+project: quicksov
#+depends-on: L0 Primary Context

# L1 Design Language Specification

本文档定义 quicksov 所有视觉元素的原子 token。所有后续 QML 组件的代码中**不得出现任何字面量颜色值、像素值、时间值**——必须全部引用本文档定义的 token，通过 `Theme.qml` singleton 暴露。

## 1. 色彩系统

- **来源**：从 `tokyonight.lua`（Neovim 主题）提取的完整 token 集
- **主文件**：`config/theme_tokyonight.json`
- **语义 token**：`surface` / `surface-variant` / `on-surface` / `on-surface-muted` / `accent` / `muted` / `danger` / `warning` / `success`
- **Theme 全局唯一**，不做日夜切换

具体色值保存在 `theme_tokyonight.json`，daemon 启动时读取并通过 `theme` topic 推送给 qs。

## 2. 排版系统

### 2.1 字体族

| 角色                     | 字体                          | 用途                                                                 |
|--------------------------|-------------------------------|----------------------------------------------------------------------|
| **primary**              | Editorial New                 | UI 和 Display 共用。从 bar 上的小字时钟到 auto-hide 面板的 hero 标题 |
| **cjk** (fallback)       | 筑紫A明朝 (Tsukushi A Mincho) | CJK 字符 fallback。serif/明朝系与 Editorial New 调性统一             |
| **icon-font** (fallback) | Symbols Nerd Font Mono        | 仅作为 tray icon name 的字形 fallback 层                             |

**注意事项**：
- Editorial New 小字号 (10-13px) 实测可读性 OK
- 筑紫A明朝是 FONTWORKS 商业字体，需持有授权
- QML 文本组件必须配置 fallback chain：`"Editorial New, Tsukushi A Mincho, sans-serif"`

### 2.2 图标策略

**主力方案：SVG Icon Set**，不用 Nerd Font 字形图标作为主要 icon 来源。理由：

- Nerd Font 字形在非整数像素位置渲染模糊（主屏 1.25x DPR 下尤其明显）
- 字形 icon 的描边粗细与 Editorial New 的细 serif 不协调
- SVG 可以做部分着色（WiFi 活动条 accent + 未激活条 muted）
- SVG 可以被 QML `PropertyAnimation` 直接驱动做动画（蓝牙扫描脉冲、WiFi 连接波纹）

**Icon 集选型**：

| 集 | 用途 | 许可 |
|---|---|---|
| **Lucide** | 主力。bar 上的 battery/wifi/bt/volume/notification、tray fallback | MIT |
| **Phosphor** (Duotone) | 展示层。auto-hide 面板里的大图标，视觉重量更强 | MIT |

本地存放路径：`~/.config/quicksov/icons/lucide/` 与 `~/.config/quicksov/icons/phosphor/`。

QML 通过 `Image { source: ...svg; sourceSize: ... }` 加载，改色通过 `ColorOverlay` 或预处理脚本（替换 `stroke="currentColor"` 为 theme token 引用）。

### 2.3 字号梯度

所有数值为**逻辑像素**（qs 会按屏幕 DPR 自动缩放）。

| Token | 尺寸 | 用途 |
|---|---|---|
| `micro` | 10px | 辅助数字（电量百分比附属等） |
| `small` | 11px | bar 默认文字（时钟、窗口标题） |
| `body` | 13px | popup/tooltip 正文 |
| `label` | 15px | popup 标题 |
| `display` | 20px | auto-hide 面板标题 |
| `hero` | 32px | auto-hide 面板大标题（power menu、音乐专辑） |

### 2.4 字重

| Token | 值 | 用途 |
|---|---|---|
| `regular` | 400 | 默认正文 |
| `medium` | 500 | bar 当前 focus workspace、活动状态 |
| `semibold` | 600 | display 与 hero 级别 |

### 2.5 字体特性

- **`tabular-nums` 始终开启**于所有数字显示场景（时钟、百分比、时长）。防止 proportional 数字在更新时左右跳动
- QML 设置：`font.features: { "tnum": 1 }`

## 3. 间距系统

### 3.1 基础单位

**4px**。所有间距是其倍数。4px 在 1.25x 和 2x DPR 下都能对齐物理像素。

### 3.2 Spacing scale

| Token | 值 | 用途 |
|---|---|---|
| `xs` | 4px | widget 内部元素之间（icon ↔ 文字） |
| `sm` | 8px | widget 内部 padding |
| `md` | 12px | 相邻 widget 之间的 gap |
| `lg` | 16px | bar 三区域间的最小 gap |
| `xl` | 24px | auto-hide 面板内部大区块间距 |
| `xxl` | 32px | 面板内部章节分隔 |

### 3.3 主屏 top-bar 几何（悬浮式）

| 参数 | 值 |
|---|---|
| `outer_margin` | 8px | bar 距屏幕三边空隙 |
| `height` | 32px |
| `inner_pad_x` | 12px |
| `inner_pad_y` | 6px |
| `corner_radius` | 14px（= `radius.lg`） |
| `shadow` | `0 2px 12px rgba(0,0,0,0.25)` |

### 3.4 副屏 auto-hide left-bar 几何

| 参数 | 值 |
|---|---|
| `collapsed_width` | 0px（完全贴边） |
| `trigger_zone` | 3px（触发热区宽） |
| `trigger_delay_ms` | 200（hover 防误触） |
| `expanded_width` | 320px（为 music panel 设计） |
| `expanded_margin` | 8px |
| `corner_radius_collapsed` | 0 |
| `corner_radius_expanded` | 14px |

### 3.5 Popup 几何

| 参数 | 值 |
|---|---|
| `gap_from_bar` | 6px |
| `padding` | 16px |
| `corner_radius` | 10px（= `radius.md`） |
| `shadow` | `0 4px 24px rgba(0,0,0,0.35)` |

## 4. 圆角系统

### 4.1 Radius scale

**四级嵌套系统**。外层圆角必须大于内层圆角，否则内层元素会"顶出"外层的视觉包络。

| Token | 值 | 用途 |
|---|---|---|
| `xs` | 4px | 最内层：单个 button 内的 hover 高亮、progress bar、tag |
| `sm` | 6px | 中层：bar 内单个 widget 的 hover 背景、tooltip |
| `md` | 10px | popup / menu / notification 卡片 |
| `lg` | 14px | 悬浮 bar 本身、auto-hide 大面板 |

### 4.2 嵌套规则

**公式**：`外层 radius >= 内层 radius + padding`

校验：
- bar (14px) 包含 widget hover (6px) with pad 8px → 14 ≥ 6+8 ✓
- popup (10px) 包含 button (4px) with pad 6px → 10 ≥ 4+6 ✓

### 4.3 为什么 bar 用 14 而不是 12/16

- 12px 在 32px 高的 bar 上显得偏硬、偏"tab 按钮感"
- 16px 让 32px 高的 bar 显得过于药丸形
- 14px 是经过微调的中间值，既柔软又保留 bar 的矩形识别度

## 5. 动效规则

### 5.1 时长 token

| Token | 值 | 用途 |
|---|---|---|
| `instant` | 0ms | 状态直接切换（workspace 高亮） |
| `fast` | 120ms | hover 反馈、button press、icon 切换 |
| `normal` | 200ms | popup 展开/收起、tooltip 出现 |
| `slow` | 320ms | auto-hide bar 滑入/滑出 |
| `deliberate` | 480ms | notification 进入、大面板过渡 |

### 5.2 缓动曲线

| Token | 曲线 | 用途 |
|---|---|---|
| `standard` | `cubic-bezier(0.2, 0, 0, 1)` | Material emphasized |
| `decelerate` | `cubic-bezier(0, 0, 0, 1)` | 进入屏幕时用 |
| `accelerate` | `cubic-bezier(0.3, 0, 1, 1)` | 离开屏幕时用 |
| `spring` | damping 0.8, stiffness 300 | auto-hide 滑动，有轻微 overshoot |

### 5.3 规则绑定

| 场景 | 时长 | 曲线 |
|---|---|---|
| `hover_feedback` | fast | standard |
| `popup_enter` | normal | decelerate |
| `popup_exit` | fast | accelerate |
| `autohide_enter` | slow | spring |
| `autohide_exit` | normal | accelerate |
| `notification_in` | deliberate | spring |

**原则**：进入慢、退出快。人眼对"出现"和"消失"的不对称感知要求出现需要时间被注意到，消失越快越好，避免拖沓。

### 5.4 不做动画的字段

这些字段的变化直接切换，无过渡：

- `battery_percent`
- `volume_percent`
- `cpu_percent` / `mem_percent`
- `clock`（时间值）

理由：这些是高频更新的纯数值，动画会制造视觉抖动。

图标状态变化（蓝牙扫描等）使用**循环动画**而非一次性过渡动画。

## 6. Design Token 的固化形式

本文档的所有 token 固化为 `~/.config/quicksov/design-tokens.toml`，结构对应 sections 1-5。

- Daemon 启动时读取该文件，通过 `theme` topic 推送给 qs
- qs 的 `Theme.qml` singleton 接收并暴露为 `Theme.color.*` / `Theme.spacing.*` / `Theme.radius.*` / `Theme.motion.*`
- 文件变化触发热重载，qs 立即应用新 token 无需重启

**硬性规则**：所有 QML 组件代码不得出现裸的数值或颜色字面量，必须引用 `Theme.*`。违反此规则的组件无法通过审查。
