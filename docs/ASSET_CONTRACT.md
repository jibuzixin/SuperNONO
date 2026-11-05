# 素材契约

运行时**只读** `assets/catalog.json`。各目录下的 `manifest.json` / `meta.json` 是
上游素材包的原生格式，由 `tools/build_catalog.py` 归一化；不要手改 catalog。

```bash
python3 tools/build_catalog.py          # 重新生成
python3 tools/build_catalog.py --check  # 校验 catalog 是否与素材一致（CI 用）
nono --check                            # 从应用侧校验
```

## 1. catalog.json 结构

```json
{
  "format_version": 1,
  "name": "超能 NONO",
  "default_form": 1,
  "forms": [
    {
      "id": 1,
      "name": "形态 1",
      "fps": 25,
      "walk": {
        "pet_anchor": [32.7219, 90.4],
        "idle_direction": "down",
        "idle_frame": 0,
        "directions": {
          "down": {
            "file": "nono-form1-walk/nono1_down.json",
            "canvas": [66, 96],
            "frames": 13,
            "fps": 25,
            "pet_anchor": [32.7219, 90.4],
            "loop": true,
            "audio": null
          }
        }
      },
      "actions": [
        {
          "id": 2,
          "slug": "boot",
          "name": "开机",
          "file": "nono-form1-actions/nono1_action_boot.json",
          "canvas": [132, 243],
          "frames": 170,
          "fps": 25,
          "pet_anchor": [64.1549, 216.45],
          "content_bbox": [-0.01, 0.05, 131.57, 242.81],
          "loop": false,
          "auto_end": true,
          "return_facing": null,
          "audio": {
            "file": "nono-sounds/sounds/action_boot.mp3",
            "loops": 1,
            "loop_until_stop": false
          }
        }
      ],
      "expressions": [
        {
          "id": 1,
          "slug": "happy",
          "name": "开心",
          "file": "nono-form1-expressions/nono1_exp_happy.json",
          "canvas": [188, 163],
          "frames": 94,
          "fps": 25,
          "pet_anchor": [94.9, 158.1907],
          "loop": false,
          "auto_end": true,
          "audio": null
        }
      ]
    }
  ]
}
```

字段说明：

| 字段 | 说明 |
|---|---|
| `canvas` | Lottie 画布尺寸（矢量单位，运行时按 `scale*dpi` 光栅化） |
| `pet_anchor` | 宠物脚底在画布内的坐标，**摆放唯一依据** |
| `frames` / `fps` | 帧数与帧率，来自 Lottie 的 `op - ip` / `fr` |
| `loop` | 走路为 `true`；动作/表情为 `false` |
| `auto_end` | 动作播完后是否回 Idle。充电/关机为 `false`（停在末帧） |
| `return_facing` | 回待机时的朝向（`down/left/right/...`）。召唤、玩魔方为 `down`；吃电池形态 1 左、形态 2 右；形态 2 玩球为 `right`，形态 3 玩球为 `down` |
| `after_action` | `auto_end=0` 的动作播完后进入的循环动作。关机为 `charge`（有充电动作的形态） |
| `audio.loops` | `null` + `loop_until_stop=true` 表示一直循环到下一个动画 |
| `content_bbox` | 内容包围盒（预留给逐像素命中/裁切，M1 未使用） |

## 2. 摆放规则（最重要）

**不要按画布居中。** 各动画画布差异很大（形态 1 从 66×96 到 526×303），
必须把每份动画的 `pet_anchor` 对齐到屏幕上的同一个点。

```
窗口宽度 = canvas_w × 用户缩放 × 屏幕 DPI
窗口高度 = canvas_h × 用户缩放 × 屏幕 DPI
窗口 x   = 屏幕锚点.x − pet_anchor.x × 用户缩放 × 屏幕 DPI
窗口 y   = 屏幕锚点.y − pet_anchor.y × 用户缩放 × 屏幕 DPI
```

实现见 `crates/nono-core/src/geometry.rs::pet_window_rect`，并有单测锁定。

## 3. 播放与音效语义

- **走路 / 静止**：走路动画一直循环。静止时原地播放（托盘「静止时播放走路」可关，
  关掉后待机不再重绘、CPU≈0）。移动方向由目标点差值量化为八方向，起步时锁定。
  起步/停步不会重启循环相位，避免卡顿。
- **动作**：播一遍停在末帧。`auto_end=1` 的动作播完回 Idle 并清理；
  `auto_end=0` 的动作停在末帧保持占用。关机结束时如果该形态有充电动作
  （形态 1/2），会继续进入**循环充电**；没有充电动作（形态 3）则静止在关机末帧。
- **表情**：播一遍回 Idle；跨形态表情名不同（形态 3：头晕/生气/玩乐/开心）。
- **音效**：动作开始时播放；下一个动作/表情开始前停止。走路、表情、充电在
  原资源里没有声音（`audio: null`），不要伪造。

## 4. 逐动画修正 `tuning.json`

上游 SWF 转换出的个别动作与走路不完全一致，`assets/tuning.json` 提供局部补偿，
不改源 Lottie：

```json
{
  "format_version": 1,
  "animations": {
    "form2/action/boot": { "offset": [0, 2.5] },
    "form2/action/cube": { "scale": 0.83, "offset": [0, -2.4] }
  }
}
```

- `scale`：整张画布以 `petAnchor` 为锚点缩放（1.0 为原始；窗口随之变化，避免放大后被裁）。
- `offset`：末帧的平移（画布单位，正 y 向下）；没有 `offset_start` 时全程恒定。
- `offset_start`：可选的起始偏移。给出后从首帧到末帧线性插值，用于首尾都需要对齐的
  动画（形态 2 开机：起始对齐关机、结束对齐待机）。
- `frames`：只播放前 N 帧，用来裁掉源素材里重复或空白的尾巴。
- 键是运行时动画 id：`form2/action/boot`、`form2/walk/down`、`form2/expr/happy`。

当前修正值来自逐帧实测：

| 动画 | 问题 | 修正 |
|---|---|---|
| `form2/action/boot` | 结束位置比待机高 2.5；起始又比关机低 1.33 | `offset [0, 2.5]`, `offset_start [0, 1.17]` |
| `form2/action/cube` | 宠物大约 1.21 倍（面罩 30.7 vs 25.3） | `scale 0.83` + `offset [0,-2.4]` |
| `form1/action/summon` | 结束比待机低 3.6 | `offset [0,-3.6]` |
| `form2/action/summon` | 结束比待机低 6.2 | `offset [0,-6.2]` |
| `form3/action/summon` | 结束低 1.3，且第 70 帧后整段空白 | `offset [0,-1.3]` + `frames 70` |
| `form1/action/shutdown` | 整条动画是两遍；第 94 帧第一遍结束（脸部全黑），之后重复 | `frames 95` |
| `form1/action/charge` | 关机末帧 → 充电首帧时向右上偏约 1 单位 | `offset [-0.3, 0.9]` |
| `form2/action/charge` | 同上，偏高 0.67 | `offset [0, 0.67]` |

## 5. 渲染细节（ThorVG）

- 关键帧是 **hold（`h:1`）**，必须按整数帧推进；ThorVG 的 Lottie loader 支持。
- 帧号允许小数，但 `set_frame` 与当前帧相差 < 0.0009 时会返回
  `InsufficientCondition`，调用方应视为「无需更新」。
- 画布是预乘 `ABGR8888`，小端内存里是 RGBA。

## 5.1 换色（运行时，不属于素材）

外壳换色在加载时按图层完成：只染“纯白 + 中性/米色阴影、无饱和色”的本体层，以及靠近身体中心的大块纯白层（如形态 3 头盔）；灰色阴影会换成较深的同色。只含纯白的小图层（白色闪光、汗滴、眼睛高光、气泡、道具）不动。不改 Lottie 文件。配置项 `tint = [r, g, b]`，`None`/缺省即原始白色。详见架构文档「换色」一节。

## 5.2 辉光（标签在素材里，效果在运行时）

需要发光的图层带 `nono.glow`：

```json
"nono": { "role": "accent", "shape": "13",
          "glow": {"color": [50,254,254], "alpha": 255, "blur": 5.0, "blurY": 5.0,
                   "strength": 1.0, "inner": false, "compositeSource": true,
                   "passes": 1, "baked": false, "pulse": true, "src": true} }
```

- `baked: true` = 资产里已经用描边烘焙过，程序**不再**叠一层（否则双倍发光）；
- `color: [255,0,0]` 是源里未经颜色变换的原始值（画面里没有红光），运行时按青色
  `[50,254,254]` 画；
- `nono.glow` 是**唯一入口**：有就画，没有就不画；`src` / `manual` 只是来源标记，不做开关；
- 顶层 `nono.glowShapes` 是源参数全表（按 shape id），只用于人工核查，运行时不读；
- 标注合并工具：`python3 tools/merge_glow_labels.py <标注目录>`（只写 `nono.glow`，
  其它字段不动；两边图层数量/名字必须一致）。

生成物缓存在 `~/Library/Caches/NonoPet/glow/<hash>/`（hash = 文件路径+大小+mtime），
换 sigma/alpha 不需要重新生成，换素材自动失效。

配置项（`config.toml`，均支持热重载）：

```toml
glow       = true   # 总开关
glow_sigma = 0.6    # 模糊半径 = 源 blur × 该系数（0.1–2.0）
glow_alpha = 1.3    # 辉光亮度系数（0.0–2.0）
```

`NONO_GLOW_OFF=1` 环境变量可临时整体关闭（截图对照、排查、省电）。

## 6. 新增素材

1. 按现有目录约定放 Lottie 文件和 `manifest.json`（字段参考现有 manifest）。
2. 在 `tools/build_catalog.py` 里加入新目录（如果是新形态）。
3. `python3 tools/build_catalog.py && nono --check`。
4. 跑 `cargo test -p nono-core -p nono-render`（会校验真实素材）。

`format_version` 用于未来演进；新增字段请保持向后兼容（缺省值）。
