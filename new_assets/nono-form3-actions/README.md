# 超能 NONO · 形态 3 · 动作动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/action/{2..7}_3.swf` 直接转换而来的
**形态 3（超能 NONO 第三个形态）动作动画**，矢量 Lottie 格式。

> 文件名里的 `_3` 是**形态编号**。客户端按 `action/<动作ID>_<superStage>` 取资源。
>
> ⚠️ **形态 3 没有「充电」动作**：原资源里 `super/action/` 只有 `2_3` ~ `7_3`，
> **没有 `1_3.swf`**（充电是 id=1）。这是**资源本身的缺口**，不是转换遗漏 ——
> 形态 4 / 形态 5 同样没有 `1_4` / `1_5`。客户端在形态 3 下触发充电会拿不到资源，
> 这里**如实缺席**，没有用别的动作顶替，也没有编造。

配套资产：八方向循环走路在 [`../nono-form3-walk/`](../nono-form3-walk/)，
4 个表情在 [`../nono-form3-expressions/`](../nono-form3-expressions/)，三者取景与宠物锚点一致。

## 文件

| 文件 | 动作 | id | 帧数 | 画布 | 图层 | 体积 |
|---|---|---|---|---|---|---|
| `nono3_action_boot.json` | 开机 | 2 | 84 | 103×113 | 58 | 155 KB |
| `nono3_action_shutdown.json` | 关机 | 3 | 63 | 77×80 | 49 | 91 KB |
| `nono3_action_ball.json` | 玩球 | 4 | 164 | 503×297 | 136 | 403 KB |
| `nono3_action_battery.json` | 吃电池 | 5 | 275 | 298×325 | 84 | 2.4 MB |
| `nono3_action_summon.json` | 召唤 | 6 | 100 | 410×205 | 60 | 422 KB |
| `nono3_action_cube.json` | 玩魔方 | 7 | 262 | 175×267 | 151 | 2.9 MB |
| `meta.json` | — | — | — | — | — | 尺寸与体积速查 |
| `manifest.json` | — | — | — | — | — | 字节数 / sha256 / 帧数 / 画布 / 图层 / 裁剪原点 / 宠物锚点 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），**25fps**，不依赖任何位图。合计约 **6.4 MB**。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: false,              // 动作只播一遍
  autoplay: true,
  path: 'assets/nono-form3-actions/nono3_action_boot.json',
});
```

- **动作一律 `loop: false`**（原资源是 `gotoAndPlay(1)`，播一遍停在末帧）。
- **摆放按 `petAnchor` 对齐**（见 `meta.json`），不要按画布居中。
- `autoEnd`：**开机 / 玩球 / 吃电池 / 召唤 / 玩魔方 = 1**（末帧派发 `PLAY_COMPLETE`、
  切回走路并清理动作）；**关机 = 0**（客户端在这两个动作播完时不派发完成事件、
  不切回走路，动作停在末帧保持占用状态）。
- 画布尺寸差异很大（77×80 ~ 503×297），是原资源的构图决定的。

## 音效

形态 3 的 6 个动作**全部有内嵌音效**（`super/action/*_3.swf` 里的 `sou` 类），
已提取到 [`../nono-sounds/sounds/`](../nono-sounds/sounds/)，
文件名带 `_s3` 后缀（`action_boot_s3.mp3` 等）。

| 动作 | 音效 | 播放规则 |
|---|---|---|
| 开机 | `action_boot_s3.mp3` | 动作开始时播 1 次 |
| 关机 | `action_shutdown_s3.mp3` | 动作开始时播 1 次 |
| 玩球 | `action_ball_s3.mp3` | 动作开始时播 1 次 |
| 吃电池 | `action_battery_s3.mp3` | 动作开始时播 1 次 |
| 召唤 | `action_summon_s3.mp3` | 动作开始时播 1 次 |
| 玩魔方 | `action_cube_s3.mp3` | 动作开始时播 1 次 |

播放与停止规则写在 **两个方向**的清单里：动画 manifest 的 `audio` 段
（`soundId` / `trigger` / `loops` / `stop` / `playAs3` / `playWeb`），
以及音效 manifest 的 `bindings[]`。两边的数据来自**同一个数据源**
`_work/tools/audio_spec.py`，不会写岔。

> 充电（id=1）在本形态没有资源，也就没有音效 —— 不塞占位音频。

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到，Lottie 帧由 **lottie-web 在浏览器里渲染**，
两边逐像素比对（阈值 40）：

| 动作 | 平均差异 | 最差单帧 |
|---|---|---|
| 玩球 | 0.07% | 949 px (帧 43) |
| 吃电池 | 0.30% | 799 px (帧 205) |
| 开机 | 0.25% | 57 px (帧 52) |
| **玩魔方** | **1.76%** | 5252 px (帧 50) |
| 关机 | 0.53% | 74 px (帧 0) |
| 召唤 | 0.18% | 973 px (帧 14) |
| **总体** | **0.52%**（332908 / 73146580 px，0.46%） | |

对照基线：形态 1 动作 0.54%、形态 2 动作 0.54%。

**开机动作**是这一批的关键 —— 修复前差 **39.73%**，修完 0.25%
（补上**变形形状**后又降了一档，见 [`../../开发注意事项.md`](../../开发注意事项.md) 铁律 27）。
根因是 `PlaceObject2` 换角色时 **`colorTransform` 没有继承**
（Flash / FFDec 的 `DepthState` 是拷贝语义）：开机第 28 帧用 `alphaMult=0`
把青色漩涡设为全透明，之后每隔 2 帧换一个 segment 角色、且都不带 CT ——
继承则此后全程隐形，误重置成恒等则漩涡变成不透明大色块铺满画布。

**玩魔方 1.76%** 是本批最高的，主要来自魔方方块的三维翻转被展平成
逐帧矢量路径时的边缘抗锯齿差异，没有结构性缺失。

## 已知偏差：BLUR / GLOW 滤镜未还原

形态 3 的动作资源里有一半带 `GLOWFILTER` / `BLURFILTER`
（从 FFDec 导出的 XML 里统计）：

| 资源 | GLOWFILTER | BLURFILTER |
|---|---|---|
| `action/2_3.swf`（开机） | — | — |
| `action/3_3.swf`（关机） | — | — |
| `action/4_3.swf`（玩球） | 143 | — |
| `action/5_3.swf`（吃电池） | 348 | — |
| `action/6_3.swf`（召唤） | 55 | — |
| `action/7_3.swf`（玩魔方） | 360 | **30** |

Lottie（bodymovin 规范）**没有 glow 滤镜**，只有 Drop Shadow 和 Gaussian Blur
两种滤镜效果；GLU 的 glow 无法直接表达。

- **GLOW / BLUR 一律未还原**，本批资产**不做滤镜近似** ——
  形态 1 的动作脚本曾用「同心描边」近似 glow，实测在吃电池 / 玩魔方 / 玩召唤上
  **加了反而差异更大、体积翻倍**，因此宁可保持矢量干净。
- **玩魔方 1.76%**（本批最高）主要就是这批 glow + blur 的边缘差异：
  魔方方块旋转时带辉光，参考帧的方块边缘是柔化的，Lottie 是硬边。

## 重建

```bash
_work/venv/bin/python _work/tools/swf2lottie_action.py --form 3
_work/venv/bin/python _work/tools/build_manifest.py --form 3
```
