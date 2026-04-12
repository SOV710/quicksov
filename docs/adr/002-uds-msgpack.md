# ADR-002: IPC Transport — UDS + MessagePack + u32 LE Framing

**Status**: Accepted  
**Date**: 2026-04-12

## Context

Daemon 与 qs 是同机两进程，需要频繁双向通信：请求-响应、状态广播、一次性命令。通信开销直接影响 bar 的响应延迟和 CPU 占用。协议必须机器可解析（daemon 侧）且便于 QML 侧 JS 解析。

## Decision

- **传输**：Unix Domain Socket，路径 `$XDG_RUNTIME_DIR/quicksov/daemon.sock`
- **序列化**：MessagePack
- **Framing**：每条消息前 4 字节小端 `u32` 表示后续 payload 长度，payload 为 MessagePack 编码的 envelope 对象
- **最大单消息**：16 MiB
- **协议版本**：`qsov/1`，握手时双方声明

## Alternatives Considered

- **D-Bus**：Linux 桌面标准 IPC。但 D-Bus 的 daemon + bus 模型对单对单通信是过度工程；序列化开销（marshaling）显著高于 msgpack；qs 侧直接使用 D-Bus 的复杂度高于自建 UDS 客户端
- **gRPC over UDS**：有强类型 proto schema 的好处，但 QML 侧没有原生 gRPC 支持；额外依赖 protobuf 编译工具链
- **JSON over UDS**：编解码易读，但体积大约是 msgpack 的 1.5-3 倍，且数字精度有 JS 侧浮点问题
- **JSON-RPC / HTTP**：HTTP 开销大，JSON-RPC 额外增加封装层

## Consequences

- 自建协议必须自己维护 schema 文档（`protocol/spec.md`），双侧实现靠人工同步
- MessagePack 的数字类型清晰、二进制高效，qs 侧的 JS 实现可使用 `msgpackr` 或手写 decoder
- UDS 零拷贝、无网络栈开销，性能优于 TCP loopback
- u32 LE framing 简单可靠，标准做法，两侧实现都只需数十行代码
- 无广播开销（D-Bus 对多订阅者的广播更复杂），但 quicksov 的场景里 qs 就是唯一 client（多屏也共用一个 qs 进程），不需要广播
