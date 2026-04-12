# ADR-003: Service Abstraction — Actor Tasks, No Trait

**Status**: Accepted  
**Date**: 2026-04-12

## Context

Daemon 内有约 10 个 service，每个都需要维护自己的状态、订阅后端事件、响应 router 转发来的请求。最初的设计提议定义一个 `trait Service` 让 router 持有 `Box<dyn Service>`，但该设计存在两个根本问题：

1. **Object safety**：`async fn` in trait 返回 `impl Future`，使 trait 不 dyn-compatible。使用 `#[async_trait]` 宏可以绕过但带来每次调用一次堆分配开销
2. **借用冲突**：若 router 通过 `handle_request(&mut self, ...)` 调用 service，它需要独占引用；但 service 同时在自己的事件循环里持有 `&mut self`——这两者无法同时成立

这两个问题指向同一个错误：把 service 当成"被 router 调用方法的对象"是 OOP 思维，不符合 tokio 的并发模型。

## Decision

每个 service 是一个独立的 **tokio task**。对外只暴露一个**具体结构体** `ServiceHandle`，不定义任何 trait。

```
ServiceHandle {
    request_tx:  mpsc::Sender<ServiceRequest>     // REQ/ONESHOT
    state_rx:    watch::Receiver<StateSnapshot>   // 当前状态快照
    events_tx:   Option<broadcast::Sender<Event>> // 仅通知类 service 需要
}
```

Router 持有 `HashMap<String, ServiceHandle>`，所有交互通过 channel 进行，永远不共享内存。每个 service 的主循环是一个 `tokio::select!`，同时处理来自 `mpsc` 的请求和来自后端的事件——所有状态都是 task 局部变量，不跨 await 被外部借用。

Service 的"注册"通过一个手写的 `start_services(cfg, bus)` 函数完成，内部逐个调用每个 service 模块的 `spawn(config) -> ServiceHandle`，组装成 HashMap 返回。

## Alternatives Considered

- **`trait Service` + `Box<dyn>`**：如 context 所述，object safety 与借用冲突无法同时解决
- **`Arc<Mutex<Service>>`**：把状态包在锁里让多方共享。但每个请求都要获取锁，和 service 自身的事件循环争用同一把锁，性能和正确性都不理想
- **Actor 框架（actix / ractor / kameo）**：提供更多机制（supervision、mailbox 溢出策略等），但为 10 个内部 actor 引入第三方框架是过度工程。本项目的 actor 模型非常简单，tokio primitive 已足够

## Consequences

- **优点**：没有任何堆分配和虚调用开销；每个 service 的状态类型可以完全不同，保留强类型；事件循环和请求处理在同一 select! 里，不存在争用；新人阅读代码时无 trait 抽象层
- **代价**：`start_services` 函数随 service 数量线性变长，新增 service 必须显式注册。但这是"显式优于隐式"的合理代价，且对单人项目是好事——唯一的事实来源，不会漏
- Router 变得非常瘦，只是 HashMap 查找 + channel 转发
