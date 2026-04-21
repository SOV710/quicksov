# ADR-009: Frontend Visual Refresh — nested containers, Material status icons, oversized status panels

**Status**: Proposed  
**Date**: 2026-04-21

## Context

当前前端并不是“功能不全”问题，而是**视觉语义和交互层级不够统一**：

- 色彩系统已经稳定，且必须继续唯一来源于 `config/theme_tokyonight.json`
- 排版系统已经稳定，Editorial New + CJK fallback 的组合继续保留
- 间距系统已经稳定，4px base unit 与现有 spacing scale 不需要推翻
- 但 icon 体系、圆角体系、bar 的层级包络、右上角状态区的组织方式、popup panel 的尺度，都还停留在较早阶段

当前主要问题：

1. **icons 语义不统一**：shell chrome 仍大量依赖 Lucide，但部分 glyph 与目标气质不匹配  
2. **圆角过小**：bar、widget、popup 的 radius 偏硬，无法形成柔和、递进的包络  
3. **顶栏结构过平**：workspace strip、window info、tray、status modules 直接贴在 bar 上，缺少“bar -> group container -> leaf”层次  
4. **右上角系统状态区过碎**：battery / network / bluetooth / volume / notification 仍像五个独立按钮，不像一个统一的系统状态胶囊  
5. **popup panel 过小**：状态类面板的尺度不足，不符合主屏高分辨率桌面使用场景  
6. **景深不足**：bar 与 panel 的浮层感不够，阴影语言缺位

本 ADR 记录下一轮 QML 大规模重构的**目标设计方向**。它先定义目标，不要求在本次提交立即实现。

## Decision

### 1. 保留不动的系统

以下三套系统在本轮重构中视为**锁定约束**：

- 色彩系统：唯一来源仍是 `config/theme_tokyonight.json`
- 排版系统：字体、字号梯度、数字排版规则继续沿用
- 间距系统：4px base unit 与现有 spacing scale 继续沿用

这次重构不是重新发明整套视觉语言，而是对**图标、圆角、容器层级、主栏结构、状态区组织、panel 尺度**做重构。

### 2. shell chrome 图标系统切换到 Material

top bar 与状态类 popup 的 shell-owned icons，默认迁移到本仓库的 `icons/material/`。

具体边界：

- **主屏右上角状态区**：全部使用 Material icons
- **clock 三段式胶囊中的辅助 icon（如后续需要）**：优先 Material
- **tray**：继续显示应用自己提供的原生图标，不做“统一替换”
- **legacy Lucide / Phosphor**：允许在旧 popup 或过渡阶段存在，但不再是 shell chrome 的目标默认集

这一定义的核心不是“仓库里完全禁用 Lucide”，而是：

- **shell 自己的视觉语言**不再以 Lucide 作为默认 glyph vocabulary
- **应用自己的图标**仍尊重应用自身（尤其是 tray）

### 3. 顶栏改为明显的三层包络

主屏 top bar 的视觉层级固定为：

`bar shell -> group container -> leaf item`

要求：

- 外层 bar 是一整条大圆角悬浮条
- `workspace-strip` 不直接贴 bar，而是放进独立 group container
- `window-info` 不直接贴 bar，而是放进独立 group container
- `tray` 的每个 item 有自己的半透明 chip container
- `battery/network/bluetooth/volume/notification` 不再分散，而是进入一个**统一的 status capsule**

颜色与圆角都必须递进：

- 越外层，面积越大、圆角越大、颜色越接近基础 surface
- 越内层，面积越小、圆角越小、颜色越更实、更明确

### 4. CENTER 时钟重构为三段式胶囊

当前单段 clock 重构为三段并列小胶囊：

- 左段：`MM/DD`
- 中段：`HH:MM`
- 右段：`Tue`

规则：

- 三段必须**整体绝对居中**，不能受 left/right 内容挤压
- 三段允许使用并列但不同语义的表面色
- 中段时间是主阅读对象，视觉权重最高
- 左段日期与右段星期是辅助语义
- 星期段允许使用 warm accent surface，但颜色仍必须来自 `theme_tokyonight.json`

### 5. 右上角状态区重构为统一胶囊

右上角状态区收敛为一个整色圆角胶囊，内部按固定顺序排列：

1. 电池
2. Wi-Fi / 有线网络
3. 蓝牙
4. 音量
5. 通知

bar 内状态区规则：

- **默认无文字**
- **不显示电量百分比**
- **不显示音量百分比**
- **不显示通知数量**
- 日常 idle 状态时，五个 icon 采用统一前景色
- 充电时，battery icon 可单独转为绿色语义
- Wi-Fi / 蓝牙处于主动扫描时，允许蓝色呼吸灯语义
- 有未读通知时，不显示数字，仅在铃铛右上角显示红点

这一决策的目标是把右侧从“五个小工具”变成“一个系统状态对象”。

### 6. tray 保留应用图标，但统一容器语义

tray 不进入统一 status capsule，因为它表示的是**应用外来状态**，不是 shell 自己的系统语义。

但 tray 每个 item 都必须放进一致的淡色半透明 container：

- 统一外轮廓
- 统一 hover / pressed 命中区
- 不强行重绘应用图标

即：

- **icon 不统一**
- **container 统一**

### 7. 状态类 popup 统一扩大为 large panel family

`battery` / `network` / `bluetooth` / `volume` / `notification center` 统一进入更大的 popup family。

目标不是让每个 panel 完全长一样，而是：

- 统一外轮廓
- 统一面板宽度等级
- 统一更大的圆角
- 统一更明显的 shadow
- 统一屏幕边界感知行为

基线要求：

- 新面板尺寸至少达到当前实现的 **2x 级别**
- 宽面板优先，不再沿用“窄高抽屉”观感
- panel 打开后仍必须保持对屏幕边界敏感，不允许超出可点击区域

### 8. bar 与 panel 需要显式景深

bar 与 popup panel 都引入 soft shadow。

此处“要不要阴影”不是开放问题，决策已经确定为：

- **视觉结果必须有阴影**
- 具体用 Qt Graphical Effects、MultiEffect、shader、或 compositor-friendly 等价方案，由实现阶段决定

### 9. 动效本轮冻结，不在本 ADR 收口

本轮只定义静态视觉和结构语义。

以下内容明确**不在本 ADR 内定稿**：

- hover 动效规则
- popup 进入/退出动效细节
- 扫描态 pulse 的具体时间曲线
- bar / panel 的阴影动画

实现阶段只允许做最小必要动画，不得借机重新定义整套 motion language。后续如要系统化重做，另开 ADR。

## Consequences

- `docs/L1-design-language.md` 需要更新 icon strategy、radius scale、top-bar geometry、panel geometry 与 shadow 规则
- `docs/L2-components.md` 需要更新 MainBar 结构、clock 组件、tray、status indicators 与 popup family 规格
- QML 侧后续会引入一轮较大的结构性调整：现有 MainBar 的直接平铺布局不会保留
- `icons/material/` 会成为后续 shell chrome 的主力资源目录
- 这份 ADR 目前是 **Proposed**，用于实现前讨论；用户纠正后再收口为 Accepted

## Non-Goals

本轮不处理：

- 新的动画系统
- 新的色板
- 新的字体系统
- tray 的 daemon 化
- popup 的信息架构大改

## Follow-up

在正式 QML 重构前，至少需要先完成：

1. `docs/L1-design-language.md`
2. `docs/L2-components.md`
3. 本 ADR 的审阅与修正

文档先行，代码后改。
