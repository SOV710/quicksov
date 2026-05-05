# ADR-017: Battery Popup Liquid Panel

## Status

Accepted (retroactive)

## Date

2026-05-05

## Context

ADR-010 已经把 `battery / network / bluetooth / volume / notification` 收回到 compact status panel family，ADR-015 又把这些 popup 接入 `MainBarPanelScene`，让内容页不再负责外壳、阴影和 dock 几何。ADR-016 则把 battery 数据面收敛到 UPower 与 power-profiles-daemon。

这几条决策解决了宿主拓扑和数据来源，但还没有记录 battery popup 的产品表达方式。

Battery panel 需要在紧凑面板里表达三类信息：

- 当前电量与供电状态
- 可量化但次要的电池健康与能量信息
- 可操作的 power profile 控制

如果继续使用文字密集列表或普通进度条，battery panel 会变成一块解释性 dashboard，和 quicksov 当前的视觉方向冲突。这个 shell 的状态类 popup 应优先通过 icon、动效、图形化量表和少量必要数字传达状态；文字只用于不可用、错误或需要明确解释的控制状态。

## Decision

Battery popup 采用 liquid hero + mini gauges + snap control 的三段式内容模型。

1. 首段是每个 present battery 一张 full-width liquid hero card
   - 使用从左向右推进的液态电量场，而不是普通水平进度条
   - 液面持续轻微波动，数值变化时波锋水平推进
   - front edge 需要柔和且上下不对称：上部更收敛，下部液体体量更厚、更靠前
   - 液体颜色从主题色系统推导，并按 `charging / fully charged / discharging / unknown` 等状态切换语义色
   - 渐变方向需要强化电量从左到右推进的视觉方向
2. hero 下方只保留必要文本和图标
   - 左侧显示大号百分比，例如 `87%`
   - 可追加灰色小字电池名，例如 `BAT0`
   - 右侧用 source icon 表达 `battery / power`
   - `charging` 与 `fully charged` 用小 badge icon 表达，不使用长文案
3. 第二段只显示两个 mini radial gauges
   - `Battery Health`
   - `Energy`
   - 每个 gauge 使用中心主值、小号单位、简化环形进度和底部标签
   - 默认不加入温度、电压、循环次数等更多指标，避免紧凑面板变成诊断页
4. 第三段是 `Power Mode` snap slider
   - 档位来自 daemon 暴露的可用 `power_profile` 列表，支持二档或三档
   - 每档使用 icon + label
   - thumb 在档位间吸附，支持点击和拖动
   - 若 power profile 后端不可用、权限不足、写入失败或设备不足两档，控制项保留但整体 disabled，并显示 warning message
   - `performance` degraded 时复用同一区域显示系统限制原因
5. 空状态和错误状态不伪造指标
   - `No battery detected` 与 `Battery backend unavailable` 分开呈现
   - 无电池或后端不可用时省略 hero 与 gauges，只保留状态卡和可解释的控制状态
6. 本次决策不纳入亮度控制
   - 亮度可以作为未来 power-related control 集成，但不属于当前 battery popup 的默认内容
7. `BatteryPopup` 仍是 content-only 组件
   - panel 外壳、背景、边界回推与关闭行为继续由 `MainBarPanelScene` / `PanelBackgroundField` 负责
   - battery 内容页不创建自己的 top-level window，也不绘制共享 panel shell

## Consequences

### Positive

- battery popup 从文字解释型面板转为图形优先、可快速扫描的状态面板
- 电量、健康值、能量和 power profile 分层清楚，适合 compact status panel 的信息密度
- 多电池设备可以自然通过 hero card repeat 表达，而不是压进单个聚合条
- UI 直接匹配 ADR-016 的 UPower / power-profiles-daemon 数据模型
- 不可用状态仍然清晰，不需要在正常状态里引入大量说明文字

### Negative

- liquid hero 需要 shader 或等价图形实现，渲染复杂度高于普通进度条
- battery popup 的视觉定制程度高于其他 status popup，需要通过主题 token 约束住色彩和尺寸
- 默认只展示 `Battery Health` 与 `Energy`，会刻意隐藏一部分诊断型电池信息
- 少文字策略要求 icon、badge 和 disabled state 足够稳定，否则可理解性会下降

## Notes

这是一篇追认 ADR，用于记录已经落到 `docs/L2-components.md` 与 `shell/overlays/BatteryPopup.qml` 的 battery panel refactor。后续如果要加入亮度、温度、电池循环次数或更底层平台调节项，应另开 ADR 或更新本 ADR，而不是把当前 compact battery popup 扩成通用 power dashboard。
