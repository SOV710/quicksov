# ADR-005: Network Stack — netlink + wpa_supplicant, No NetworkManager

**Status**: Accepted  
**Date**: 2026-04-12

## Context

L0 的宿主系统约束：Gentoo + OpenRC，运行时不依赖 systemd；网络栈是 wpa_supplicant + dhcpcd。作者明确拒绝 NetworkManager 和 iwd。Shell 需要显示的网络信息包括：接口状态（载体、IP）、WiFi 连接状态（SSID、信号强度、扫描结果）、网速。

## Decision

拆分 **`net.link`** 和 **`net.wifi`** 两个独立 service：

- **`net.link`**: 通过 **rtnetlink** 直接订阅 `RTMGRP_LINK` / `RTMGRP_IPV4_IFADDR` / `RTMGRP_IPV6_IFADDR` / `RTMGRP_IPV4_ROUTE` 消息，维护所有接口的 operstate、carrier、IP、路由、字节计数。纯只读，无 actions
- **`net.wifi`**: 通过 `wpa_supplicant` 的 ctrl socket (`/run/wpa_supplicant/wlo1`) 使用 wpa_cli 协议实现扫描、连接、断开、保存网络等操作

dhcpcd 的租约变化不需要显式订阅——租约生效时会通过 netlink 的 ADDR 事件体现，`net.link` 已经在监听该事件。

权限：wpa_supplicant ctrl socket 默认 root-only，通过配置其 `ctrl_interface_group` 为用户所在 group 解决。

## Alternatives Considered

- **NetworkManager D-Bus**：硬性否决，违反 L0 约束
- **iwd D-Bus**：同上
- **调用 `ip` / `iw` / `wpa_cli` 外部命令并解析 stdout**：脆弱，依赖输出格式稳定，解析慢。netlink + ctrl socket 是同一批工具底层使用的接口，没理由绕一层
- **合并 link 和 wifi 为单个 service**：两者后端完全不同、更新频率差距大（link 事件稀疏、wifi 扫描结果可能几 KB 一次），分离更清晰

## Consequences

- 实现复杂度比用 NetworkManager 高——需要自己解析 netlink 消息和 wpa_cli 响应
- 但得到了对宿主栈的精确匹配，无需引入额外守护进程
- 未来切换到 iwd 等其他 WiFi 后端需要新写一个 `net.wifi` service 的 backend 实现，但 state snapshot schema 保持不变
- `rtnetlink` crate 提供了 netlink 的 async 客户端，不需要手写 socket + nlmsg 解析
- `wpa_cli` 协议简单易实现（基于文本命令），几十行代码即可覆盖需要的功能
