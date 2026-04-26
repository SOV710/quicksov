# ADR-016: Battery Telemetry via sysfs, Privileged Writes via `qsosysd`

## Status

Accepted

## Context

原 battery 方案依赖两条 D-Bus 链路：

- `org.freedesktop.UPower` 负责电池 telemetry
- `net.hadess.PowerProfiles` 负责 power-profile 读写

这会把本来已经存在于 kernel `power_supply` sysfs 里的事实，再包一层用户态依赖。同时，power-profile 写入只需要一个极小的特权面，不值得把整个 `qsovd` 提权成 root daemon。

本项目当前只需要三类产品语义：

- 电池 telemetry 只关心系统电池
- power mode 只认 `power-saver / balanced / performance`
- 特权写入范围只需要 `/sys/firmware/acpi/platform_profile`

## Decision

1. `battery` service 改为直接读取 `/sys/class/power_supply/*`
2. 刷新 hint 来自 raw `NETLINK_KOBJECT_UEVENT`、polling 与 `PrepareForSleep(false)`
3. `platform_profile` 成为 power-profile 唯一正式 backend
4. 新增 root sidecar `qsosysd`
   - 只监听 Linux abstract UDS `@quicksov.qsosysd`（raw 名 `\0quicksov.qsosysd`）
   - 只开放 `set_platform_profile(profile)` 一个动作
   - 内部固定执行：读 choices → 校验映射 → 写入 → read-back 校验
   - 每个连接在读请求前先取 `SO_PEERCRED`
   - 只信任两类 caller：`uid == 0`，或 `/proc/<pid>/exe` basename 严格等于 `qsovd`
   - `/proc/<pid>/exe` 读取失败、缺失 pid、或 `...(deleted)` caller 一律拒绝
5. `qsovd` 保持普通用户进程
   - 只读状态永远直接读 sysfs
   - 只有 `set_power_profile` action 才调用 helper

## Consequences

### Positive

- battery telemetry 更接近 kernel 真值，移除了对 UPower 的运行时依赖
- power-profile 写权限被收敛到一个白名单 helper，而不是扩大到整个 daemon
- UI 能同时拿到多电池原始列表与聚合值，不需要在 QML 端自行归约

### Negative

- 需要额外部署 root long-running `qsosysd` service（OpenRC 或 systemd）
- `platform_profile` 缺档、helper 不可达、helper 拒绝 caller 或 backend 写入失败时，第三张 Power Mode 卡片会进入灰态
- 本轮不提供 EPP / cpufreq fallback
