# ADR-011: MainBar Client-Side Background Effect

**Status**: Accepted
**Date**: 2026-04-23

## Context

此前 quicksov 的主屏 `MainBar` 与其 popup 已具备：

- 浮层几何
- 阴影语言
- 统一的圆角 family

但仍缺少真正的 background blur / frosted glass 材质感。

在 `quickshell master` 与 `niri-git` 下，当前已经具备：

- `ext-background-effect-v1` compositor support
- Quickshell `BackgroundEffect.blurRegion`
- `Region` 的圆角与组合能力

同时，当前 `MainBar` 架构有一个关键事实：

- `clock popup`
- `notification center`
- `battery / network / bluetooth / volume` popup

都不是独立 window，而是挂在同一个 `MainBar.qml` 的 `PanelWindow` 中。

因此，这一轮 blur 的正确实现单位不是“每个 popup 各自挂 blur”，而是：

- 由 `MainBar` 这个 `PanelWindow` 统一挂一份 client-side background effect
- 用精确 `Region` 表达当前需要 blur 的外壳几何

## Decision

### 1. Blur 只挂在 MainBar window

`MainBar.qml` 的 `PanelWindow` 作为唯一的 blur protocol attachment owner。

这份 blur region 由以下几部分组成：

- `barRect`
- 当前可见的 `clock popup` shell
- 当前可见的 `notification center` shell
- 当前可见的 `battery / network / bluetooth / volume` shell

不在每个 popup 内重复附着 `BackgroundEffect`。

### 2. Blur region 只覆盖视觉外壳

blur region 必须只覆盖真正的 shell geometry：

- 包含：外壳圆角矩形
- 不包含：shadow 投影
- 不包含：window 中用于 outside-click dismiss 的透明命中区域
- 不包含：bar / popup 外的透明留白

也就是说，视觉外壳、点击捕获区、shadow 不再混成同一层几何概念。

### 3. 材质透明度由 shell fill 承担，不由整块 item opacity 承担

steady-state 的 glass 感来自：

- 半透明 shell fill
- compositor blur behind region
- 轻描边与内部更实的内容卡

而不是让整个 popup / bar 在稳定状态下保持 `< 1.0` 的整体 opacity。

因此：

- shell 外壳颜色本身必须带 alpha
- 文本、icon、内部 card 在稳态下保持正常不透明度
- item `opacity` 只承担进入/退出动画，不承担材质语义

### 4. Blur 强度仍由 compositor 决定

这轮不引入 daemon 协议扩展，也不尝试从 shell 端精细控制 blur passes / radius。

当前实际能力边界是：

- shell 可以决定**哪里**请求 blur
- compositor 决定 blur 的**强度与算法**

因此 Theme 中与 blur 相关的视觉控制，当前只落在：

- shell fill alpha
- border 对比度
- 内容卡明暗递进

而不是协议层面的 blur radius。

### 5. 本轮范围只覆盖 MainBar family

本轮 blur 只覆盖：

- 主屏 `MainBar`
- 由 `MainBar` 展开的 `clock popup`
- 右上角 status popup family

不包括：

- `AuxBar`
- `PowerDock`
- wallpaper renderer
- 其它独立 window / layer surface

## Consequences

- `docs/L1-design-language.md` 需要把 bar/popup shell 记录为 glass-like material，而不是纯实色面
- `docs/L2-components.md` 需要明确 `MainBar` family 的 blur attachment 与 region 语义
- `shell/Theme.qml` 需要把 shell fill 与稳态整体 opacity 解耦
- `shell/bars/MainBar.qml` 需要成为 blur region 的唯一汇总点
- 各 popup 组件需要暴露统一的 shell geometry 引用，供 `MainBar` 组装 region
