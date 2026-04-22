# ADR-010: Compact Status Panels for Right-Side Popups

**Status**: Accepted
**Date**: 2026-04-22
**Supersedes**: ADR-009 section 7 (`large panel family`) for `battery` / `network` / `bluetooth` / `volume` / `notification center`

## Context

ADR-009 把右上角状态类 popup 统一推向了 oversized / wide panel 路线。

这条路线在讨论阶段成立，但落到实际 UI 后暴露了两个问题：

1. 右上角 popup 并不适合长期保持超宽面板，视觉上显得笨重、发散，而且会压过 top bar 本身的层级
2. `bluetooth` / `network` 这类 panel 的真实信息密度是**动态高度优先**，不是“默认就应大面积展开”

当前更合适的方向不是继续把所有状态类 popup 做成“宽抽屉”，而是：

- 保留统一 family
- 保留统一圆角、阴影、间距、边界感知
- 但将几何规格收回到**紧凑型 status panel**

## Decision

### 1. 右上角状态类 popup 改为 compact family

`battery` / `network` / `bluetooth` / `volume` / `notification center` 统一使用 compact status panel family。

新的基线规格：

- `status_panel_width = 440px`
- `status_panel_max_height = 380px`
- 仍保持 `panel_edge_inset = 24px`

它们继续共享：

- `radius.xl` 外轮廓
- popup shadow
- bar 下方 `gap_from_bar`
- 统一的边界回推逻辑

### 2. 右上角 panel 以内容自适应为主，不追求“默认很大”

这组 panel 的优先级是：

1. 信息清晰
2. 默认体量克制
3. 在需要时通过列表区增长

因此：

- `battery` 保持较低默认高度
- `network` / `bluetooth` 在无扫描、无大列表时保持紧凑
- `volume` / `notification` 的列表区上限也同步收回

### 3. 右上角 popup 不再沿用“宽面板”设计语义

clock popup 仍是独立 family，可以保持大 panel。

但右上角 status popup 必须回到：

- 更窄
- 更短
- 更像 anchored utility panel

而不是 dashboard / side sheet。

## Consequences

- `docs/L1-design-language.md` 的 status panel family 几何需要改为 compact 规格
- `docs/L2-components.md` 里关于 `battery` / `network` / `bluetooth` / `volume` / `notification` 的 popup 描述，需要补上“紧凑、动态高度优先”的语义
- `shell/Theme.qml` 的统一 panel token 需要回调，避免各 popup 局部继续维持 oversized 尺寸
- 这次调整只影响右上角 status popup family，不影响 clock popup family
