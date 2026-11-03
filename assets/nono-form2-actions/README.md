# 超能 NONO · 形态 2 · 动作动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/action/{1..7}_2.swf` 直接转换而来的
**形态 2（超能 NONO 第二个形态）7 个动作动画**，矢量 Lottie 格式。

> 文件名里的 `_2` 是**形态编号**，不是变体。客户端按
> `action/<动作ID>_<superStage>` 取资源（`NonoModel.getActionRes()`），
> 所以 `1_2` ~ `7_2` 就是形态 2 玩家能触发的全部 7 个动作。

配套资产：八方向循环走路在 [`../nono-form2-walk/`](../nono-form2-walk/)，
两者取景与宠物锚点一致，可以原地互换。

## 文件

| 文件 | 动作 | 帧数 | 画布 | 图层 | 体积 |
|---|---|---|---|---|---|
| `nono2_action_charge.json` | 充电 | 1 | 76×83 | 8 | 18 KB |
| `nono2_action_boot.json` | 开机 | 91 | 76×86 | 64 | 251 KB |
| `nono2_action_shutdown.json` | 关机 | 61 | 76×83 | 43 | 72 KB |
| `nono2_action_ball.json` | 玩球 | 124 | 877×480 | 107 | 535 KB |
| `nono2_action_battery.json` | 吃电池 | 219 | 314×145 | 103 | 2.10 MB |
| `nono2_action_summon.json` | 召唤 | 149 | 346×157 | 277 | 2.25 MB |
| `nono2_action_cube.json` | 玩魔方 | 250 | 247×252 | 382 | 4.88 MB |
| `meta.json` | — | — | — | — | 尺寸与体积速查 |
| `manifest.json` | — | — | — | — | 字节数 / sha256 / 帧数 / 画布 / 图层 / 裁剪原点 / 宠物锚点 / 内容包围盒 / **音效规则** |

全部为**扁平化** Lottie（无 precomp），每个 shape 实例摊成根级 shape 层，
**25fps**，不依赖任何位图。合计约 **12 MB**。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: false,              // 动作播一遍就停
  autoplay: true,
  path: 'assets/nono-form2-actions/nono2_action_charge.json',
});
anim.addEventListener('complete', () => {
  // 动作播完
});
```

- **7 个动作一律 `loop: false`**（原资源是 `gotoAndPlay(1)`，只播一遍）。
- 其中**充电（id=1）和关机（id=3）的 `autoEnd=0`** —— 客户端在这两个动作播完时
  **不派发 `PLAY_COMPLETE`、不切回走路**，动作会停在末帧保持占用状态；
  其余 5 个（开机/玩球/吃电池/召唤/玩魔方）`autoEnd=1`，播完派发完成事件并切回走路。
- **摆放按 `petAnchor` 对齐**（见 `meta.json`），不要按画布居中 —— 各动作画布尺寸
  差异很大（76×83 ~ 877×480），居中会让宠物乱跳。
- 缩放与落点在**客户端**做，资产保持与原 SWF **1:1**，不要在资产里预缩。

## 音效

形态 2 的动作音效单独放在 [`../nono-sounds/`](../nono-sounds/)，文件名带 `_s2` 后缀
（`action_boot_s2.mp3` 等）。每个动作的 `manifest.json` 里带一个 `audio` 段，
写明「配哪个音效、什么时候播、播几次、怎么播」：

| 动作 | 音效 | 状态 |
|---|---|---|
| 充电 | — | `missing-from-source`（原资源里就没有声音，**没有编造占位音频**） |
| 开机 | `action_boot_s2.mp3` | ok |
| 关机 | `action_shutdown_s2.mp3` | ok |
| 玩球 | `action_ball_s2.mp3` | ok |
| 吃电池 | `action_battery_s2.mp3` | ok |
| 召唤 | `action_summon_s2.mp3` | ok |
| 玩魔方 | `action_cube_s2.mp3` | ok |

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到（根时间轴展到 N 帧、去掉背景色、按 Lottie 画布取景），
Lottie 帧由 **lottie-web 在浏览器里渲染**，两边逐像素比对（阈值 40）：

| 动作 | 平均差异 | 最差单帧 |
|---|---|---|
| 玩球 | 0.02% | 152 px |
| 吃电池 | 0.55% | 426 px |
| 开机 | 0.92% | 177 px |
| 充电 | 0.60% | 38 px |
| 玩魔方 | 0.68% | 938 px |
| 关机 | 0.62% | 59 px |
| 召唤 | 0.40% | 873 px |
| **总体** | **0.54%**（210555 / 86810960 px） | |

残差是 1px 的抗锯齿边缘 + 少部分 GLOW 辉光的近似误差。

## 已知偏差

- **GLOW / BLUR 滤镜**：Lottie 没有对应能力，形态 1 用「同心描边」近似过
  （`_work/tools/swf2lottie_action.py` 的 `GLOW_CFG`），但实测在吃电池 / 玩魔方 /
  玩召唤上**加了反而差异更大、体积翻倍**，所以**本批动作一律不做辉光近似**。
  形态 2 的走路动画里触角带 GLOW，实测辉光对像素的影响很小（主要是 RGB 色彩变换），
  故走路也未做辉光近似。
- **PlaceObject2 的 Ratio 字段（charRatio）**按规范不是缩放，FFDec 渲染器只把它
  透传给 morph 形变，sprite 放置会直接丢弃，故本转换器忽略它。
- 遮罩（clip depth）本批资源里没有用到。

## 重建

```bash
_work/venv/bin/python _work/tools/swf2lottie_action.py --form 2
_work/venv/bin/python _work/tools/build_manifest.py --form 2
```
