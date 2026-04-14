# ADR-006: Quickshell Native Integration Boundary — tray stays in Quickshell, mpris stays in daemon

**Status**: Proposed  
**Date**: 2026-04-14

## Context

当前文档存在边界冲突：

- `docs/L2-components.md` 将 **tray** 的数据源定义为 `Quickshell.Services.SystemTray`，将副屏 **music-panel** 的数据源定义为 `Quickshell.Services.Mpris`
- `docs/L3-architecture.md` 与 `protocol/spec.md` 又将 **`tray`** 和 **`mpris`** 都定义为 daemon topic / service，由 Rust daemon 统一对外提供 IPC

这不是实现细节分歧，而是**系统边界分歧**：到底由 Quickshell 直接接系统 API，还是由 daemon 统一抽象后再通过 IPC 暴露给 QML。

需要分别判断 `tray` 和 `mpris`：两者虽然都来自桌面总线，但约束并不相同，不能被打包成同一种决策。

## Decision

### 1. `tray` 不进入 daemon，保留为 Quickshell 原生能力

`tray` 的**唯一权威实现**放在 QML / Quickshell 侧，直接使用 `Quickshell.Services.SystemTray`。

因此：

- `protocol/spec.md` 中删除 `5.8 tray`
- `protocol/schema.json` 中删除 `tray`
- `docs/L3-architecture.md` 中删除 `tray` topic、`services/tray/`、`Tray.qml` 对 daemon topic 的依赖描述
- `docs/L2-components.md` 中保留 `Quickshell.Services.SystemTray` 作为数据源，并将交互描述改为 Quickshell 的真实 API（`display()` / `menu`），而不是伪接口名

### 2. `mpris` 保留为 daemon service

`mpris` 的**规范实现**继续放在 daemon，QML 通过 IPC 订阅 `mpris` topic。

因此：

- `protocol/spec.md` 中的 `5.6 mpris` 保持存在，作为规范来源
- `docs/L3-architecture.md` 中保留 `mpris` topic / `services/mpris/`
- `docs/L2-components.md` 中将 music-panel 的数据源从 `Quickshell.Services.Mpris` 改为 daemon `mpris` service
- `docs/L2-components.md` 末尾“也可让 qs 直接用 Quickshell.Services.Mpris”脚注删除，避免双轨设计

## Rationale

### 为什么 `tray` 必须留在 Quickshell

`Quickshell.Services.SystemTray` 不只是“读状态”的 helper，而是 Quickshell 进程内的系统托盘实现入口。它暴露的关键对象包括：

- `SystemTray.items`：当前所有 tray item 列表
- `SystemTrayItem.menu`：与 item 绑定的菜单句柄
- `SystemTrayItem.display(parentWindow, relativeX, relativeY)`：在给定窗口与相对坐标处显示原生菜单
- `SystemTrayItem.activate()` / `secondaryActivate()` / `scroll()`：交互动作

这里的 `menu` / `display(parentWindow, ...)` 明确依赖 **QML 窗口对象** 与 **本地 UI 进程内句柄**。这类对象不能自然地序列化为 msgpack 后跨 UDS 传给 daemon 再回到 QML。

如果 daemon 同时实现 StatusNotifier host，而 Quickshell 又引用 `SystemTray` singleton，则会出现：

- host / watcher 责任重复
- 菜单所有权不清晰
- `open_menu` 的真正展示位置仍然必须回到 QML 进程解决

所以 `tray` 不适合作为 daemon topic。它属于 **UI 所在进程必须直接持有的桌面集成能力**。

### 为什么 `mpris` 可以继续留在 daemon

`Quickshell.Services.Mpris` 的 API 已足够完成当前 music-panel UI：

- `Mpris.players`
- `MprisPlayer.dbusName`
- `identity` / `trackTitle` / `trackArtist` / `trackAlbum` / `trackArtUrl`
- `playbackState` / `position` / `length`
- `togglePlaying()` / `next()` / `previous()` / `stop()` / `seek()`

但它并不能无损对齐当前协议设计：

- protocol 中的 `artist` 是 `string[]`，而 Quickshell 暴露的高层便捷字段是 `trackArtist: string`；`trackArtists` 还是 deprecated
- protocol 中有显式 `active_player` / `select_active` 语义，Quickshell 没有直接提供，需要 QML 本地再造一层策略状态
- protocol 目标是给 QML 之外的客户端也提供统一状态面与动作面；daemon 更适合做归一化和策略收口

与 `tray` 不同，`mpris` 的状态与动作都是普通可序列化数据：

- 状态：播放器列表、元数据、位置、播放状态
- 动作：play/pause/next/previous/seek/set_position/select_active

不存在必须绑定某个 QML window handle 的对象边界问题。

因此，`mpris` 保留在 daemon 内是合理的，代价只是需要手写 D-Bus 追踪与归一化逻辑。

## Alternatives Considered

### A. `tray` 和 `mpris` 都放进 daemon

否决。

`mpris` 可行，但 `tray` 的菜单与 host 语义不适合穿过 IPC。继续坚持该方案会迫使协议为“菜单句柄”“窗口定位”“临时 UI 对象”发明额外抽象，复杂度与脆弱性都很高。

### B. `tray` 和 `mpris` 都直接由 Quickshell 提供

部分可行，但最终否决。

这能快速做出 UI，但会削弱 daemon 作为统一状态中台的角色。对 `mpris` 来说，协议已经存在，且状态/动作天然可序列化，没有必要放弃 daemon 这一层。

### C. 双轨：QML 可选直连 Quickshell，也可订阅 daemon

否决。

双轨会带来两套事实来源：

- 谁定义 active player
- 谁决定去抖 / 归一化 / 容错
- 谁是最终调试入口

对单人项目，这种灵活性没有收益，只会放大维护成本。

## Consequences

- 下一轮文档修订必须先完成，之后才能写“实现所有 service”的 Phase prompt
- `tray` 不再属于 daemon 服务集合，Phase 3/4 的“all services”范围需要重新定义为：**所有 daemon-owned services**，不包括 Quickshell-native integration
- `mpris` 继续由 daemon 提供，副屏 music-panel 通过 IPC 使用 `mpris` topic
- QML 侧需要一个直接依赖 `Quickshell.Services.SystemTray` 的 `TrayHost` / `TrayModel`，而不是 IPC wrapper
- `protocol/spec.md` 与 `schema.json` 将因删除 `tray` topic 而发生一次破坏性变更；应在提交说明中明确

## Follow-up

在任何新的实现 prompt 之前，先做一个 **docs-only 修订提交**，至少包括：

1. `docs/L2-components.md`
2. `docs/L3-architecture.md`
3. `protocol/spec.md`
4. `protocol/schema.json`

待文档收口后，再撰写“实现所有 daemon-owned services”的下一阶段 prompt。
