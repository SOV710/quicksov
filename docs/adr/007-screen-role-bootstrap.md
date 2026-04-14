# ADR-007: Screen role bootstrap rides on `meta`, not `theme` and not hardcoded output names

**Status**: Accepted  
**Date**: 2026-04-14

## Context

`docs/L2-components.md` 已经要求：

- Quickshell 通过 `Quickshell.screens` 感知当前输出设备
- 哪块屏幕加载 `MainBar`、哪块屏幕加载 `AuxBar`，由 `daemon.toml` 的 `[screens.mapping]` 驱动
- 该映射需要由 daemon 推送给 qs

但当前文档与协议还没有把这件事落到一个明确的数据面上：

- `theme` topic 的职责是设计 token，不应混入运行时屏幕拓扑
- `meta` topic 当前只有版本、uptime、service 健康，没有屏幕角色映射
- `protocol/spec.md` / `schema.json` 没有一个字段让 qs 知道 `DP-1` 是 `main`、`eDP-1` 是 `aux`

如果没有这层 bootstrap，qs 只能：

1. 把输出名写死在 QML 里；或
2. 直接读取 daemon.toml。

前者违背配置驱动设计；后者把 daemon 配置解析逻辑重复实现到 QML，破坏职责边界。

## Decision

### 1. 屏幕角色映射通过 `meta` topic 推送

`meta` state snapshot 增加一个字段：

```json
{
  "screens": {
    "type": "object",
    "properties": {
      "roles": {
        "type": "object",
        "description": "output name -> role",
        "additionalProperties": {
          "type": "string",
          "enum": ["main", "aux"]
        }
      }
    },
    "required": ["roles"]
  }
}
```

语义：

- key 是 Niri / Quickshell 看到的输出名，如 `DP-1`、`eDP-1`
- value 是 UI 角色，如 `main`、`aux`
- 该对象由 daemon 启动时从 `daemon.toml.[screens.mapping]` 派生

### 2. `theme` 保持纯设计 token

`theme` topic 继续只承载 design token，不混入任何运行时拓扑、screen mapping、service 状态。

### 3. Quickshell 以 `meta.screens.roles[modelData.name]` 判定加载哪个 bar

QML 侧不得硬编码：

- `if (screen.name === "DP-1") ...`
- `if (screen.name === "eDP-1") ...`

必须通过 `meta` snapshot 的 `screens.roles` 做角色选择。未知输出默认不加载 bar。

## Rationale

### 为什么不是 `theme`

`theme` 是视觉 token 的唯一事实来源。把运行时拓扑塞进 `theme` 会导致：

- 视觉配置与硬件拓扑耦合
- 主题热重载与屏幕角色判断耦合
- `Theme.qml` 既做视觉又做拓扑 bootstrap，职责变脏

### 为什么不是 QML 直接读 `daemon.toml`

`screens.mapping` 已经是 daemon 配置的一部分。让 qs 再解析一遍 TOML 会带来：

- 双份配置解析逻辑
- 环境变量展开规则重复
- 配置热重载行为不一致

### 为什么放在 `meta`

`meta` 本来就是 daemon 自身运行时信息的 topic：版本、uptime、service 健康、是否需要重启。屏幕角色映射同样属于“shell bootstrap 所需的 daemon 元信息”。它应当跟 `config_needs_restart` 一起由 `meta` 暴露。

## Alternatives Considered

### A. 继续在 QML 里写死 `DP-1` / `eDP-1`

否决。

这只适合一次性原型，不适合当前文档已经承诺的配置驱动架构。

### B. 把 screen mapping 混入 `theme`

否决。

这会污染 design token topic，并让 `Theme.qml` 承担不必要的职责。

### C. 新建独立 topic，例如 `shell.layout`

暂不采用。

当前只有屏幕角色映射这一个 bootstrap 需求，放在 `meta` 即可。单独拆 topic 会增加一次额外订阅、额外 schema 和额外心智成本。

## Consequences

- `protocol/spec.md` 与 `protocol/schema.json` 的 `meta` snapshot 需要同步更新
- `docs/L2-components.md` 需要把“由 daemon 通过 theme 或 meta 推送”收紧为“由 daemon 通过 meta 推送”
- `docs/L3-architecture.md` 需要把 screen-role bootstrap 明确记到 `meta` topic / shell 启动流程中
- daemon `meta` service 需要从已解析配置中派生 `screens.roles`
- Quickshell 首次实现时，screen role 选择必须依赖 `meta`

## Follow-up

在 Quickshell 第一阶段实现前，先做一个 docs-first 提交，至少同步修改：

1. `docs/L2-components.md`
2. `docs/L3-architecture.md`
3. `protocol/spec.md`
4. `protocol/schema.json`

随后再做一个小提交，把 daemon `meta` snapshot 扩展为包含 `screens.roles`。
