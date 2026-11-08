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
`AnimationFinished`、`ChangeState`、`ChangeForm`、`SetScale`、`SetDisplays`、`Summoned`、
`SetTrail`、`SetSpeed`、`AssetLoaded/Error`、`ExtensionEvent`、`AgentRequest/Reply/Error`。
新增事件只需在 `Event` 枚举加一个变体。

## 3. 角色状态机

`nono_core::PetState` 持有：当前形态、状态、动画规格、播放起点、屏幕锚点（脚底
坐标）、朝向、缩放/DPI。锚点是**逻辑全局坐标**（主屏左上角为原点、单位为 points），
`dpi` 只在渲染/窗口边界用于换算物理像素；这样混合 DPI 的多屏坐标仍然连续。

| 状态 | 说明 | 结束条件 |
|---|---|---|
| `Idle` | 静止，**原地循环播放**当前朝向的走路动画（可关闭） | 用户交互 / 行为调度 |
| `Walk` | 沿直线走到目标点，八方向动画 | 到达目标 |
| `Action` | 播放动作（播一遍） | `auto_end=1` 回 Idle（可带 `return_facing`）；`auto_end=0` 停在末帧，关机则进入循环充电（`after_action`） |
| `Expression` | 播放表情（播一遍） | 播完回 Idle |

动作结束时可以指定回到待机后的朝向（`return_facing`）：召唤/玩魔方回正面，
形态 1 吃电池回左侧、形态 2 吃电池回右侧。数据来自 catalog，详见素材契约。

素材包的真实语义被如实保留：

- 走路循环（形态 1/2/3 分别 13/19/16 帧 @25fps）。
- 动作/表情只播一遍，末帧钳制。
- **充电、关机**（`auto_end=0`）停在末帧并保持占用，直到下一次交互；关机播完后
  若该形态有充电动作，则接着循环播放充电（形态 3/4 没有充电，停在关机末帧）。
- 形态 3/4 没有「充电」；形态 3 的表情是「头晕 / 生气 / 玩乐 / 开心」，形态 4 是「头晕 / 生气 / 玩乐 / 得意」。
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
- **换色（调色盘）**：素材的每个填充（`ty:"fl"`）带自定义字段 `nono.dye`：
  - `solid`：染成所选实色（本体白、耳根白）；
  - `shade`：按原始亮度染成较深色（本体灰色阴影，保留阴影层次）；
  - 没有该字段：永不染色（青色、面罩、眼睛、道具、白色闪光、其它杂色）。
  加载时直接改 Lottie 的矢量填充色（文件不改），因此任意缩放清晰、跟着动画和姿势走。
  静态 `solid` 填充取所选颜色本身；带颜色关键帧的填充逐关键帧按亮度换色，原来的明暗/淡入淡出动画
  得以保留。形态 4/5 的资产尚未打标，等于不染色。未选颜色（默认白）时完全跳过。
- **染色性能与缓存**：染色需要把整份 Lottie JSON 解析成树再写回。ThorVG 的 Lottie 解析器
  在对象 key 被排序后会病态变慢（13 MB 的形态 2 魔方从 ~0.1 s 变成 ~8 s），所以使用
  自定义的 `JsonTree`（`Vec<(key, value)>`）保持原始 key 顺序。生成结果缓存在
  `~/Library/Caches/NonoPet/tinted`（key 含源文件路径/大小/mtime/颜色，换颜色会清空），
  冷加载约 0.38 s、热缓存约 0.1 s。加载耗时会被 `PetState::compensate_pause` 从播放
  时钟里扣除，因此慢加载后仍从第 0 帧开始，不会直接跳到末帧。
- **画布引用释放**：`tvg_canvas_add` 内部会 `ref()` 目标 paint；我们为借用 picture 而临时
  加的那一次引用必须在 add 后用 `tvg_paint_unref` 释放，否则每切换一次动画就泄漏整棵
  Lottie 场景（形态 2 魔方约 17 MB/次）。这也是之前“点得越多内存越大”的根因。
- **首帧加载顺序**：切换动画时先加载/解析 Lottie，再改窗口尺寸和呈现层。否则冷启动
  第一次跑大动作时，窗口会先变大、内容还没准备好，出现短暂空白。
- **辉光（运行时重建）**：Lottie 没有 glow 滤镜。素材里每个需要发光的图层都带
  `nono.glow`（颜色 / blur / strength / `baked` / `pulse`，来自源 SWF），加载时按
  (颜色, 半径, 强度) 分组，把对应图层抽出来生成几份很小的 Lottie（只含发光图层，
  统一涂成辉光色，保留图层的不透明度动画 ⇒ 原素材的脉动免费复现），缓存在
  `~/Library/Caches/NonoPet/glow`。`baked: true` 的图层已经用同心描边烘焙过，跳过；
  纯红色条目是源里未做颜色变换的脏数据，直接忽略。
  渲染时每份 pass 各自画到一块**降分辨率**的画布（所有 pass 共享 20 万像素预算，
  最低 0.25 倍），用 ThorVG 的 CPU 高斯模糊后，只在该 pass 的非空包围盒内以 bilinear
  采样合成到主帧**下面**（等效 Flash 的“辉光在物体后面”）。
  注意坐标换算方向：`scale` 是主图→辉光图的比例，包围盒要乘 `1/scale` 映射回主图。
  辉光会超出角色画布，所以画布四周额外留出 `2.6 × 最大模糊半径` 的透明边距
  （`Renderer::glow_pad()` 以画布单位上报，窗口几何按同样数值放大并平移），否则光晕
  会被窗口边缘切出一条直线。
  生成为**流式**解析：13 MB 的 Lottie 不再整体建成 JSON 树（那会临时吃 260 MB），
  只把保留下来的图层材料化，峰值内存 ~50 MB。
  `NONO_GLOW_OFF=1` 可整体关闭；`sigma/alpha` 是配置项，支持热重载。
- **动画加载（首次播放的停顿从哪来）**：一次 `load()` 有三块开销——着色副本生成
  （有 `tint` 时要把整份 Lottie 解析出来改色再写盘）、辉光 pass 生成、ThorVG 解析。
  前两块都落盘缓存，并且启动 1.5 s 后由后台线程**预热全部动画**（当前形态排在最前；
  换色时清空后重新预热），所以只有"第一次点某个动作"才可能感觉到停顿。
  实测（2x，缓存预热后、带着色）：关机 2 ms / 开机 3 ms / 玩球 9 ms / 充电 13 ms /
  吃电池 24 ms / 召唤 36 ms；玩魔方 113 ms——它的 Lottie 是 13.7 MB 纯矢量、没有可裁的
  冗余预合成，解析时间降不下来。
- **点击区域**：只有 NONO 的圆形身体接受点击/拖动。渲染后按调色色相（未调色时按
  白/灰本体）取脚底锚点附近最大的连通块，换算成身体圆；鼠标离开身体时窗口用
  `set_cursor_hittest(false)` 整体穿透。窗口静止时 100ms 轮询，动画时随帧更新，
  残影舞台同样只认身体圆。
  `Renderer::body_hit()` 返回的 `(dx, dy, radius)` 是**应用逐动画缩放后的画布单位**，
  相对宠物脚底锚点；计算时会先扣掉画布四周的辉光透明边距（物理像素）再折算，
  否则圆会整体向左上偏移（Retina 上尤其明显）。窗口层只需再乘用户缩放即可。
- **逐动画缩放**：`tuning.json` 的 `scale` 是整张画布的缩放（以 `petAnchor` 为锚点），
  窗口尺寸同步按比例变化，放大的动作不会被原始画布裁掉；`offset` 仍按源画布单位、
  随缩放一起换算。
- **行走残影**：时间驱动，而不是按距离或固定档位。
  - 每 `trail_interval_ms` 生成一份快照（控制疏密），快照只保留不透明度>0 的
    角色部分；地面阴影是脚底锚点所在的独立连通块，生成快照时用
    `nono_render::strip_ground_shadow` 整块剔除，所以影子不会拖尾。
  - 每份快照按自己的年龄 `t = age / lifetime` 计算透明度：
    `opacity × fade_in(t) × (1-t)^1.6`，前 10% 淡入、之后逐渐淡出（控制尾巴长度）。
  - 渲染时窗口临时扩大 `pad = min(行走速度 × 停留时间 + 2px, 700px)`，旧帧按当时的
    脚底锚点偏移贴回，最后把当前帧合成在最上层（`nono_render::compose_trail`）。
    700px 上限保证极端速度/停留组合不会分配巨大的舞台。
  - 每个快照同时记录「脚底在该帧图像内的像素位置」`anchor_in_frame`。不同走路方向的
    辉光边距不同（形态 2 有 11/15 两档），画布尺寸也会不同；用快照自己的锚点回贴，
    而不是要求所有帧尺寸一致，否则跟随鼠标转圈、频繁换方向时整条尾巴会被丢掉。
  - 舞台尺寸按「锚点相对范围」缓存：一次行走中只扩不缩，等残影完全消失才缩回。
    这样快照逐个过期时不会每 120 ms 改一次窗口大小/原点（那会表现为闪烁）。
  - 停止走路后，历史帧继续按剩余停留时间淡出，随后释放并把窗口缩回原尺寸；
    淡出期间窗口保持透明边距，透明区域点击穿透。
  - **同步移动（防抖）**：残影的屏幕位置依赖当前锚点与窗口位置。如果窗口按 60 Hz
    移动、残影只在 25 fps 动画帧重排，旧残影会跟着窗口来回“晃”再弹回去，尾巴越长
    越明显。因此走路时窗口位置不再单独移动，而是由 `present_cached` 在每个 tick
    用缓存的 Lottie 帧重新合成残影层并同步移动窗口；Lottie 本身仍只按 25 fps 重绘。
    代价是走路时多一层 ~60 fps 的合成/present CPU。
  - 三个参数都可由用户自定义：间隔 20–400 ms、停留 100–1600 ms、浓度 0.02–0.60。
    默认 120 ms / 640 ms / 0.22。停留时间还会被 `间隔 × 48` 限制，避免快照数量
    和窗口无限增长。托盘提供 ± 微调、“预览一次（走一小段）”、“恢复默认”、
    打开配置文件，以及一个运行时“自动重载配置（调试）”开关
    （每 500 ms 轮询一次文件 mtime）。
  - 舞台长度 = 逻辑速度 × `dpi` × 停留时间（换算成物理像素），因此散步（60 pt/s）
    和跟随鼠标（140 pt/s）的尾巴长度不同，符合运动速度直觉。
- 渲染按动画自身帧率（25fps）节流；只有「正在播动画」或「内容变化」时才重绘。
  Idle 时完全不重绘，CPU 接近 0。

## 5. 窗口几何（靠 petAnchor 对齐）

每份动画都有自己的画布尺寸与 `petAnchor`（宠物脚底在画布内的坐标）。
设锚点（脚底）在**逻辑全局屏幕坐标**中的位置为 `A`、用户缩放 `s`，则窗口的
逻辑矩形为：

```
w = round(canvas_w * s)
h = round(canvas_h * s)
x = A.x - pet_anchor.x * s
y = A.y - pet_anchor.y * s
```

窗口交给 winit 时用 `LogicalPosition` / `LogicalSize`，所以 AppKit 收到的就是这套
逻辑点坐标，切换动画时 **宠物在屏幕上完全重合**，不会跳动。只有渲染器和呈现层
才把逻辑尺寸乘以当前屏幕的 backing scale `d` 得到物理像素：

```
物理窗口宽高 = round(逻辑宽高 * d)
```

这样主屏 2x + 外接屏 1x 时，宠物跨屏不会被重复缩放；跨屏后 `state.dpi` 跟着锚点
所在屏幕更新并重载当前帧。`nono_platform::displays()` 返回每块屏幕的逻辑可用区域
（`NSScreen.visibleFrame`，排除菜单栏 / Dock）、缩放倍率和稳定 id。

窗口还会与**所有已启用屏幕的可用区域并集**求交：大动作窗口如果会溢出屏幕，就只开
可见的那一块，并按裁剪偏移渲染。否则 macOS 会把窗口钳回屏幕内，角色就被整体带偏
（形态 2 玩球最容易触发）。并集而不是单屏，是为了不让正在跨越接缝的宠物被切掉一半。

窗口随动画画布动态缩放：

- 走路窗口约 `99×144 pt`（1.5×，2x 屏），很轻。开着残影时窗口在走路期间临时向
  四周扩出 `pad = min(速度(物理px/s) × 停留时间 + 2px, 700px)`（默认约 `40 pt` 散步 /
  `91 pt` 跟随）作为透明舞台；透明区域点击穿透，但 `cursor_over_pet()` 会按屏幕光标
  位置把宠物本体那一块临时恢复为可点击，所以走路时也能点中/拖动。停下后残影继续
  淡出，窗口保持同样大小直到最后一份快照消失，然后恢复原始尺寸和交互。
- 大动作会把窗口临时放大（例如形态 2 玩球到 `1315×720 pt`），期间窗口设为
  **点击穿透**，不挡桌面；`auto_end=0` 的动作例外，保持可交互。
- 个别素材首尾位置不完全一致（形态 2 开机），`tuning.json` 支持 `offset_start`：
  第 0 帧用起始偏移、末帧用结束偏移，中间线性插值。这样关机→开机、开机→待机的
  衔接点都能对上。
- 拖拽时锚点按「按下时的锚点 + 全局光标位移」计算；拖到屏幕外或未启用的屏幕时，
  `RoamArea::clamp` 会把它投影到最近的可活动屏幕。`RoamArea` 为每块已启用屏幕保存
  一个矩形，以八方向走路 `content_bbox` 的并集把轮廓外扩从**外边缘**扣掉；两块屏幕
  共享的接缝不扣边距，因此宠物可以贴着接缝拖过去（渲染侧仍会裁剪超出并集的窗口边）。
  自动走动只选择 `RoamArea::segment_inside` 能直线走到的目标，避免跨过屏幕间空隙。

坐标约定：**`PetState::anchor` / `Point` / `Rect` 全部是逻辑全局 points**；
物理像素只出现在渲染器输出、残影缓冲和 presenter 层。macOS 的 `NSWindow.frame`
与 `NSScreen.frame` 本身就是 points，`nono-platform` 负责把 AppKit 的
bottom-left 原点翻转成 top-left，并读取每块屏幕的 backing scale。

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
                            # form / scale / sound / volume / wander
                            # / move_speed / follow_speed / idle_speed / trail_* / anchor
                            # / display_ids / start_at_center / summon_animation
                            # / glow / glow_sigma / glow_alpha
~/Library/Caches/NonoPet/tinted/   # generated tinted Lottie cache
~/Library/Caches/NonoPet/glow/     # generated glow passes (one per colour/blur group)
```

配置在启动时读取，菜单操作与退出时写回。`anchor` 是逻辑全局坐标，在拖拽松手、
召唤和退出时保存。`start_at_center = true`（默认）时启动是「主显示器中心 + 召唤动画」，
不使用 `anchor`；设为 `false` 时启动恢复上次位置，如果该点不在当前启用的屏幕上
则回退到中心。`display_ids` 是允许活动的显示器 id 列表；缺省或为空表示「只允许主显示器」。

## 8. 行为调度

`nono_core::Behavior` 在 Idle 时每 6~16 秒掷一次骰子：

- 55% 走一段（随机方向、50~180 逻辑像素，1 秒左右）
- 20% 做一个随机表情
- 其余：换个朝向继续待机

走一段的速度取「待机自由移动速度」：`idle_speed` 有值就用它，否则回退到全局
`move_speed`。可以从菜单关闭「自由走动」。行为只调用状态机的公开 API，不直接碰窗口。

行为现在接收 `RoamArea`（而不是单个屏幕矩形）：目标先按随机半径生成，再投影到
可活动区域。若目标与当前位置的直线采样（`segment_inside`）会穿过屏幕之间的空隙，
就退回当前屏幕内，保证自动走动不会跑到桌面空洞里。

## 8.1 鼠标跟随（20 秒）

托盘的「跟随鼠标（20 秒）」是一段**运行时模式**，不写入配置：

- 每 150 ms 读一次鼠标位置（macOS 用 `NSEvent.mouseLocation`，换算到逻辑全局 points），
  把目标夹到 `RoamArea` 内（鼠标在未启用屏幕时取最近的可活动边缘）。
- 距离超过 60 逻辑像素就以「跟随速度」走过去（`follow_speed`，未设置时回退到
  全局 `move_speed`；默认 140 pt/s）；进入该范围后停下并转向鼠标方向。
- 20 秒后自动结束（或再次点菜单 / 拖拽 / 召唤提前结束），随后把待机决策
  往后推，避免刚跟完就自己乱走。
- 跟随期间跳过常规的行为调度，但渲染、窗口移动、边缘夹紧仍走同一套逻辑；
  跨屏时锚点所在屏幕的 `dpi` 变化会触发一次保留播放进度地重载。

## 8.2 召唤与启动

- `App::summon()` 把锚点设为主显示器可用区域中心（主屏未启用时用第一块可启用屏），
  清空旧残影、显示窗口、必要时切换 `state.dpi`，然后播放当前形态的 `summon` 动作。
  动作来自 catalog，形态 1–4 都有；播放结束按 `auto_end` 回到 Idle。
- `summon_at(anchor, dpi)` 是同一个流程的显式位置版本，供启动时恢复上次位置使用；
  托盘召唤和屏幕消失后的回退仍调用 `summon()`（屏幕中心）。
- 启动位置由 `config.start_at_center` 决定：
  - `true`（默认）：主显示器中心。
  - `false`：读取 `config.anchor`；只有该点落在当前已启用的屏幕上才恢复，否则回退中心。
  两种模式都在该位置播放召唤动画；`config.summon_animation = false` 时只定位并回到
  Idle，不播放动画。
- 托盘「启动时回到屏幕中心」勾选状态与 `start_at_center` 同步；取消勾选时会立即保存
  当前位置，保证下次启动有位置可恢复。
- 托盘「移动范围」勾选变化会立即重算 `RoamArea`；如果宠物当前在刚被取消的屏幕上，
  会调用 `summon()` 把它带回可活动屏幕。热插拔每 5 秒轮询一次（只比较 id 集合），
  变化时重建托盘并重新解析 `display_ids`。

## 8.3 移动速度

`nono_core::speed` 定义范围 10–400 pt/s。**这些速度现在是逻辑 points/s，
`PetState` 的位移不再乘 `dpi`**；只有残影舞台长度会按 `speed × dpi` 换算成物理像素：

```
move_speed   = 60.0    # 全局速度，始终存在
follow_speed = 140.0   # Option：None = 跟随全局
idle_speed   = None    # Option：None = 跟随全局
```

- `Config::effective_follow_speed()` / `effective_idle_speed()` 负责回退。
- `Behavior` 存一份待机速度，配置/热重载改变时会同步；跟随步骤每次读取最新值。
- 托盘「速度」子菜单显示三个当前值，提供 ±10 pt/s、两个“使用全局速度”开关、
  “恢复默认速度”。默认值保留原手感：全局 60、跟随 140、自由移动跟全局。
- 托盘里的“打开配置文件…”/“自动重载配置（调试）”同时覆盖残影、速度、召唤动画字段。

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
- 残影舞台的点击穿透按宠物「紧矩形」动态开关，不是逐像素命中；紧矩形内的透明边缘
  也会接收点击。其他大动作窗口仍按状态整体开关。
- 用户素材导入 / 视频转精灵帧流水线尚未开始（素材目前全是 Lottie）。
