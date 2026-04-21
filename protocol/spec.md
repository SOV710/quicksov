#+title: quicksov IPC Protocol Specification
#+version: qsov/1
#+status: draft

# quicksov IPC Protocol Specification

本文档是 qsovd 与 qs 之间 IPC 通信的**唯一事实来源**。Rust daemon 侧和 qs JS 侧各自手写实现，必须严格符合本文档定义。任何协议变更必须在一个 commit 中同步更新本文档、`schema.json` 以及两侧实现代码。

## 1. Transport

- **Socket**：Unix Domain Socket, `SOCK_STREAM`
- **Path**：`$XDG_RUNTIME_DIR/quicksov/daemon.sock`
- **Encoding**：UTF-8 JSON
- **Framing**：NDJSON — 每条消息一行 JSON，以 `\n` 结束

```
┌──────────────────────────────────┬─────┐
│  JSON payload (UTF-8)            │ \n  │
└──────────────────────────────────┴─────┘
```

- **最大消息长度**：16 MiB（单行 UTF-8 字节数；超过此限视为 `E_PROTO_MALFORMED`，立即断连）
- **Proto version**：`qsov/1`
- **整数范围**：所有出现在线协议中的整数必须落在 JavaScript safe integer 范围内（`-2^53+1` 至 `2^53-1`）

## 2. Envelope

所有业务消息（握手后）共享顶层 envelope：

```json
{
  "$schema": "envelope",
  "type": "object",
  "required": ["id", "kind", "topic"],
  "properties": {
    "id":      { "type": "integer", "minimum": 0, "description": "u64; REQ/REP 配对; PUB/ONESHOT 固定为 0" },
    "kind":    { "type": "integer", "enum": [0,1,2,3,4,5,6], "description": "0=REQ 1=REP 2=ERR 3=PUB 4=ONESHOT 5=SUB 6=UNSUB" },
    "topic":   { "type": "string" },
    "action":  { "type": "string", "description": "REQ/ONESHOT 必填; REP/ERR/PUB 为空串" },
    "payload": { "description": "任意 JSON 值, schema 由 (topic, action) 决定" }
  }
}
```

## 3. Handshake

连接建立后双方在 2 秒内完成握手，任一方超时则断连并记 `E_HANDSHAKE_TIMEOUT`。

### 3.1 Client → Server: `Hello`

```json
{
  "type": "object",
  "required": ["proto_version", "client_name", "client_version"],
  "properties": {
    "proto_version":  { "type": "string", "example": "qsov/1" },
    "client_name":    { "type": "string", "example": "qs" },
    "client_version": { "type": "string", "example": "0.1.0" }
  }
}
```

### 3.2 Server → Client: `HelloAck`

```json
{
  "type": "object",
  "required": ["server_version", "capabilities", "session_id"],
  "properties": {
    "server_version": { "type": "string" },
    "capabilities":   { "type": "array", "items": { "type": "string" }, "description": "当前启用的 topic 列表" },
    "session_id":     { "type": "integer", "description": "u64, 日志关联用" }
  }
}
```

Major version 不匹配（例如 server 是 `qsov/2`）→ server 回 `E_PROTO_VERSION` 后断连。

## 4. Standard Error Codes

`ERR` 消息的 payload 结构：

```json
{
  "type": "object",
  "required": ["code", "message"],
  "properties": {
    "code":    { "type": "string", "pattern": "^E_[A-Z_]+$" },
    "message": { "type": "string" },
    "details": { "description": "结构由 code 决定" }
  }
}
```

| Code | 含义 |
|---|---|
| `E_PROTO_VERSION` | 协议主版本不匹配 |
| `E_PROTO_MALFORMED` | 消息格式错误（JSON 解析失败、行超长、envelope 缺字段） |
| `E_HANDSHAKE_TIMEOUT` | 握手超时 |
| `E_TOPIC_UNKNOWN` | topic 不存在或未启用 |
| `E_ACTION_UNKNOWN` | topic 不支持该 action |
| `E_ACTION_PAYLOAD` | action payload schema 不符 |
| `E_SERVICE_INTERNAL` | service 内部错误 |
| `E_SERVICE_UNAVAILABLE` | service 暂不可用 |
| `E_PERMISSION` | 权限不足 |
| `E_RATE_LIMITED` | 请求频率过高（保留） |
| `E_CANCELED` | 请求被取消 |

## 5. Topics

每个 topic 定义：**State Snapshot**（SUB 时和状态变化时推送的完整快照）、**Actions**（REQ/ONESHOT 可执行的动作清单）、可选的 **Events**（不可合并的离散事件，仅 `notification` 使用）。

本骨架只给出 state snapshot 的核心字段，action 列表完整但 payload schema 部分标记为 `TBD`，在首版实现前逐步填充。

---

### 5.1 `battery`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["availability", "present", "on_battery", "level", "state", "power_profile_available"],
  "properties": {
    "availability":      { "type": "string", "enum": ["ready", "no_battery", "backend_unavailable"] },
    "present":           { "type": "boolean" },
    "on_battery":        { "type": "boolean", "description": "UPower OnBattery" },
    "level":             { "type": "integer", "minimum": 0, "maximum": 100, "description": "百分比" },
    "state":             { "type": "string", "enum": ["charging","discharging","empty","fully_charged","pending_charge","pending_discharge","unknown"] },
    "time_to_empty_sec": { "type": ["integer","null"] },
    "time_to_full_sec":  { "type": ["integer","null"] },
    "power_profile":     { "type": "string", "enum": ["performance","balanced","power-saver","unknown"] },
    "power_profile_available": { "type": "boolean" },
    "health_percent":    { "type": ["number","null"], "minimum": 0, "maximum": 100, "description": "Battery health derived from EnergyFull / EnergyFullDesign when available" },
    "energy_rate_w":     { "type": ["number","null"], "description": "Positive charge/discharge rate in watts" },
    "energy_now_wh":     { "type": ["number","null"], "description": "Current stored energy in Wh" },
    "energy_full_wh":    { "type": ["number","null"], "description": "Current full capacity in Wh" },
    "energy_design_wh":  { "type": ["number","null"], "description": "Design capacity in Wh" }
  }
}
```

**Actions**:
- `set_power_profile` — payload `{ profile: "performance"|"balanced"|"power-saver" }`

---

### 5.2 `net.link`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["interfaces"],
  "properties": {
    "interfaces": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name","kind","operstate","carrier"],
        "properties": {
          "name":      { "type": "string", "example": "wlo1" },
          "kind":      { "type": "string", "enum": ["wifi","ethernet","loopback","other"] },
          "operstate": { "type": "string", "enum": ["up","down","unknown","dormant","lowerlayerdown","notpresent","testing"] },
          "carrier":   { "type": "boolean" },
          "mac":       { "type": "string" },
          "mtu":       { "type": "integer" },
          "ipv4":      { "type": "array", "items": { "type": "string" } },
          "ipv6":      { "type": "array", "items": { "type": "string" } },
          "gateway":   { "type": ["string","null"] },
          "rx_bytes":  { "type": "integer" },
          "tx_bytes":  { "type": "integer" }
        }
      }
    }
  }
}
```

**Actions**: 无。link 层纯只读，由 netlink 事件驱动快照更新。

**Events 源**: rtnetlink `RTMGRP_LINK` / `RTMGRP_IPV4_IFADDR` / `RTMGRP_IPV6_IFADDR` / `RTMGRP_IPV4_ROUTE`。

---

### 5.3 `net.wifi`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["interface", "state"],
  "properties": {
    "interface": { "type": "string", "example": "wlo1" },
    "state":     { "type": "string", "enum": ["disconnected","scanning","associating","connected","unknown"] },
    "present":   { "type": "boolean", "description": "Whether the target Wi-Fi interface exists in sysfs" },
    "enabled":   { "type": "boolean", "description": "Whether Wi-Fi operations are currently enabled and usable" },
    "availability": {
      "type": "string",
      "enum": ["ready", "disabled", "unavailable"],
      "description": "High-level backend state, distinct from association state"
    },
    "availability_reason": {
      "type": "string",
      "enum": [
        "none",
        "no_adapter",
        "rfkill_soft_blocked",
        "rfkill_hard_blocked",
        "wpa_socket_missing",
        "permission_denied",
        "backend_error",
        "unknown"
      ]
    },
    "interface_operstate": { "type": ["string","null"], "description": "Raw /sys/class/net/<iface>/operstate value" },
    "rfkill_available": { "type": "boolean", "description": "Whether the rfkill command is available for set_enabled / airplane actions" },
    "rfkill_soft_blocked": { "type": "boolean" },
    "rfkill_hard_blocked": { "type": "boolean" },
    "airplane_mode": { "type": "boolean", "description": "Best-effort derived state: all wireless rfkill entries blocked" },
    "ssid":      { "type": ["string","null"] },
    "bssid":     { "type": ["string","null"] },
    "rssi_dbm":  { "type": ["integer","null"] },
    "signal_pct":{ "type": ["integer","null"], "minimum": 0, "maximum": 100 },
    "frequency": { "type": ["integer","null"], "description": "MHz" },
    "saved_networks": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "ssid":     { "type": "string" },
          "priority": { "type": "integer" },
          "auto":     { "type": "boolean" }
        }
      }
    },
    "scan_results": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "ssid":      { "type": "string" },
          "bssid":     { "type": "string" },
          "rssi_dbm":  { "type": "integer" },
          "signal_pct":{ "type": "integer" },
          "flags":     { "type": "array", "items": { "type": "string" } },
          "frequency": { "type": "integer" }
        }
      }
    }
  }
}
```

**Actions**:
- `scan` — 触发扫描，payload `{}`
- `connect` — payload `{ ssid: string, psk?: string, save?: boolean }`
- `disconnect` — payload `{}`
- `forget` — payload `{ ssid: string }`
- `set_enabled` — payload `{ enabled: boolean }`，通过 `rfkill block/unblock wifi` 控制 Wi-Fi soft block
- `set_airplane_mode` — payload `{ enabled: boolean }`，通过 `rfkill block/unblock all` 控制全局飞行模式 soft block

**后端**: `wpa_supplicant` ctrl socket `/run/wpa_supplicant/wlo1`，通过 `wpa_cli` 协议（`SCAN`, `SCAN_RESULTS`, `ADD_NETWORK`, `SET_NETWORK`, `ENABLE_NETWORK`, `SELECT_NETWORK` 等）。

---

### 5.4 `bluetooth`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["available","powered","discovering","devices"],
  "properties": {
    "available":   { "type": "boolean", "description": "whether a BlueZ adapter exists" },
    "powered":     { "type": "boolean" },
    "discovering": { "type": "boolean" },
    "devices": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "address":   { "type": "string" },
          "name":      { "type": "string" },
          "icon":      { "type": "string", "description": "BlueZ class icon" },
          "paired":    { "type": "boolean" },
          "connected": { "type": "boolean" },
          "trusted":   { "type": "boolean" },
          "battery":   { "type": ["integer","null"], "minimum": 0, "maximum": 100 }
        }
      }
    }
  }
}
```

**Actions**:
- `power` — payload `{ on: boolean }`
- `scan_start` / `scan_stop` — payload `{}`
- `connect` / `disconnect` / `pair` / `forget` — payload `{ address: string }`

**后端**: BlueZ D-Bus `org.bluez`。

---

### 5.5 `audio`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["default_sink","default_source","sinks","sources","streams"],
  "properties": {
    "default_sink":   { "type": "string" },
    "default_source": { "type": "string" },
    "sinks": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id":         { "type": "integer" },
          "name":       { "type": "string" },
          "description":{ "type": "string" },
          "volume_pct": { "type": "integer", "minimum": 0, "maximum": 150 },
          "muted":      { "type": "boolean" }
        }
      }
    },
    "sources": { "description": "同 sinks 结构" },
    "streams": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id":         { "type": "integer" },
          "app_name":   { "type": "string" },
          "binary":     { "type": "string" },
          "title":      { "type": "string" },
          "volume_pct": { "type": "integer", "minimum": 0, "maximum": 150 },
          "muted":      { "type": "boolean" }
        }
      }
    }
  }
}
```

**Actions**:
- `set_volume` — payload `{ sink_id: int, volume_pct: int }`
- `set_mute` — payload `{ sink_id: int, muted: bool }`
- `set_default_sink` — payload `{ sink_id: int }`
- `set_stream_volume` — payload `{ stream_id: int, volume_pct: int }`

**后端**: PipeWire。

---

### 5.6 `mpris`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["active_player","players"],
  "properties": {
    "active_player": { "type": ["string","null"], "description": "当前 active bus name" },
    "players": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "bus_name":        { "type": "string" },
          "identity":        { "type": "string" },
          "playback_status": { "type": "string", "enum": ["Playing","Paused","Stopped"] },
          "title":           { "type": "string" },
          "artist":          { "type": "array", "items": { "type": "string" } },
          "album":           { "type": "string" },
          "art_url":         { "type": "string" },
          "length_us":       { "type": "integer" },
          "position_us":     { "type": "integer" }
        }
      }
    }
  }
}
```

**Actions**:
- `play_pause` / `next` / `prev` / `stop` — payload `{ bus_name?: string }`（省略时作用于 active）
- `seek` — payload `{ bus_name?: string, offset_us: integer }`
- `set_position` — payload `{ bus_name?: string, position_us: integer }`
- `select_active` — payload `{ bus_name: string }`

---

### 5.7 `notification`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["unread_count","history"],
  "properties": {
    "unread_count": { "type": "integer" },
    "history": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id":        { "type": "integer" },
          "app_name":  { "type": "string" },
          "summary":   { "type": "string" },
          "body":      { "type": "string" },
          "icon":      { "type": "string" },
          "urgency":   { "type": "string", "enum": ["low","normal","critical"] },
          "timestamp": { "type": "integer", "description": "unix ms" },
          "actions":   { "type": "array", "items": { "type": "object", "properties": { "id": { "type":"string" }, "label": { "type":"string" } } } }
        }
      }
    }
  }
}
```

**Events** (广播通道, 非状态合并):
- `new` — 新通知到达，payload 为单条 notification 对象
- `closed` — 通知被关闭，payload `{ id: int, reason: "expired"|"dismissed"|"closed"|"undefined" }`

**Actions**:
- `invoke_action` — payload `{ id: int, action_id: string }`
- `dismiss` — payload `{ id: int }`
- `dismiss_all` — payload `{}`
- `mark_read` — payload `{ id?: int }`（省略即全部）

**后端**: daemon 实现 `org.freedesktop.Notifications` D-Bus server，取代 mako/dunst。

---

### 5.8 `niri`

**State Snapshot**:

```json
{
  "type": "object",
  "properties": {
    "workspaces": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "idx":     { "type": "integer" },
          "name":    { "type": ["string","null"] },
          "output":  { "type": "string" },
          "focused": { "type": "boolean" },
          "windows": { "type": "integer" }
        }
      }
    },
    "focused_window": {
      "type": ["object","null"],
      "properties": {
        "id":           { "type": "integer" },
        "display_name": { "type": "string" },
        "app_id":       { "type": "string" },
        "title":        { "type": "string" }
      }
    }
  }
}
```

**Actions**:
- `focus_workspace` — payload `{ idx: int }`
- `run_action` — payload `{ action: string, args?: any }` (透传 niri msg action)

**后端**: `niri msg --json event-stream`，环境变量 `NIRI_SOCKET`。

---

### 5.9 `weather`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["provider","status","ttl_sec","location","current","hourly","last_success_at","error"],
  "properties": {
    "provider": { "type": "string", "example": "open-meteo" },
    "status": {
      "type": "string",
      "enum": ["loading","ready","refreshing","init_failed","refresh_failed"]
    },
    "ttl_sec": { "type": "integer", "minimum": 1, "description": "成功快照 TTL，前端据此决定何时丢弃旧成功数据" },
    "location": {
      "type": ["object","null"],
      "properties": {
        "name":      { "type": "string" },
        "latitude":  { "type": "number" },
        "longitude": { "type": "number" }
      },
      "required": ["name","latitude","longitude"]
    },
    "current": {
      "type": ["object","null"],
      "properties": {
        "time":           { "type": "string", "description": "ISO 8601; provider-local current time" },
        "temperature_c":  { "type": "number" },
        "apparent_c":     { "type": "number" },
        "humidity_pct":   { "type": "integer" },
        "wind_kmh":       { "type": "number" },
        "wmo_code":       { "type": "integer" },
        "icon":           { "type": "string", "description": "Lucide icon name, mapped from WMO" },
        "description":    { "type": "string" },
        "timezone_abbreviation": { "type": "string", "description": "provider-local timezone abbreviation, e.g. GMT+9" }
      },
      "required": ["temperature_c","apparent_c","humidity_pct","wind_kmh","wmo_code","icon","description"]
    },
    "hourly": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["time","temperature_c","wmo_code"],
        "properties": {
          "time":          { "type": "string", "description": "ISO 8601" },
          "temperature_c": { "type": "number" },
          "wmo_code":      { "type": "integer" }
        }
      }
    },
    "last_success_at": { "type": ["integer","null"], "description": "unix sec；最近一次成功抓取时间" },
    "error": {
      "type": ["object","null"],
      "required": ["kind","message","at"],
      "properties": {
        "kind":    { "type": "string", "enum": ["config","timeout","http","decode","internal"] },
        "message": { "type": "string" },
        "at":      { "type": "integer", "description": "unix sec" }
      }
    }
  }
}
```

**Actions**:
- `refresh` — 立即重新拉取，payload `{}`

**后端**: scheduler task + fetch worker；当前 provider 为 Open-Meteo HTTPS，默认轮询 600s，成功快照 TTL 固定为 1800s。失败刷新不会抹掉上一份成功数据，而是通过 `status` / `error` 让前端自行决定何时将旧数据视为过期。

---

### 5.10 `wallpaper`

**State Snapshot**:

```json
{
  "type": "object",
  "required": [
    "directory",
    "availability",
    "availability_reason",
    "entries",
    "fallback_source",
    "sources",
    "views",
    "transition",
    "renderer"
  ],
  "properties": {
    "directory": {
      "type": "string",
      "description": "Resolved absolute wallpaper directory path"
    },
    "availability": {
      "type": "string",
      "enum": ["ready","empty","unavailable"]
    },
    "availability_reason": {
      "type": "string",
      "enum": ["none","directory_missing","permission_denied","scan_failed"]
    },
    "entries": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["path","name","kind"],
        "properties": {
          "path": { "type": "string" },
          "name": { "type": "string" },
          "kind": { "type": "string", "enum": ["image","video"] }
        }
      }
    },
    "fallback_source": {
      "type": ["string","null"],
      "description": "Default source id used when an output has no explicit view"
    },
    "sources": {
      "type": "object",
      "description": "Resolved wallpaper source sessions keyed by source id",
      "additionalProperties": {
        "type": "object",
        "required": ["id","path","name","kind","loop","mute"],
        "properties": {
          "id": { "type": "string" },
          "path": { "type": "string" },
          "name": { "type": "string" },
          "kind": { "type": "string", "enum": ["image","video"] },
          "loop": { "type": "boolean" },
          "mute": { "type": "boolean" }
        }
      }
    },
    "views": {
      "type": "object",
      "description": "Per-output wallpaper view mapping",
      "additionalProperties": {
        "type": "object",
        "required": ["output","source","fit","crop"],
        "properties": {
          "output": { "type": "string" },
          "source": { "type": "string" },
          "fit": { "type": "string", "enum": ["cover"] },
          "crop": {
            "type": ["object","null"],
            "required": ["x","y","width","height"],
            "properties": {
              "x": { "type": "number" },
              "y": { "type": "number" },
              "width": { "type": "number" },
              "height": { "type": "number" }
            }
          }
        }
      }
    },
    "transition": {
      "type": "object",
      "required": ["type","duration_ms"],
      "properties": {
        "type": { "type": "string", "enum": ["fade"] },
        "duration_ms": { "type": "integer", "minimum": 0 }
      }
    },
    "renderer": {
      "type": "object",
      "required": [
        "process",
        "backend",
        "status",
        "pid",
        "last_error",
        "decode_backend_order",
        "decode_device_policy",
        "render_device_policy",
        "allow_cross_gpu",
        "present_backend",
        "present_mode",
        "vsync",
        "video_audio"
      ],
      "properties": {
        "process": { "type": "string" },
        "backend": { "type": "string" },
        "status": { "type": "string", "enum": ["starting","running","error"] },
        "pid": { "type": ["integer","null"] },
        "last_error": { "type": ["string","null"] },
        "decode_backend_order": {
          "type": "array",
          "items": { "type": "string" }
        },
        "decode_device_policy": {
          "type": "string",
          "enum": [
            "auto",
            "same-as-compositor",
            "same-as-render",
            "prefer-discrete",
            "prefer-integrated",
            "nvidia",
            "amdgpu",
            "intel"
          ]
        },
        "render_device_policy": {
          "type": "string",
          "enum": [
            "auto",
            "same-as-compositor",
            "same-as-render",
            "prefer-discrete",
            "prefer-integrated",
            "nvidia",
            "amdgpu",
            "intel"
          ]
        },
        "allow_cross_gpu": { "type": "boolean" },
        "present_backend": {
          "type": "string",
          "enum": ["auto","shm","dmabuf"]
        },
        "present_mode": { "type": ["string","null"] },
        "vsync": { "type": "boolean" },
        "video_audio": { "type": "boolean" }
      }
    }
  }
}
```

**Actions**:
- `refresh` — 重新扫描 wallpaper directory，payload `{}`
- `set_output_source` — 将某个 output 绑定到已有 source，payload `{ output: string, source: string }`
- `set_output_path` — 将某个 output 绑定到指定 wallpaper entry，payload `{ output: string, path: string }`
- `next_output` — 某个 output 切换到下一张 wallpaper entry，payload `{ output: string }`
- `prev_output` — 某个 output 切换到上一张 wallpaper entry，payload `{ output: string }`
- `set_output_crop` — 更新某个 output 的 normalized crop，payload `{ output: string, crop: { x, y, width, height } | null }`

**v2 约束**:
- daemon 会索引静态图片与视频文件，两者都会出现在 `entries`
- 渲染模型从单一 `current` 切换为 `source + view`
- 一个 source 可被多个 output view 复用，用于同视频多屏不同裁切
- 不同 output 也可绑定不同 source，用于多视频并行
- `renderer.process` / `renderer.status` 反映专用 wallpaper renderer 进程的运行态
- `renderer.render_device_policy` / `renderer.decode_device_policy` / `renderer.allow_cross_gpu` 暴露 GPU 选择策略；默认安全值分别是 `same-as-compositor` / `same-as-render` / `false`
- 当前 renderer 会把 `render_device_policy` 用于 GBM/libplacebo 渲染设备选择，把 `decode_device_policy` 用于 FFmpeg hwdec 设备偏好与 backend 排序；当 render GPU 与 compositor 主 GPU 不同，present 路径会默认改为“render on render GPU, allocate/present on compositor GPU”
- `renderer.present_backend` 是用户偏好；当前 renderer 会优先尝试 `dmabuf`，若 feedback / GBM / import 任一步失败则在运行时自动 fallback 到 `shm`
- 当前实现由 `qsovd` 直接监督 `qsov-wallpaper-renderer` 承载渲染热路径；state/action 面保持不变

**后端**: daemon 本地目录扫描 + `qsov-wallpaper-renderer` 直接监督/拉起。默认目录 `$HOME/.config/quicksov/wallpapers`，可通过 `daemon.toml.[services.wallpaper].directory` 覆盖。渲染器偏好与默认 source/view 绑定来自 `daemon.toml.[services.wallpaper]`。

---

### 5.11 `theme`

**State Snapshot**: 整个 `design-tokens.toml` 解析后的 JSON 对象。Schema 由 `config/design-tokens.toml` 的结构定义，此处不复述。

**Actions**: 无。纯只读，配置文件变化触发 PUB 推送。

---

### 5.12 `meta`

**State Snapshot**:

```json
{
  "type": "object",
  "properties": {
    "server_version": { "type": "string" },
    "uptime_sec":     { "type": "integer" },
    "services": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "status":    { "type": "string", "enum": ["healthy","degraded","unavailable"] },
          "last_error":{ "type": ["string","null"] }
        }
      }
    },
    "config_needs_restart": {
      "type": "boolean",
      "description": "用户修改了需要重启 daemon 的配置项"
    },
    "screens": {
      "type": "object",
      "properties": {
        "roles": {
          "type": "object",
          "description": "Maps DRM connector names to logical roles (main, aux)",
          "additionalProperties": { "type": "string", "enum": ["main", "aux"] }
        }
      }
    },
    "power": {
      "type": "object",
      "properties": {
        "actions": {
          "type": "object",
          "properties": {
            "lock":     { "type": "boolean" },
            "suspend":  { "type": "boolean" },
            "logout":   { "type": "boolean" },
            "reboot":   { "type": "boolean" },
            "shutdown": { "type": "boolean" }
          }
        }
      }
    }
  }
}
```

**Actions**:
- `ping` — payload `{}`，响应 `{ pong: true, server_time: int }`
- `shutdown` — daemon 优雅退出（保留，初版可不实现）

## 6. Versioning

- 本文档标识版本 `qsov/1`
- Breaking change 增 major（`qsov/2`），新增字段/action 不增 major
- 任何 topic 的 state snapshot schema 修改需同步更新 `schema.json` 和两侧实现
