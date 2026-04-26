  * [ ] #+title: L0 Primary Context
#+author: SOV710
#+date: 2026-04-02
#+project: quicksov

# L0 Primary Context

本文档固化 quicksov 项目的客观约束。所有后续设计（L1 设计语言、L2 组件布局、L3 技术架构）都必须在此上下文之内进行。

## 1. 宿主系统

- **OS**：Gentoo Linux
- **Init**：OpenRC（运行时**不依赖** systemd；仓库允许同时提供 OpenRC / systemd init artifacts）
- **Compositor**：Niri（Wayland）
- **音频栈**：PipeWire
- **网络栈**：wpa_supplicant + dhcpcd（**无 NetworkManager、无 iwd**）
- **DNS 历史**：resolv.conf 单 nameserver + dhcpcd 管理

任何在运行时强依赖 systemd / NetworkManager / iwd 的方案都必须被拒绝。Daemon 实现网络功能时，除 WiFi 连接管理（通过 wpa_supplicant ctrl socket）外，所有网络状态通过 **netlink** 直接获取。

## 2. 硬件上下文

### 2.1 当前 setup

两块横屏，副屏在左，主屏在中，主屏对齐视线中心：

| 角色 | 设备 | 尺寸 | 分辨率 | DPR | 刷新率 | 位置 |
|---|---|---|---|---|---|---|
| 主屏 | Dell P2418D | 24" | 2560×1440 | 1.25x | 60 Hz | 视线正中 |
| 副屏 | B160QAN02.7 (笔记本内屏) | 16" | 2560×1600 | 2x | 165 Hz | 左侧 |

### 2.2 网络接口命名

- WiFi：`wlo1`
- Ethernet：`enp109s0`

### 2.3 未来扩展假设

纯假设，仅用于约束 Quickshell setup 的可扩展性设计：

- 主屏：Dell UltraSharp U3224KB 31.5" 6144×3456，横屏，视线正中
- 副屏：Dell P2418D 23.5"（当前主屏），**竖屏**，左侧

候选主屏选型参考（基于作者的 logic ppi 脚本筛选）：
31.5" 6K / 32" 8K UHD / 27" 8K / 27" 5K / 39" 5K2K 5120×2160 / 32" 6K / 27" 6K / 43" 4K UHD

当前无具体产品选型，仅作为架构可扩展性的假设依据。

### 2.4 屏幕职责分配

- **主屏**：多个 Neovim coding、Claude Code
- **副屏**：Vivaldi 查文档/RFC/IRC/email/Discord、Emacs org 记笔记

两屏的职责、功能、物理位置、信息消费模式**完全不同**，必须分别设计 bar，**两屏绝对不能共用同一套 bar 配置**。

## 3. 输入上下文

- 主输入：**split 键盘**
- 辅助输入：**轨迹球**
- 交互哲学：**键盘驱动优先**

桌面上的大多数 action 必须能被键盘触发。屏幕上保留的信息只限于**需要被 glance 消费的信息**，button 也需要被设计，但是是二等公民

## 4. Bar 布局约束（由硬件与职责推导）

- **主屏使用悬浮 top-bar**（macOS 式，非贴边），悬浮的外边距由 L1 spacing 定义
- **副屏 bar 可以完全不存在**——副屏不需要 glance 信息。若有 bar，采用 **auto-hide left-bar** 设计，用于消费音乐等非 glance 信息
- 副屏 bar 未来拓展到竖屏时天然适配——left-bar 在纵向屏幕上占用的是最窄的一边

Niri 下的屏幕感知通过 `Quickshell.screens` 的 `Variants` 机制实现，按 DRM connector 名（`DP-1`、`eDP-1`）分发不同的 bar 组件。映射关系由 `daemon.toml` 中的 `[screens.mapping]` 驱动。

## 5. 信息消费清单

### 5.1 主屏必须 glance 的信息

| 项                      | 形态与展开行为                                                            |
|-------------------------|---------------------------------------------------------------------------|
| **clock**               | `YY-MM-DD · HH:MM (UTC+*) · WWW`；click 展开日历 + 天气 widget            |
| **workspace strip**     | 类似 `󱓻 󱓻 󱓻 󱓻 󱓻` 的动态 strip，高亮当前 workspace（正式名称暂定 "strip"） |
| **focused window info** | 例：`Vivaldi | <tab title>`、`GNU Emacs | <filename> - Doom Emacs`        |
| **system tray**         | StatusNotifierItem host                                                   |
| **notification**        | 一个 button，点开展开 notification center；有未读时右上角红点             |
| **battery**             | icon + 百分比；电源状态由 UPower `DisplayDevice` 聚合，power mode 由 power-profiles-daemon 提供 |
| **network**             | WiFi 连接状态 + 信号强度；可 click 展开下拉                               |
| **bluetooth**           | 仅一个 icon，亮暗区分连接状态，动画区分扫描进度/完成；可 click 展开       |
| **volume**              | icon + 总音量百分比；可 click 展开                                        |

### 5.2 主屏明确不需要 glance 的信息

- 音乐
- CPU / RAM 占用
- Power menu

这些以 **auto-hide widget** 形式存在：
- **Power menu** → 主屏底部 auto-hide（底部中心热区触发）
- **音乐面板** → 副屏左侧 auto-hide（左边缘热区触发）

### 5.3 副屏信息

副屏默认无常驻 bar。仅提供左侧 auto-hide music panel。如未来需要看时间，通过视线切换到主屏解决，不在副屏重复时钟。

## 6. 主题与壁纸策略

- **Theme 全局唯一**，不做日夜切换。反复的主题色相变化对人眼不友好
- 壁纸可以做日升日落变化（留待后续实现，不影响 L1 设计语言）

## 7. 命名

- **项目名**：quicksov
- **Git 仓库**：`~/proj/quicksov`
- **运行时根**：`~/.config/quicksov/`
- **Daemon 二进制**：`qsovd`

## 8. 交付物形态

整个项目策划阶段的最终产物是**给 Claude Code 的 implementation prompt**。设计文档（L0/L1/L2/L3 + ADR + protocol spec）是 prompt 的前置材料，不是最终交付物本身。作者本人不直接编写实现代码。
