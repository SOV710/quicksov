#+title: ADR-013 Status Dock Host Correction
#+author: SOV710
#+date: 2026-04-23
#+project: quicksov
#+status: accepted

# ADR-013: Status Dock Host Correction

## Context

ADR-012 将右上角状态区改造成 `StatusDockHost`，但第一版把 dock host 错误地与 `status capsule` 做了完全融合：

- panel shell 与 `status capsule` 同色
- panel junction 对准 `status capsule`
- `status capsule` 被拉伸成与 bar 等高

这导致视觉层级判断错误：真正应该与 panel 形成连续关系的是 **MainBar 本体**，而不是 bar 内部的一个小胶囊。

## Decision

保留统一 `StatusDockHost` 架构，但修正其视觉连接关系：

- dock host 仍然是右上角五个系统页的统一宿主
- panel shell 的 fill / border 改为与 `bar shell` 同源
- panel junction 改为与 **bar 本体底边** 形成 inverse-radius / concave transition
- `status capsule` 恢复为 bar 内部嵌入式 trigger，不再上下顶住 bar

## Consequences

正面影响：

- bar 与 dock panel 的主次关系恢复正确
- `status capsule` 回到“内部控件”而不是“上半块外壳”
- dock shell 与 MainBar 的玻璃材质统一
- 仍保留单一宿主、单一 blur attachment、单一 drawer reveal 的实现收益

代价：

- `MainBar` 与 `StatusDockHost` 的职责边界需要再次调整
- 需要撤回一部分 ADR-012 的几何表述

## Implementation Notes

- `StatusDockHost` 只负责 bar 下方的 dock shell 与内容页
- `status capsule` 回到 `MainBar` 内部布局
- blur region 继续由 `MainBar` 统一请求，但 dock shell 的复合轮廓改为“panel body - top corner cutouts”
