#+title: L3 Wallpaper Engine Runtime Architecture
#+author: SOV710
#+date: 2026-04-21
#+project: quicksov
#+depends-on: L3 Technical Architecture, protocol/spec.md

# L3 Wallpaper Engine Runtime Architecture

本文档记录 quicksov wallpaper engine 的当前真实实现。

- 这是事实文档，不是预案文档。
- 本文描述的是当前主链路；若代码与本文不一致，应在同一轮变更中修正文档或修正实现。
- UI/视觉目标仍由 L1/L2 定义；本文只描述进程、数据、渲染与运行时边界。

## 1. 当前生效的主链路

当前 wallpaper engine 的主链路是：

```text
daemon.toml
  -> qsovd wallpaper service
  -> wallpaper snapshot
  -> qsov-wallpaper-renderer
  -> Wayland background layer surfaces
```

当前主 `shell.qml` 不负责壁纸渲染。主 shell 只实例化：

- `MainBar`
- `AuxBar`
- `PowerDock`

旧的 QML wallpaper 路径已经从仓库中移除。当前默认运行路径只有 daemon -> wallpaper renderer 这一条主链路。

## 2. 进程与职责边界

### 2.1 `qsovd`

`qsovd` 中的 `wallpaper` service 负责：

- 读取并归一化 `daemon.toml.[services.wallpaper]`
- 扫描 wallpaper directory
- 将媒体文件归约为 `entries`
- 将配置归约为 `sources` 与 `views`
- 维护 `fallback_source`
- 暴露 `wallpaper` topic snapshot
- 查找 wallpaper renderer binary
- 监督并直接拉起专用 renderer 进程
- 透传 `QSOV_SOCKET`
- 在 child `exec` 前设置 `PR_SET_PDEATHSIG`

它不负责：

- 实际 Wayland surface 创建
- 视频解码
- 图片/视频最终合成
- 向 compositor 提交 buffer

### 2.2 `qsov-wallpaper-renderer`

这是当前 wallpaper engine 的真实 renderer。

它负责：

- 连接 daemon socket 并订阅 `wallpaper`
- 连接 Wayland
- 跟踪 `wl_output` 生命周期
- 为每个 output 创建 background layer-shell surface
- 按 source 创建共享的媒体 session
- 决定 decode / render / present GPU
- 选择 `dmabuf` 或 `shm` 提交路径
- 执行 per-output crop / cover / fade transition

## 3. daemon 侧状态模型

### 3.1 配置输入

`daemon.toml.[services.wallpaper]` 当前支持的主配置轴：

- `directory`
- `transition`
- `transition_duration_ms`
- `renderer`
- `decode_backend_order`
- `decode_device_policy`
- `render_device_policy`
- `allow_cross_gpu`
- `present_backend`
- `present_mode`
- `vsync`
- `video_audio`
- `sources.<id>`
- `views.<output>`

### 3.2 归约模型

daemon 归约出的核心模型不是“当前一张壁纸”，而是：

- `entries`: 扫描目录后得到的媒体文件列表
- `sources`: 已解析、可引用的媒体源
- `views`: 每个 output 对某个 source 的绑定与 crop
- `fallback_source`: 当 output 没有显式 `view` 时使用的默认 source

该模型支持两类场景：

- 多屏绑定不同 source
- 多屏复用同一 source，但各自使用不同 crop

### 3.3 对外协议

`wallpaper` topic 的状态与 action 定义以 `protocol/spec.md` 为准。

当前主 action：

- `refresh`
- `set_output_source`
- `set_output_path`
- `next_output`
- `prev_output`
- `set_output_crop`

## 4. renderer 订阅与 snapshot 消费

wallpaper renderer 启动后会：

1. 连接 daemon UDS
2. 完成 hello / hello-ack
3. 发送 `SUB wallpaper`
4. 接收完整 `wallpaper` snapshot
5. 后续持续接收 `PUB wallpaper`

renderer 当前真正消费的 snapshot 字段主要是：

- `fallback_source`
- `sources`
- `views`
- `transition.duration_ms`
- `renderer.decode_backend_order`
- `renderer.decode_device_policy`
- `renderer.render_device_policy`
- `renderer.allow_cross_gpu`
- `renderer.present_backend`

## 5. Wayland output 与 surface 模型

renderer 通过 `wl_registry` 跟踪 `wl_output`。

每个 output 对应一个 `OutputSurface`。每个 `OutputSurface` 会创建一个：

- `zwlr_layer_shell_v1.background` layer surface
- 全屏 anchor
- `exclusive_zone = -1`
- `keyboard_interactivity = none`
- 空 input region

因此当前 wallpaper surface：

- 不参与输入
- 不挤占布局空间
- 正确位于普通窗口之下

如果 compositor 提供 `linux-dmabuf feedback`，renderer 还会为每个 output 维护独立的 surface feedback，用于选择更安全的 dmabuf 分配策略。

## 6. source session 模型

renderer 内部按 `source id` 建立 `SourceSession`，不是按 output 建立 decoder。

因此：

- 两个 output 使用同一个 source 时，共享一个解码会话
- 两个 output 使用不同 source 时，各自拥有独立解码会话

`SourceSession` 的两种类型：

- image source: 启动时用 `QImageReader` 读入静态图
- video source: 创建 `VideoDecoder`，进入持续解码

这就是当前“shared-decoder”的真实粒度：按 source 共享，而不是按 output 共享。

## 7. 视频解码链路

视频路径由 `cpp/wallpaper/decoder/ffmpeg/VideoDecoder` 提供。

其职责：

- 维护独立 decoder thread
- 打开输入流并选择视频 stream
- 按 `decode_backend_order` 尝试硬解 backend
- 按 `preferredDevicePath` 绑定目标设备
- 输出两类帧快照：
  - `FrameSnapshot`: CPU/QImage 路径
  - `HardwareFrameSnapshot`: 硬件帧路径

当前实现会优先尝试配置中的硬解后端，失败自动回退到 `software`。

`cuda` 路径当前不是简单使用默认 CUDA device，而是尝试将选中的 DRM render node 映射到精确 CUDA ordinal；若映射失败，则跳过该 `cuda` 候选。

## 8. 渲染与提交链路

每个 `OutputSurface` 都会独立执行 render loop，但它消费的 source 可以是共享的。

当前提交流程：

1. 先尝试准备 `dmabuf` buffer
2. 若可行，再尝试 GPU fast-path
3. 若 GPU fast-path 失败，则回退到 CPU 合成
4. 若 `dmabuf` 路径整体失败，则回退到 `wl_shm`
5. 最终统一执行 `wl_surface_attach + damage + frame callback + commit`

### 8.1 GPU fast-path

GPU fast-path 当前使用：

- GBM
- EGL
- libplacebo OpenGL backend

它会：

- 从 `HardwareFrameSnapshot` 读取硬件帧
- 将源帧导入 libplacebo
- 将目标 dmabuf 导入为 target texture
- 执行 crop + cover 渲染
- 输出到目标 dmabuf

这条路径的目标是避免“硬件帧先回 CPU 再重新上传”的额外代价，但它仍不是完整 Vulkan-native 渲染链路。

### 8.2 CPU 合成路径

当 GPU fast-path 不成立时，当前实现会：

- 对 `dmabuf` 做 `gbm_bo_map`，或直接使用 `wl_shm`
- 构造 `QImage`
- 使用 `QPainter` 做 CPU 合成
- 再提交给 compositor

静态图片总是可以走这条路径。

视频在以下场景也会走这条路径：

- GPU compositor 初始化失败
- libplacebo 无法导入源帧
- transition 激活中
- 需要从上一帧做 CPU fade
- dmabuf GPU fast-path 被禁用或失败

## 9. 切换动画

当前唯一生效的 transition 是 `fade`。

实现方式：

- 切 source 前抓取当前 output 的上一帧图像
- 切 source 后，新旧画面在 CPU 合成路径中按时间做淡入淡出

因此当前 fade transition 的真实语义是：

- 它依赖 output 侧的上一帧快照
- 它不是单独的 compositor-side transition
- 它会让该段时间回落到 CPU 合成思路

## 10. 多 GPU 策略

当前 renderer 将 GPU 决策拆成三条轴：

- decode device
- render device
- present device

### 10.1 render device

由 `render_device_policy` 决定。

默认值是 `same-as-compositor`，因此默认情况下渲染 GPU 与 compositor 主 GPU 保持一致。

### 10.2 decode device

由 `decode_device_policy` 决定。

默认值是 `same-as-render`。

因此默认情况下，解码设备与渲染设备保持一致。

### 10.3 present device

当前 present device 的真实策略是：

- 若 render device 与 compositor device 相同，则直接在同一 GPU 上完成 present
- 若 render device 与 compositor device 不同，则 present 优先回 compositor 主 GPU

这意味着当前跨 GPU 模型不是“直接把任意 render GPU buffer 提给 compositor”，而是：

- decode / render 可以偏向选中的 render GPU
- present 尽量收敛回 compositor 主 GPU

## 11. 当前真实支持的场景

当前已支持：

- 单屏静态壁纸
- 多屏静态壁纸
- 单视频 source 多屏复用
- 单视频 source 多屏不同 crop
- 多 source 并行，不同 output 绑定不同 source
- `dmabuf -> shm` 运行时降级
- output 热插拔后的 surface 生命周期更新

当前不支持“每个 workspace 使用不同动态壁纸”这一模型；当前模型是 output 级 wallpaper，而不是 workspace 级 wallpaper。

## 12. 已删除的旧路径

以下旧路径已从仓库中删除，不再属于当前 codebase：

- `shell/wallpaper-shell.qml`
- `shell/desktop/WallpaperLayer.qml`
- `shell/services/Wallpaper.qml`
- `shell/services/WallpaperSessions.qml`
- `native/wallpaper_mpv/`
- `native/wallpaper_ffmpeg` 中仅用于 QML plugin 的 `WallpaperVideoItem.*`
- 旧的 plugin 构建/安装脚本

当前默认 Niri 会话中的 wallpaper 渲染完全不依赖这些组件。

## 13. 当前已暴露但未完全接通的字段

以下字段目前存在于配置与协议中，但 wallpaper renderer 主链路没有完整消费，或没有形成独立语义：

- `renderer.backend`
  - 当前主实现固定走 wallpaper renderer 主链路；该字段不是运行时 backend factory。
- `transition.type`
  - 当前只有 `fade`；native 实际主要消费 `duration_ms`。
- `renderer.present_mode`
  - 当前未形成独立 present mode 策略实现。
- `renderer.vsync`
  - 当前未形成独立的 vsync 开关实现。
- `renderer.video_audio`
  - 当前 snapshot 会暴露该字段，但 renderer 主链路不按它做全局音频行为切换。

这些字段不应被误解为“已经有完整运行时语义”。

## 14. 文档维护规则

后续只要以下任一事实发生变化，就必须同步更新本文档：

- wallpaper 主渲染进程变化
- source/session 粒度变化
- output/surface 生命周期模型变化
- decode/render/present GPU 策略变化
- present backend 优先级变化
- transition 机制变化
- 是否重新引入非主链路的 wallpaper 前端/renderer

相关协议变化还必须同时更新：

- `protocol/spec.md`
- `protocol/schema.json`
