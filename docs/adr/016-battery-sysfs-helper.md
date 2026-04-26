# ADR-016: Battery and Power Profiles via UPower + power-profiles-daemon

## Status

Accepted

## Context

quicksov 的 battery 产品面当前只需要四类上层语义：

- 聚合后的系统电量百分比
- 单个电池的 health / energy 信息
- 多电池设备的统一展示入口
- `power-saver / balanced / performance` 三档中的子集

自实现 `sysfs + qsosysd` 路线带来了额外的硬件兼容、特权维护和部署负担，但没有为当前 UI 提供明显超出桌面标准栈的价值。UPower 已经提供：

- `DisplayDevice` 作为桌面聚合电池视图
- 单个 battery device 的 Energy / Capacity / State 等属性

同时，power-profiles-daemon 提供：

- `ActiveProfile`
- `Profiles`
- `PerformanceDegraded`

并把写入权限交给宿主的 polkit 流程处理。

## Decision

1. `battery` service 改为使用 `org.freedesktop.UPower`
   - 顶层 `level / state / time_to_*` 读取 `DisplayDevice`
   - `batteries[]` 读取真实 battery devices
   - 顶层 `energy_* / health_percent` 由真实 battery devices 聚合
2. `power_profile` 改为使用 `org.freedesktop.UPower.PowerProfiles`
   - 当前档位读取 `ActiveProfile`
   - 可用档位读取 `Profiles`
   - degraded 原因读取 `PerformanceDegraded`
3. `qsovd` 保持普通用户进程
   - 不再自带 root helper
   - power-profile 写入直接调用 power-profiles-daemon，由宿主 session polkit agent 决定授权
4. 不保留双后端
   - 删除 `qsosysd`
   - 删除 `platform_profile` / abstract UDS / `SO_PEERCRED` 鉴权链路

## Consequences

### Positive

- 多电池聚合与电量时间语义直接对齐 UPower 标准桌面模型
- power-profile 控制直接复用宿主 PPD/polkit，而不是自维护提权面
- battery service 仍然保留 quicksov 自己的 public schema 与 UI 归约层

### Negative

- 运行时依赖变为 `UPower + power-profiles-daemon + host polkit agent`
- `power_profile` 的可用档位不再强制三档，前端必须接受二档设备
- 不再尝试提供 `platform_profile`、EPP、cpufreq 等更底层的专有调节面
