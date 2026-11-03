# 超能 NONO · 形态 3 · 表情动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/exp/{1..4}_3.swf` 直接转换而来的
**形态 3（超能 NONO 第三个形态）4 个表情动画**，矢量 Lottie 格式。

> 文件名里的 `_3` 是**形态编号**。客户端按 `exp/<表情ID>_<superStage>` 取资源，
> 所以 `1_3` ~ `4_3` 就是形态 3 玩家能触发的全部 4 个表情。
>
> ⚠️ **形态 3 的表情名字按「画面」定**，与配置表 `<superExp>` 的默认名字对不上
> （逐帧核对参考帧后确认：1_3 画的是「头晕」、3_3 画的是「玩乐」、4_3 画的是「开心」）。
>
> 名字的**单一事实来源是配置表**，不是本目录：
> `NONO形态与动作_20261021/配置表/NoNo配置表_MachXMLInfo.xml`
> → `<superExp>` 是全部超能形态的默认名字；
> → **新增的 `<superExpStage>`** 是**形态专属覆盖**，形态 3 的名字写在这里：
> ```xml
> <superExpStage>
>     <stage id="3">
>         <item id="1" name="头晕"/>
>         <item id="2" name="生气"/>
>         <item id="3" name="玩乐"/>
>         <item id="4" name="开心"/>
>     </stage>
> </superExpStage>
> ```
> 客户端 `MachXMLInfo.getExpName()` 已改成「先查 stage 覆盖，再回落 `<superExp>`」，
> 所以**游戏里显示的名字、配置表、这里的资产名是同一份**。
> 没在 `<superExpStage>` 里列出的形态（1 / 2 / 4 / 5）沿用默认名字，**不受影响**。
>
> **id 与源文件编号永远不动** —— 客户端按 `exp/<id>_<stage>.swf` 寻址，动了会波及形态 1 / 2。
> 形态 3 不需要形态 2 那样的「源文件编号覆盖」（`SOURCE_NUM_OVERRIDE`），
> 因为编号本身是对的，只是名字与画面不符。

配套资产：八方向循环走路在 [`../nono-form3-walk/`](../nono-form3-walk/)，
动作在 [`../nono-form3-actions/`](../nono-form3-actions/)，三者取景与宠物锚点一致。

## 文件

| 文件 | 表情 | id | 帧数 | 画布 | 图层 | 体积 | 来源 |
|---|---|---|---|---|---|---|---|
| `nono3_exp_dizzy.json` | 头晕 | 1 | 76 | 160×126 | 54 | 376 KB | `exp/1_3.swf` |
| `nono3_exp_angry.json` | 生气 | 2 | 75 | 237×215 | 50 | 490 KB | `exp/2_3.swf` |
| `nono3_exp_play.json` | 玩乐 | 3 | 100 | 77×80 | 69 | 407 KB | `exp/3_3.swf` |
| `nono3_exp_happy.json` | 开心 | 4 | 94 | 100×126 | 31 | 436 KB | `exp/4_3.swf` |
| `meta.json` | — | — | — | — | — | — | 尺寸与体积速查 |
| `manifest.json` | — | — | — | — | — | — | 字节数 / sha256 / 帧数 / 画布 / 图层 / 裁剪原点 / 宠物锚点 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），**25fps**，不依赖任何位图。合计约 **1.7 MB**。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: false,              // 表情播一遍即结束
  autoplay: true,
  path: 'assets/nono-form3-expressions/nono3_exp_play.json',
});
```

- **4 个表情一律 `loop: false`**（原资源是 `gotoAndPlay(1)`，播一遍就结束；
  配置表 `<exp>` 段没有 `autoEnd` 字段）。
- **摆放按 `petAnchor` 对齐**（见 `meta.json`），不要按画布居中。

## 音效：无

4 个表情在**原资源里就没有声音**（`exp/*_3.swf` 里没有内嵌 `sou` 类），
客户端的表情分支（`NonoModel.as` 的 `_expMC.gotoAndPlay(1)`）也不碰 `Sound`。
`manifest.json` 里每个表情的 `audio.status` 为 `missing-from-source`，
**没有编造占位音频**。音效资产在 [`../nono-sounds/`](../nono-sounds/)。

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到，Lottie 帧由 **lottie-web 在浏览器里渲染**，
两边逐像素比对（阈值 40）：

| 表情 | 平均差异 | 最差单帧 |
|---|---|---|
| 头晕 | 0.81% | 316 px (帧 33) |
| **生气** | **2.50%** | 4373 px (帧 24) |
| 玩乐 | 0.83% | 110 px (帧 84) |
| 开心 | 1.58% | 337 px (帧 17) |
| **总体** | **1.43%**（131618 / 7154185 px，1.84%） | |

对照基线：形态 1 表情 0.54%、形态 2 表情 1.82%。形态 3 与形态 2 同量级。

**生气 2.50%** 是本批最高的，来源是原资源给火焰加的 **GLOW 滤镜**：
参考帧里火焰外围有一圈柔和的辉光（参考帧第 24 帧的火焰边缘是渐变的，
Lottie 是硬边），辉光边缘的像素必然对不上 —— 生气表情的画布最大（237×215）、
火焰铺满画面，受影响的边缘像素占比也最高。**这是已知的、有意接受的偏差**，
详见下面的「已知偏差」。

## 已知偏差：GLOW 滤镜未还原

形态 3 的 4 个表情**全部带 `GLOWFILTER`**（从 FFDec 导出的 XML 里统计）：

| 资源 | GLOWFILTER | BLURFILTER |
|---|---|---|
| `exp/1_3.swf`（头晕） | 200 | — |
| `exp/2_3.swf`（生气） | 68 | — |
| `exp/3_3.swf`（玩乐） | 84 | — |
| `exp/4_3.swf`（开心） | 42 | — |

Lottie（bodymovin 规范）**没有 glow 滤镜**，只有 Drop Shadow 和 Gaussian Blur
两种滤镜效果；GLU 的 glow 无法直接表达。**形态 3 的表情没有 BLUR 滤镜**
（形态 2 的「悲哀」有 37 个 `blurX=blurY=15`，形态 3 没有）。

> 形态 1 的动作脚本曾用「同心描边」近似 glow，实测在吃电池 / 玩魔方 / 玩召唤上
> **加了反而差异更大、体积翻倍**，因此本批资产一律不做滤镜近似，
> 宁可保持矢量干净，也不引入更差的近似。

## 重建

```bash
_work/venv/bin/python _work/tools/swf2lottie_exp.py --form 3
_work/venv/bin/python _work/tools/build_manifest.py --form 3
```
