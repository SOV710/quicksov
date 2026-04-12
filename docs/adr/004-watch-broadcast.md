# ADR-004: State Distribution — `tokio::sync::watch` for State, `broadcast` for Discrete Events

**Status**: Accepted  
**Date**: 2026-04-12

## Context

L2 的"订阅时立即推送当前快照"需求需要一种机制：qs 新订阅一个 topic 后，daemon 必须在不等任何事件的情况下立即把当前状态发给 qs。同时，状态高频变化（电量、音量、CPU）时，中间态可以被合并——qs 只需要看到最新值，不需要看到每一次变化。

另一类通信是**不可合并**的离散事件——最典型的是通知到达。不能用"最新覆盖"模型，每条通知都必须送达。

两种语义必须分别处理。

## Decision

对 **状态（state）** 使用 `tokio::sync::watch`：
- Service 持有 `watch::Sender<StateSnapshot>`，用 `send_replace` 发布最新值
- Router 为每个订阅 session clone 一个 `watch::Receiver`
- 新订阅者 clone 后第一次 `borrow()` 或 `changed().await` 就能读到当前值，天然实现"订阅即推快照"
- 多次 send 之间如果 receiver 尚未处理，会自动合并——只会看到最新值

对 **离散事件（events）** 使用 `tokio::sync::broadcast`（仅 `notification` service 需要）：
- Service 持有 `broadcast::Sender<NotificationEvent>`
- Router 为每个订阅 session clone 一个 `broadcast::Receiver`
- 缓冲满时采用 lag 策略（receiver 被标记 `Lagged(n)`，跳过 n 条旧消息），初版可以接受
- 每条通知独立传递，不被合并

## Alternatives Considered

- **只用 `broadcast`**：对状态场景过度——快速变化的电量值会在 broadcast 队列里堆积，浪费内存且 qs 看到一堆已经过时的值
- **只用 `watch`**：对通知场景错误——快速到达的多条通知会被合并，后来的覆盖前面的，造成通知丢失
- **手写环形缓冲**：功能上可以同时覆盖两种语义，但实现成本不值得，tokio 的原语已经足够

## Consequences

- 两种通道语义清晰，各司其职
- `ServiceHandle` 的 `events_tx` 字段是 `Option`，绝大多数 service 为 `None`
- Router 转发 `watch` 和 `broadcast` 的代码路径不同，但都是简单的 spawn forwarder task 模式
- 广播通道的 lag 处理需要在实现阶段决策（丢弃 vs 报警），初版采用简单丢弃 + 日志警告
