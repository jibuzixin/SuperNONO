# 架构设计（M1 实现版）

> 目标：轻量、常驻、可扩展的超能 NONO 桌宠。本文件描述 **已经落地的 M1 架构**，
> 以及 M2 / M3 的扩展点。

## 1. 分层

```
┌──────────────────────────────────────────────────────────────┐
│ nono-app        主程序：winit 事件循环、窗口几何、菜单分发、编排 │
├──────────────────────────────────────────────────────────────┤
│ nono-platform   透明呈现(Presenter)、托盘菜单、NSSound、macOS 适配 │
│ nono-render     ThorVG SwCanvas + Lottie 播放（软件光栅，无 GPU）  │
├──────────────────────────────────────────────────────────────┤
│ nono-core       事件总线、角色状态机、行为调度、素材目录、配置     │
├──────────────────────────────────────────────────────────────┤
│ nono-plugin     扩展契约（manifest/权限/宿主机 trait）— M3 实现   │
│ nono-agent      Agent 协议 + Adapter trait + Mock — M2 实现      │
└──────────────────────────────────────────────────────────────┘
```

依赖方向是单向的：`app → platform/render/plugin/agent → core`。`core` 不依赖任何
窗口、渲染或音频 crate，因此可以单独测试。

## 2. 事件总线

所有跨模块通信都走 `nono_core::Bus`：

- 底层是 `tokio::sync::broadcast`，容量 256；订阅者掉队只会丢旧消息，不会阻塞发布者。
- 事件是带 `"type"` 判别字段的 JSON，例如：

```json
{"type":"PlayAnimation","anim_id":"form1/walk/down","form":1,"loop_anim":true}
{"type":"AgentReply","text":"哈喽！","emotion":"happy","suggest_anim":"talk"}
```

- `Bus::publish` 之后会调用注册的 waker（主程序把它接到 `EventLoopProxy`），
  这样 GUI 线程在 `ControlFlow::Wait` 时也能被后台模块唤醒。
- `--trace` 会把每条事件以 JSON 打到 stderr，便于调试插件和 Agent。

已定义的事件：`UserInput`、`UserClick`、`DragStarted/Ended`、`PlayAnimation`、
`AnimationFinished`、`ChangeState`、`ChangeForm`、`SetScale`、`AssetLoaded/Error`、
`ExtensionEvent`、`AgentRequest/Reply/Error`。新增事件只需在 `Event` 枚举加一个变体。

## 3. 角色状态机

`nono_core::PetState` 持有：当前形态、状态、动画规格、播放起点、屏幕锚点（脚底
坐标）、朝向、缩放/DPI。

| 状态 | 说明 | 结束条件 |
|---|---|---|
| `Idle` | 静止，**原地循环播放**当前朝向的走路动画（可关闭） | 用户交互 / 行为调度 |
| `Walk` | 沿直线走到目标点，八方向动画 | 到达目标 |
| `Action` | 播放动作（播一遍） | `auto_end=1` 回 Idle（可带 `return_facing`）；`auto_end=0` 停在末帧 |
| `Expression` | 播放表情（播一遍） | 播完回 Idle |

动作结束时可以指定回到待机后的朝向（`return_facing`）：召唤/玩魔方回正面，
形态 1 吃电池回左侧、形态 2 吃电池回右侧。数据来自 catalog，详见素材契约。

素材包的真实语义被如实保留：

- 走路循环（形态 1/2/3 分别 13/19/16 帧 @25fps）。
- 动作/表情只播一遍，末帧钳制。
- **充电、关机**（`auto_end=0`）停在末帧并保持占用，直到下一次交互。
- 形态 3 没有「充电」；形态 3 的表情是「头晕 / 生气 / 玩乐 / 开心」。
  菜单里跨形态点动作时会先切形态，再校验该形态是否存在该动作。

时间由调用方传入 `Instant`，所以状态机是确定性的，可以直接单测（见
`crates/nono-core/src/state.rs` 的测试）。

## 4. 渲染

- `nono_render::Renderer` 封装 ThorVG 的 **SwCanvas**（纯 CPU，无 OpenGL/Metal 上下文）。
- 载入 Lottie 后用 `tvg_paint_ref` 把动画**自己的 picture** 共享给画布；这一点很关键：
  `Paint::duplicate()` 是深拷贝，帧推进不会反映到拷贝上。
- 输出 `ABGR8888` 预乘 alpha，在小端机器上内存布局就是 `R,G,B,A`，
  正好匹配 CoreGraphics 的 `kCGImageAlphaPremultipliedLast`。
- 每次播放只保留当前动画；切换动画时释放上一个（34 MB 素材包不会被整体载入内存）。
- 渲染按动画自身帧率（25fps）节流；只有「正在播动画」或「内容变化」时才重绘。
  Idle 时完全不重绘，CPU 接近 0。

## 5. 窗口几何（靠 petAnchor 对齐）

每份动画都有自己的画布尺寸与 `petAnchor`（宠物脚底在画布内的坐标）。
设锚点在屏幕上的位置为 `A`、用户缩放 `s`、屏幕 DPI `d`，则窗口矩形：

```
scale = s * d
w = round(canvas_w * scale)
h = round(canvas_h * scale)
x = A.x - pet_anchor.x * scale
y = A.y - pet_anchor.y * scale
```

这样切换动画时 **宠物在屏幕上完全重合**，不会跳动。

窗口还会与屏幕可用区域（`NSScreen.visibleFrame`，排除菜单栏与 Dock）求交：
大动作窗口如果会溢出屏幕，就只开可见的那一块，并按裁剪偏移渲染。否则 macOS 会把
窗口钳回屏幕内，角色就被整体带偏（形态 2 玩球最容易触发）。

窗口随动画画布动态缩放：

- 走路窗口约 `99×144 pt`（1.5×，2x 屏），很轻。
- 大动作会把窗口临时放大（例如形态 2 玩球到 `1315×720 pt`），期间窗口设为
  **点击穿透**，不挡桌面；`auto_end=0` 的动作例外，保持可交互。
- 拖拽时窗口跟随光标；锚点会被限制在「宠物轮廓安全区」内：以八方向走路
  `content_bbox` 的并集算出轮廓相对锚点的最大外扩，再把这部分从屏幕可用区域里扣掉。
  这样即使把鼠标拖到屏幕外，NONO 的轮廓也不会被裁掉（窗口的多余透明边仍会被裁剪，
  由渲染侧的裁剪偏移补偿）。

坐标统一用 **物理像素**；winit 的位置/尺寸都是物理像素，而 macOS 的
`NSWindow.frame` 是点（points），转换在 `nono-platform` 内完成。

## 6. 平台层

`Presenter` trait 只有 `present(rgba, w, h, dpi)` 和 `resize(w, h, dpi)`：

- **macOS**：给 `NSView` 的 root layer 加一个 `CALayer` 子层，每帧把 RGBA 包成
  `CGImage` 设置到 `contents`，在 `CATransaction` 里关闭隐式动画。
  窗口放大时系统会发一次倍率未变的 `ScaleFactorChanged`，必须忽略，否则动画会
  被重新加载、从头播放。
  （`softbuffer` 的 macOS 后端把 alpha 写死为 `NoneSkipFirst`，不能用于透明窗，
  所以这里自建。）
- **Windows / Linux**：需要各自的透明呈现后端（分层窗口 / ARGB visual），
  M1 暂未实现，`create_presenter` 会返回 `Unsupported`。
- 托盘用 `tray-icon` + `muda`；macOS 上同时把激活策略设为 `Accessory`（不占 Dock），
  窗口 `CanJoinAllSpaces + Stationary`，并去掉阴影。
- 音效在 macOS 用 `NSSound`（不引入音频解码依赖）；MP3 直接读文件。

## 7. 数据与配置

```
assets/                     # 只读素材 + catalog.json
  catalog.json              # tools/build_catalog.py 生成，运行时唯一入口
  icon/nono-tray.png        # 由 `nono --render-icon` 生成
~/Library/Application Support/NonoPet/config.toml
                            # form / scale / sound / volume / wander / anchor
```

配置在启动时读取，菜单操作与退出时写回；锚点持久化，重启后回到原位置。

## 8. 行为调度

`nono_core::Behavior` 在 Idle 时每 6~16 秒掷一次骰子：

- 55% 走一段（随机方向、50~180 逻辑像素，1 秒左右）
- 20% 做一个随机表情
- 其余：换个朝向继续待机

可以从菜单关闭「自由走动」。行为只调用状态机的公开 API，不直接碰窗口。

## 9. 扩展点

### 插件（M3）
`nono-plugin` 已经定义 `plugin.toml`、权限白名单、`ExtensionHost` trait。
M3 用 `wasmi` 实现沙箱宿主：插件只能通过 host function 发消息/读受控资源，
宿主逐个校验权限，带 fuel 限额与热加载。详见 [PLUGIN_SPEC.md](PLUGIN_SPEC.md)。

### Agent（M2）
`nono-agent` 定义 `AgentRequest` / `AgentReply` / `AgentAdapter` 与 Mock 适配器。
主程序永远只和网关对话，网关后面可以是 Ollama、OpenAI 兼容服务或本地 CLI agent。
详见 [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md)。

## 10. 已知限制（M1）

- 仅 macOS 可运行；Windows/Linux 的 `Presenter` 待实现。
- 没有气泡与文字输入（M2）。
- 还没有实现 WASM 宿主，只是契约（M3）。
- 透明区域的点击穿透是按「状态」整体开关的，不是逐像素命中测试。
- 用户素材导入 / 视频转精灵帧流水线尚未开始（素材目前全是 Lottie）。
