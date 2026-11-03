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

- **走路**：循环；移动方向由目标点差值量化为八方向，方向在起步时锁定。
- **动作**：播一遍停在末帧。`auto_end=1` 的动作播完回 Idle 并清理；
  `auto_end=0`（形态 1/2 的充电、关机）停在末帧保持占用，直到用户交互。
- **表情**：播一遍回 Idle；跨形态表情名不同（形态 3：头晕/生气/玩乐/开心）。
- **音效**：动作开始时播放；下一个动作/表情开始前停止。走路、表情、充电在
  原资源里没有声音（`audio: null`），不要伪造。

## 4. 渲染细节（ThorVG）

- 关键帧是 **hold（`h:1`）**，必须按整数帧推进；ThorVG 的 Lottie loader 支持。
- 帧号允许小数，但 `set_frame` 与当前帧相差 < 0.0009 时会返回
  `InsufficientCondition`，调用方应视为「无需更新」。
- 画布是预乘 `ABGR8888`，小端内存里是 RGBA。

## 5. 新增素材

1. 按现有目录约定放 Lottie 文件和 `manifest.json`（字段参考现有 manifest）。
2. 在 `tools/build_catalog.py` 里加入新目录（如果是新形态）。
3. `python3 tools/build_catalog.py && nono --check`。
4. 跑 `cargo test -p nono-core -p nono-render`（会校验真实素材）。

`format_version` 用于未来演进；新增字段请保持向后兼容（缺省值）。
