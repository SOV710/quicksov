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

### 1.1 Surface 递进规则

本轮前端重构不改色板来源，但补充**容器递进规则**：

- `bar shell` / `panel shell` 使用最接近基础 `surface` 的**浅表面色**
- `group container` 使用同色系略更实一些的 `surface-raised` 或低透明 accent mix
- `leaf item` 再向内收一层，使用更明确的激活色或 hover 色

要求：

- 不允许为了解决层级感而引入脱离 `theme_tokyonight.json` 的新色
- 同一层级体系内，优先通过**明度 / 透明度 / 同源 accent 混合**建立层次，而不是换一套 hue
- 组件层禁止直接写 `rgba(1,1,1,...)`、`rgba(0,0,0,...)` 这类脱离 token 命名的颜色；所有 alpha surface / shadow / subtle fill 都必须先在 `Theme.qml` 中以具名 token 归约
- bar 内部的层级优先表现为“同一家族的递进”，而不是五颜六色的碎片化组件
- 主屏 top bar 的第一优先级是**从桌面背景中被清晰识别出来**；不得把 bar、group container、inactive spot 压暗到接近背景
- 当前设计方向是“浅 bar + 浅容器 + 局部实色高亮”，不是深色厚玻璃条

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

**主力方案：本地 SVG Icon Set**，不用 Nerd Font 字形图标作为主要 icon 来源。理由：

- Nerd Font 字形在非整数像素位置渲染模糊（主屏 1.25x DPR 下尤其明显）
- 字形 icon 的描边粗细与 Editorial New 的细 serif 不协调
- SVG 可以做部分着色（WiFi 活动条 accent + 未激活条 muted）
- SVG 可以被 QML `PropertyAnimation` 直接驱动做动画（蓝牙扫描脉冲、WiFi 连接波纹）

**Icon 集边界**：

| 集 | 用途 | 许可 |
|---|---|---|
| **Material Icons / Material Symbols** | shell chrome 主力。top bar、status indicators、status popup controls | Apache-2.0 |
| **Application native icons** | tray item 自带图标，不做统一替换 | 各应用自带 |
| **Lucide / Phosphor** | 过渡或局部展示层。允许留在旧组件中，但不再是 top bar 的默认 vocabulary | MIT |

本地资源目录以仓库内 `icons/` 为准，本轮 shell chrome 的目标目录是 `icons/material/`。

**使用规则**：

- 右上角状态区的 shell-owned icons 必须优先使用 Material assets
- tray 继续显示应用原始 icon；shell 只统一其 container，不重绘 icon 风格
- 旧的 Lucide/Phosphor 资产可在迁移阶段继续存在，但文档上的目标风格不再围绕它们设计

QML 通过 `Image { source: ...svg; sourceSize: ... }` 加载，改色通过 `currentColor` 风格的 SVG 或等价预处理完成。

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
| `outer_margin` | 20px | bar 距屏幕三边空隙 |
| `height` | 32px |
| `inner_pad_x` | 16px |
| `inner_pad_y` | 0px |
| `corner_radius` | 20px（= `radius.lg`） |
| `shadow` | `0 4px 8px rgba(0,0,0,0.18)` |

约束：

- top bar 的高度在本轮固定收回为 **32px**
- 阴影只能作为外部视觉投影，不能让 bar 看起来像多出一层实体厚度
- 阴影不参与 `exclusiveZone`、不参与 hit area、也不参与布局测量

### 3.3.1 主屏 top-bar 内层容器规则

| 参数 | 值 |
|---|---|
| `group_container_height` | 24px |
| `group_container_pad_x` | 8px |
| `group_container_radius` | 16px（= `radius.md`） |
| `leaf_chip_height` | 12-20px（视组件而定；workspace spot 优先 14px） |
| `leaf_chip_radius` | 12px（= `radius.sm`） |
| `status_capsule_height` | 26px |
| `status_capsule_radius` | 13px |

这些 token 只定义视觉层级，不强制所有组件做完全相同的内部布局；但必须遵守 `bar shell -> group container -> leaf item` 的层级关系。

补充要求：

- `workspace-strip` 的 inactive spot 不得暗到接近 bar 底色，至少需要维持清晰轮廓
- `window-info` 文本块必须在其 container 中严格竖直居中
- `status capsule` 可以比普通 group container 更高，但必须通过内边距表现为“嵌入 bar”，而不是撑高 bar 本体

### 3.4 副屏 auto-hide left-bar 几何

| 参数 | 值 |
|---|---|
| `collapsed_width` | 0px（完全贴边） |
| `trigger_zone` | 3px（触发热区宽） |
| `trigger_delay_ms` | 200（hover 防误触） |
| `expanded_width` | 320px（为 music panel 设计） |
| `expanded_margin` | 8px |
| `corner_radius_collapsed` | 0 |
| `corner_radius_expanded` | 20px |

### 3.5 Popup 几何

| 参数 | 值 |
|---|---|
| `gap_from_bar` | 12px |
| `padding` | 24px |
| `corner_radius` | 28px（= `radius.xl`） |
| `shadow` | `0 12px 40px rgba(0,0,0,0.24)` |

### 3.5.1 Status Panel family

| 参数 | 值 |
|---|---|
| `status_panel_width` | 440px |
| `status_panel_max_height` | 380px |
| `panel_edge_inset` | 24px |

适用范围：

- battery popup
- network popup
- bluetooth popup
- volume popup
- notification center

目标是统一成**紧凑型 anchored utility panel**：

- 默认尺寸克制
- 高度随内容增长
- 需要列表时优先增长内容区，而不是默认做成大面板
- 与 top bar 保持轻量、精确、贴近触发源的视觉关系

### 3.5.2 Clock Panel family

| 参数 | 值 |
|---|---|
| `clock_panel_width` | 1040px |
| `clock_panel_max_height` | 520px |

clock popup 仍是独立 family，但需要与 status panel 共享同一套圆角与阴影语言。

### 3.5.3 Segmented Clock geometry

| 参数 | 值 |
|---|---|
| `clock_segment_height` | 24px |
| `clock_segment_outer_radius` | 12px |
| `clock_segment_gap` | 0px |
| `clock_segment_min_width` | 48px |

规则：

- bar 中的 clock 不是三颗彼此分离的小药丸，而是一个**连续 segmented capsule**
- 三段之间不留缝，整体通过共享轮廓裁切
- 左右两端保留圆角，中段保持直边切分
- 文本必须在各自 segment 中水平、竖直双居中

## 4. 圆角系统

### 4.1 Radius scale

**五级嵌套系统**。本轮重构明确提高圆角，以支持更柔和的 capsule 视觉。

| Token | 值 | 用途 |
|---|---|---|
| `xs` | 8px | 最内层：badge、最小 hover highlight、inline tag |
| `sm` | 12px | leaf chip：tray item container、workspace active spot、小按钮 |
| `md` | 16px | group container：workspace strip 外层、window info 外层、clock 单段 |
| `lg` | 20px | top bar shell、status capsule |
| `xl` | 28px | popup / menu / notification / large panel 外壳 |

### 4.2 嵌套规则

**公式**：`外层 radius >= 内层 radius + padding`

校验：
- bar shell (20px) 包含 group container (16px) with pad 4px → 20 ≥ 16+4 ✓
- group container (16px) 包含 leaf chip (12px) with pad 4px → 16 ≥ 12+4 ✓
- popup shell (28px) 包含 inner card (16px) with pad 12px → 28 ≥ 16+12 ✓

### 4.3 为什么 bar 用 20 而不是更小

- 旧的 14px 在更大的 panel 语言下显得不够柔和
- 当前目标不是“按钮感 bar”，而是更接近悬浮器物感的顶栏
- 20px 能让 48px 高的 bar 拿到足够的 capsule 气质，同时保留横向长条识别度

### 4.4 圆角递进原则

- 外层 radius 必须大于内层 radius
- 越接近“整组容器”，radius 越大
- 越接近“单点交互或状态叶子”，radius 越小
- 不允许出现外层 16px、内层 20px 这种倒挂
- 同一个组件家族内，radius 的变化应该形成肉眼可识别的层级，而不是 1-2px 的无意义抖动
- segmented clock 的三段不单独追求独立圆角，优先遵守“单轮廓内部分段”原则

## 5. 动效规则

**注**：本轮前端大规模重构暂不重新定义动效系统。以下 motion token 继续作为现行默认值，真正的动画语言收口留待后续 ADR。

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
