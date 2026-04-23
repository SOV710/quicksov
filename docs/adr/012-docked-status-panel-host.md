#+title: ADR-012 Docked Status Panel Host
#+author: SOV710
#+date: 2026-04-23
#+project: quicksov
#+status: superseded

# ADR-012: Docked Status Panel Host

> Superseded by [ADR-014](./014-mainbar-overlay-field.md) and [ADR-015](./015-mainbar-shared-panel-background-field.md).

## Context

右上角系统状态区此前采用“五个独立 popup panel”方案：

- battery
- network
- bluetooth
- volume
- notification

这一方案虽然易于实现，但在视觉与交互上有结构性缺陷：

- `status capsule` 是 bar 内部一个独立漂浮胶囊，与下方 popup 之间有明确断裂
- 每个 panel 都像独立浮层，而不是同一个系统状态对象的上下两段
- blur region、动画、定位、outside-click 行为在实现上重复分散
- panel 宽高与右侧触发器之间缺少明确的宿主关系

## Decision

右上角状态区改为 **single dock host architecture**：

- `status capsule` 作为 dock host 的顶部 trigger surface
- battery / network / bluetooth / volume / notification 改为 dock host 内部的五个内容页
- 打开任一页时，由同一个 dock shell 从 bar 底部直接展开
- dock host 与 bar 无 gap
- junction 使用 inverse-radius / concave shoulder
- 下方 panel body 保持常规大圆角
- `MainBar` 统一持有 blur attachment；dock host 只提供 shell geometry

## Consequences

正面影响：

- 右上角从“五个孤立工具”收敛为“一个系统状态对象”
- shell、blur、动画、定位、关闭行为都能统一
- 视觉上更符合 docked contextual panel，而不是悬浮弹出层
- 后续若加入更多右上角系统页，可直接复用同一宿主

代价：

- `MainBar` 右侧结构需要重构
- 五个现有 panel 文件需要下沉为 content-only 组件
- blur region 从简单圆角矩形并集变为带 concave geometry 的复合区域

## Implementation Notes

- `clock popup` 不受此 ADR 影响，仍保留独立 panel family
- 右上角 dock host 只服务于 status capsule family
- 设计 token 由 `L1-design-language.md` 定义
- 组件行为由 `L2-components.md` 定义
