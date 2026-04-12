#+title: quicksov IPC Protocol Specification
#+version: qsov/1
#+status: draft

# quicksov IPC Protocol Specification

本文档是 qsovd 与 qs 之间 IPC 通信的**唯一事实来源**。Rust daemon 侧和 qs JS 侧各自手写实现，必须严格符合本文档定义。任何协议变更必须在一个 commit 中同步更新本文档、`schema.json` 以及两侧实现代码。

## 1. Transport

- **Socket**：Unix Domain Socket, `SOCK_STREAM`
- **Path**：`$XDG_RUNTIME_DIR/quicksov/daemon.sock`
- **Framing**：每条消息前 4 字节小端 `u32` 为 payload 字节长度，随后是 MessagePack 编码的 payload

```
┌─────────────────┬──────────────────────────┐
│  u32 LE length  │  MessagePack payload     │
└─────────────────┴──────────────────────────┘
```

- **最大消息长度**：16 MiB (`length > 16 * 1024 * 1024` 视为 `E_PROTO_MALFORMED`，立即断连)
- **Proto version**：`qsov/1`

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
    "payload": { "description": "任意 MessagePack 值, schema 由 (topic, action) 决定" }
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
| `E_PROTO_MALFORMED` | framing / msgpack / envelope 字段错误 |
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
  "required": ["present", "on_battery", "level", "state"],
  "properties": {
    "present":           { "type": "boolean" },
    "on_battery":        { "type": "boolean", "description": "UPower OnBattery" },
    "level":             { "type": "integer", "minimum": 0, "maximum": 100, "description": "百分比" },
    "state":             { "type": "string", "enum": ["charging","discharging","empty","fully_charged","pending_charge","pending_discharge","unknown"] },
    "time_to_empty_sec": { "type": ["integer","null"] },
    "time_to_full_sec":  { "type": ["integer","null"] },
    "power_profile":     { "type": "string", "enum": ["performance","balanced","power-saver","unknown"] }
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

**后端**: `wpa_supplicant` ctrl socket `/var/run/wpa_supplicant/wlo1`，通过 `wpa_cli` 协议（`SCAN`, `SCAN_RESULTS`, `ADD_NETWORK`, `SET_NETWORK`, `ENABLE_NETWORK`, `SELECT_NETWORK` 等）。

---

### 5.4 `bluetooth`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["powered","discovering","devices"],
  "properties": {
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
  "required": ["default_sink","default_source","sinks","sources"],
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
    "sources": { "description": "同 sinks 结构" }
  }
}
```

**Actions**:
- `set_volume` — payload `{ sink_id: int, volume_pct: int }`
- `set_mute` — payload `{ sink_id: int, muted: bool }`
- `set_default_sink` — payload `{ sink_id: int }`

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

### 5.8 `tray`

**State Snapshot**:

```json
{
  "type": "object",
  "required": ["items"],
  "properties": {
    "items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id":       { "type": "string" },
          "title":    { "type": "string" },
          "icon":     { "type": "string" },
          "tooltip":  { "type": "string" },
          "status":   { "type": "string", "enum": ["active","passive","needs_attention"] }
        }
      }
    }
  }
}
```

**Actions**:
- `activate` — payload `{ id: string, x?: int, y?: int }`
- `secondary_activate` — payload `{ id: string }`
- `scroll` — payload `{ id: string, delta: int, orientation: "horizontal"|"vertical" }`
- `open_menu` — payload `{ id: string }` (TBD: 菜单展示可能由 qs 直接通过 Quickshell.Services.SystemTray 处理)

**后端**: StatusNotifierItem host (`org.kde.StatusNotifierWatcher` / `org.freedesktop.StatusNotifierWatcher`)。

---

### 5.9 `niri`

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
        "id":      { "type": "integer" },
        "app_id":  { "type": "string" },
        "title":   { "type": "string" }
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

### 5.10 `weather`

**State Snapshot**:

```json
{
  "type": "object",
  "properties": {
    "location": {
      "type": "object",
      "properties": {
        "name":      { "type": "string" },
        "latitude":  { "type": "number" },
        "longitude": { "type": "number" }
      }
    },
    "current": {
      "type": "object",
      "properties": {
        "temperature_c":  { "type": "number" },
        "apparent_c":     { "type": "number" },
        "humidity_pct":   { "type": "integer" },
        "wind_kmh":       { "type": "number" },
        "wmo_code":       { "type": "integer" },
        "icon":           { "type": "string", "description": "Lucide icon name, mapped from WMO" },
        "description":    { "type": "string" }
      }
    },
    "hourly": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "time":          { "type": "string", "description": "ISO 8601" },
          "temperature_c": { "type": "number" },
          "wmo_code":      { "type": "integer" }
        }
      }
    },
    "updated_at": { "type": "integer", "description": "unix ms" },
    "offline":    { "type": "boolean" }
  }
}
```

**Actions**:
- `refresh` — 立即重新拉取，payload `{}`

**后端**: Open-Meteo HTTPS, 默认轮询 600s。

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
