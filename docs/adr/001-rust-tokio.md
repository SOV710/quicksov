# ADR-001: Daemon Language and Runtime — Rust + Tokio

**Status**: Accepted  
**Date**: 2026-04-12

## Context

quicksov daemon 需要承担所有系统集成的长期运行任务：D-Bus（UPower/BlueZ/MPRIS/Notification/Tray）、netlink（link/addr/route 事件流）、wpa_supplicant ctrl socket、PipeWire client、Open-Meteo HTTP 轮询，以及面向 qs 的 UDS server。所有任务都是 I/O 密集且高度并发的。

宿主环境为 Gentoo + OpenRC，运行时不依赖 systemd，无 NetworkManager；仓库可以同时携带 OpenRC / systemd 的部署文件。作者本人熟悉的系统语言为 Rust、C、TypeScript、Go。

## Decision

使用 **Rust** 实现 daemon，并发运行时采用 **Tokio**。

## Alternatives Considered

- **Go**：语法简单、编译快、D-Bus 和 netlink 生态良好，DMS 的选择即是 Go。但作者对 Rust 更熟悉，且 Rust 的类型系统更有利于 actor 式 service 模型中严格的数据流。
- **Python**：启动快、开发成本极低。但长期驻留进程的 GC 压力、以及 async 生态的碎片化不适合作为长期运行的系统组件。
- **C/C++**：可行但手动内存管理在单人项目中带来不必要的维护成本。
- **async-std / smol**：Rust 的其他 async runtime，但 `zbus` / `rtnetlink` / `reqwest` 等关键依赖的 tokio 集成最成熟。

## Consequences

- Daemon 启动时间会比 Go 稍慢（几十 ms 级），对桌面启动无感
- 所有库选型必须 tokio 兼容，个别 sync-only 库需要 `spawn_blocking` 包装
- 未来加入其他贡献者的门槛较高，但本项目是单人项目，不构成问题
- 获得强类型 + 所有权带来的并发正确性保证
