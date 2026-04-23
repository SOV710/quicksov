# ADR-014: MainBar Overlay Field Topology

## Status

Accepted

Superseded in part by [ADR-015](./015-mainbar-shared-panel-background-field.md), which keeps this window topology and replaces the Phase 1 local shell with a shared panel background field.

## Context

此前 `MainBar` popup 体系长期停留在“bar window + popup host + dismiss window”的修补式结构：

- `clock` 与右上角 status family 不是同一个拓扑模型
- outside-click capture、blur region、popup shell geometry 分散在多个对象里
- panel 轮廓很容易退化成“额外大矩形 + 局部裁切”
- 想做 `caelestia` 风格的 docked contextual panel 时，几何、输入、effect region 会互相打架

`caelestia` 的核心经验不是某一段 shader，而是：

- 用一个**全屏交互场**承载 bar 与 panel
- bar / panel 只是这个场里的锚点与可见轮廓
- outside-click capture 与 shell geometry 分离

同时，quicksov 仍然需要保留主屏 bar 的 layer-shell exclusive zone。

## Decision

主屏 `MainBar` family 改为两层窗口拓扑：

1. `MainBarExclusiveZoneWindow`
   - 每屏一个
   - 只负责 exclusive zone
   - 不承担 popup、blur、dismiss、交互
2. `MainBarOverlayWindow`
   - 每屏一个 full-screen overlay field
   - 负责渲染 bar
   - 负责承载 `clock` 与右上角 status family 的全部 popup panel
   - 负责统一 blur region
   - 负责 outside-click dismiss

popup 不再建模为独立 panel window，而是 `MainBarOverlayWindow` 内的 child shell：

- `clock` 与右上角 status family 都使用同类 docked panel shell
- shell 几何直接参考 bar 的附着关系计算
- shell 轮廓在自身 bounds 内完成，不再通过“外扩矩形 + 外部补形”模拟

## Consequences

### Positive

- `clock` 与 status family 终于进入同一套宿主模型
- blur region 只有一个 owner，effect region 更容易收敛
- outside-click dismiss 可以通过 overlay field 统一处理
- panel 轮廓与 bar 的关系更接近 `caelestia`
- 允许继续演进到 shared panel background field，而不必再拆旧 popup window

### Negative

- `MainBar` 的 QML 结构会发生 breaking change
- popup 打开时 overlay mask 会切到 full-screen capture，这要求交互层与可见 shell 分离设计
- ADR-012/013 的 `StatusDockHost` 方案被后续 ADR 保留为历史记录，不再代表当前实现

## Notes

- 这次 ADR 不要求引入 shader / blob background
- 这次 ADR 不改变 popup 的业务内容，只改变宿主拓扑与几何所有权
- shared panel background field 已在 ADR-015 中基于这个 overlay field 模型推进
