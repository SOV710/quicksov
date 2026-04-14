# ADR-008: IPC Transport — UDS + NDJSON over UTF-8

**Status**: Accepted  
**Date**: 2026-04-14

## Context

Phase 4 暴露了一个关键实现约束：Quickshell v2.1 QML 侧的 `Quickshell.Io.Socket.write()` / `Process.stdin.write()` 路径天然偏向字符串流，直接承载二进制 MessagePack 成本高，最终导致引入 Python bridge 才能与 daemon 通信。

bridge 方案有三个问题：

1. **多一层进程与两次序列化**：daemon ↔ bridge 仍是 msgpack，bridge ↔ QML 再转 JSON lines；复杂度和故障面都扩大。  
2. **违背最小原型目标**：最小 Quickshell 原型应优先减少运行部件，而不是增加一个常驻桥接进程。  
3. **QML 实现体验差**：QML/JS 天然更适合处理结构化文本 JSON，而不是自行维护二进制 framing + msgpack codec。

当前协议的 envelope / topic / action / error code 设计本身没有问题，问题只在**传输表示层**。

## Decision

- **传输**：继续使用 Unix Domain Socket，路径不变：`$XDG_RUNTIME_DIR/quicksov/daemon.sock`
- **序列化**：将 daemon ↔ qs 的线协议从 MessagePack 改为 **UTF-8 JSON**
- **Framing**：改为 **NDJSON**（newline-delimited JSON），即每条消息一行 JSON，行尾 `\n`
- **最大单消息**：16 MiB（单行 UTF-8 字节数）
- **协议版本**：仍为 `qsov/1`
- **Envelope / Hello / HelloAck / ErrorBody 的字段名、语义、topic 列表、action 名称一律不变**
- **数值约束**：所有出现在 JSON 线协议中的整数必须保持在 JavaScript safe integer 范围内；`id` 与 `session_id` 继续使用 JSON number，但不得生成超出 safe integer 的值

## Alternatives Considered

- **继续使用 UDS + MessagePack + Python bridge**：功能可行，但进程数、维护成本、调试复杂度都显著更高；不符合最小原型阶段目标
- **继续使用 UDS + MessagePack，但在 QML 侧补二进制 codec**：理论可行，实践上会把大量精力耗在 QML/JS 二进制处理而不是 shell 本身
- **JSON + u32 length prefix**：比 msgpack 更容易调试，但在 Quickshell 侧依然需要自己处理长度前缀，不如 NDJSON 直接对接 `SplitParser`
- **HTTP / WebSocket / JSON-RPC**：引入额外协议层，没有必要

## Consequences

- `docs/adr/002-uds-msgpack.md` 将被 **ADR-008 supersede**；保留存档，但不再作为现行决策
- `protocol/spec.md` 与 `protocol/schema.json` 需要同步改写传输描述：MessagePack → JSON，u32 framing → NDJSON
- Rust 侧 IPC / protocol / bus / service payload 推荐统一迁移为 `serde_json::Value`
- `rmp-serde` / `rmpv` 应从依赖和代码中移除
- QML 侧删除 Python bridge，改为直接使用 `Quickshell.Io.Socket` + `SplitParser` + `JSON.parse/stringify`
- 人类调试体验显著提升：`socat` / `nc -U` / 手写一行 JSON 即可直测协议

## Notes

本 ADR 只改变**线协议表示**，不改变 quicksov 的业务模型。topic schema、错误码、握手语义、REQ/REP/ERR/PUB/SUB/UNSUB/ONESHOT 语义保持不变。
