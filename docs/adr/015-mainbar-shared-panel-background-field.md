# ADR-015: MainBar Shared Panel Background Field

## Status

Accepted

## Context

ADR-014 将 `MainBar` 改成 `exclusive zone window + full-screen overlay field`。

这解决了 popup 宿主拓扑问题，但 Phase 1 仍保留一个局部问题：panel shell 自己同时负责几何、背景绘制和内容承载。这还不是 `caelestia` 的模型。

`caelestia` 的可取点是：

- 一个 overlay field 统一承载 bar 与 panel
- panel 几何是独立模型
- 背景场统一绘制 bar/panel 的复合轮廓
- 内容只是挂在对应 panel slot 内

## Decision

`MainBar` popup family 进入 shared panel background field 模型：

1. `MainBarPanelScene`
   - `MainBarOverlayWindow` 内的统一 panel scene
   - 承载 clock/status 两类 panel 的模型、背景和内容 slot
2. `PanelGeometryModel`
   - 只负责 docked panel 的几何事实
   - 计算 x/y/width/height、body reveal height、bar attachment、shoulder radius
3. `PanelBackgroundField`
   - 统一绘制 `bar shell + active panel shell`
   - 使用 Canvas path 画复合轮廓
   - 当前不使用 GLSL；GLSL 留给后续更复杂的 blob/metaball 背景场
4. `PanelContentSlot`
   - 只负责承载 content-only popup component
   - 不绘制外壳，不拥有 blur，不拥有 dock geometry

`ClockPopup`、`BatteryPopup`、`NetworkPopup`、`BluetoothPopup`、`VolumePopup`、`NotificationCenter` 全部作为 content-only 组件接入 `MainBarPanelScene`。

## Consequences

### Positive

- clock 和 status family 终于共享同一个 panel scene
- panel 几何成为独立事实源，不再散在外壳组件中
- bar/panel 背景由一个 field 绘制，后续可以继续演进成更复杂的曲线/形变系统
- 旧 `StatusDockHost` / `DockedPanelShell` 运行模型被移除

### Negative

- Region 仍使用 Quickshell 的矩形/圆角近似表达，不能完全表达 Canvas 的曲线边界
- 当前 Canvas 方案优先稳定和可控，不提供 shader 级连续 blob deformation

## Notes

如果未来需要真正的 metaball / blob merging：

- 先保持 `PanelGeometryModel` 不变
- 替换 `PanelBackgroundField` 的绘制实现
- 优先考虑 qsb/ShaderEffect 版本，但必须先证明不会重现全桌面卡顿和 effect region 残留问题
