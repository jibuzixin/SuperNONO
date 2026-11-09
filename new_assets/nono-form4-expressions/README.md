# 超能 NONO · 形态 4 · 表情动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/exp/{1..4}_4.swf` 直接转换而来的
**形态 4（超能 NONO 第四个形态）4 个表情动画**，矢量 Lottie 格式。

> 文件名里的 `_4` 是**形态编号**。客户端按 `exp/<表情ID>_<superStage>` 取资源，
> 所以 `1_4` ~ `4_4` 就是形态 4 玩家能触发的全部 4 个表情。
>
> **表情名字的单一事实来源是配置表**，不是本目录：
> `NONO形态与动作_20261021/配置表/NoNo配置表_MachXMLInfo.xml`
> → `<superExp>` 是全部超能形态的默认名字；
> → **`<superExpStage>`** 是**形态专属覆盖**（形态 3 的名字写在这里）。
> 客户端 `MachXMLInfo.getExpName()` 已改成「先查 stage 覆盖，再回落 `<superExp>`」。
>
> **形态 4 已有专属覆盖**（得意 / 生气 / 头晕 / 玩乐）—— 画面与默认名字对不上，
> 按画面重新定名：
> ```xml
> <superExpStage>
>     <stage id="3">…</stage>
>     <stage id="4">
>         <item id="1" name="得意"/>
>         <item id="2" name="生气"/>
>         <item id="3" name="头晕"/>
>         <item id="4" name="玩乐"/>
>     </stage>
> </superExpStage>
> ```
> 名字与画面的对应依据（FFDec 参考帧逐条核对）：
>
> | id | 源文件 | 画面特征 | 名字 | slug |
> |---|---|---|---|---|
> | 1 | `exp/1_4.swf` | 眯眼坏笑、竖起青色耳朵、手臂左右平伸摆动 | **得意** | `smug` |
> | 2 | `exp/2_4.swf` | 瞪大圆眼、眉毛竖起、一臂平举一臂斜举 | **生气** | `angry` |
> | 3 | `exp/3_4.swf` | 头顶一圈**黄色星星旋转**、身体下沉 | **头晕** | `dizzy` |
> | 4 | `exp/4_4.swf` | 闭眼张口、身边散落青色小点、最后低头垂下 | **玩乐** | `play` |
>
> 画面核对图见
> [`../../../_work/evidence/f4_exp_strips.png`](../../../_work/evidence/f4_exp_strips.png)
> （每个表情 6 帧）与
> [`../../../_work/evidence/f4_exp_names.png`](../../../_work/evidence/f4_exp_names.png)。
> 资产命名会直接读这份配置表，**改了配置表就等于改了资产名**。
>
> **id 与源文件编号永远不动** —— 客户端按 `exp/<id>_<stage>.swf` 寻址，动了会波及形态 1 / 2。

配套资产：八方向循环走路在 [`../nono-form4-walk/`](../nono-form4-walk/)，
动作在 [`../nono-form4-actions/`](../nono-form4-actions/)，三者取景与宠物锚点一致。

## 文件

| 文件 | 表情 | id | 帧数 | 画布 | 图层 | 体积 | 来源 |
|---|---|---|---|---|---|---|---|
| `nono4_exp_smug.json` | 得意 | 1 | 64 | 189×104 | 149 | 289 KB | `exp/1_4.swf` |
| `nono4_exp_angry.json` | 生气 | 2 | 63 | 189×109 | 142 | 242 KB | `exp/2_4.swf` |
| `nono4_exp_dizzy.json` | 头晕 | 3 | 66 | 189×104 | 149 | 301 KB | `exp/3_4.swf` |
| `nono4_exp_play.json` | 玩乐 | 4 | 63 | 189×137 | 150 | 251 KB | `exp/4_4.swf` |
| `meta.json` | — | — | — | — | — | — | 尺寸与体积速查 |
| `manifest.json` | — | — | — | — | — | — | 字节数 / sha256 / 帧数 / 画布 / 图层 / 裁剪原点 / 宠物锚点 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），**25fps**，不依赖任何位图。合计约 **1.06 MB**。

> 图层数比「一个形状一个图层」多很多，是因为**头顶那两只耳朵是变形形状**
> （`DefineMorphShapeTag`），几何逐帧变化 —— 每个可见帧单独一层（`ip=g, op=g+1`）。
> 详见 [`../../开发注意事项.md`](../../开发注意事项.md) 铁律 27。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: false,              // 表情播一遍即结束
  autoplay: true,
  path: 'assets/nono-form4-expressions/nono4_exp_smug.json',
});
```

- **4 个表情一律 `loop: false`**（原资源是 `gotoAndPlay(1)`，播一遍就结束；
  配置表 `<exp>` 段没有 `autoEnd` 字段）。
- **摆放按 `petAnchor` 对齐**（见 `meta.json`），不要按画布居中。

## 音效：无

4 个表情在**原资源里就没有声音**（`exp/*_4.swf` 里没有内嵌 `sou` 类），
客户端的表情分支（`NonoModel.as` 的 `_expMC.gotoAndPlay(1)`）也不碰 `Sound`。
`manifest.json` 里每个表情的 `audio.status` 为 `missing-from-source`，
**没有编造占位音频**。音效资产在 [`../nono-sounds/`](../nono-sounds/)。

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到，Lottie 帧由 **lottie-web 在浏览器里渲染**，
两边逐像素比对（阈值 40）：

| 表情 | 平均差异 | 最差单帧 |
|---|---|---|
| 得意 | 1.80% | 401 px (帧 34) |
| 生气 | 2.23% | 536 px (帧 11) |
| 头晕 | 2.36% | 540 px (帧 33) |
| 玩乐 | **1.41%** | 476 px (帧 48) |
| **总体** | **1.92%**（105291 / 5484402 px） | |

对照基线：形态 1 表情 0.54%、形态 3 表情 1.84%。形态 4 表情与形态 3 同量级。

> **修复变形形状之前**这四个表情是 **2.09%**（得意 1.94% / 生气 2.53% / 头晕 2.47% / 玩乐 1.54%）——
> 当时头顶两只青色耳朵**整个没画出来**（见下节），画面缺了一大块。
> 补上耳朵后差异反而**变小**（2.09% → 1.92%），因为多出来的画面本身就是原图的一部分。

## 变形形状（morph shape）：头顶那两只青色耳朵

耳朵在原 SWF 里是 **`DefineMorphShapeTag`（变形形状）**，不是普通形状：
它有**起始 / 结束两套边**，靠 SWF 显示列表里 **PlaceObject 的 Ratio 字段**驱动形变
（`进度 = ratio / 65536`，形态 4 的耳朵每段用 `k/7`、`k/8`、`k/9` 递增）。

**FFDec 的 `-export shape` 不导出变形形状** —— 只导出普通 `DefineShapeTag`。
早先只导了 `-export shape`，于是耳朵**整个没被导出成 SVG**，
转换脚本读不到 ⇒ Lottie 里**两只耳朵彻底消失**；
唯一还会闪一下的那一帧，是原 SWF 用**普通形状**兜底的那一帧 ⇒ 看起来「偶尔闪烁」。

修法（详见 [`../../开发注意事项.md`](../../开发注意事项.md) 铁律 27）：

- 另跑 FFDec `-export morphshape` 导出到 `_work/svgmorph/exp4/`；
- 形变 = 起始/结束两套几何**逐控制点线性插值**；
- Lottie 侧**逐帧拆层**（每帧一个图层，`ip=g, op=g+1`，几何烘焙进路径）。

对照图（左 = Lottie，中 = FFDec 渲染的原 SWF，右 = 差异）：
[`../../../_work/evidence/frame_cmp.png`](../../../_work/evidence/frame_cmp.png)

「生气 / 惊讶」偏高的来源是原资源给元素加的 **GLOW 滤镜**（`exp/2_4.xml` 30 个、
`exp/3_4.xml` 66 个）：参考帧里有一圈柔和辉光、Lottie 是硬边，
辉光边缘的像素必然对不上 —— 这是**已知的、有意接受的偏差**。

## 已知偏差：GLOW 滤镜未还原

| 资源 | GLOWFILTER | BLURFILTER |
|---|---|---|
| `exp/1_4.swf`（得意） | 64 | — |
| `exp/2_4.swf`（生气） | 30 | — |
| `exp/3_4.swf`（头晕） | 66 | — |
| `exp/4_4.swf`（玩乐） | 60 | — |

Lottie（bodymovin 规范）**没有 glow 滤镜**，只有 Drop Shadow 和 Gaussian Blur
两种滤镜效果；GLU 的 glow 无法直接表达。

> 形态 1 的动作脚本曾用「同心描边」近似 glow，实测**加了反而差异更大、体积翻倍**，
> 因此本批资产一律不做滤镜近似，宁可保持矢量干净。

## 重建

```bash
# 1) 导出（含**变形形状**的 morphshape 导出，-export shape 不带它们）
bash _work/tools/export_f4.sh

# 2) 转换
_work/venv/bin/python _work/tools/swf2lottie_exp.py --form 4
_work/venv/bin/python _work/tools/build_manifest.py --form 4

# 3) 逐像素验证（变形形状会撑大画布 ⇒ 参考帧必须重新渲染）
bash _work/tools/render_ref_f4_actexp.sh

# 画面核对图（给人工确认表情名字用）
_work/venv/bin/python _work/tools/exp_montage_f4.py
```
