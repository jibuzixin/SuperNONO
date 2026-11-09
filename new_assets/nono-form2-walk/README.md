# 超能 NONO · 形态 2 · 八方向走路动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/nono_2.swf` 直接转换而来的
**形态 2（超能 NONO 第二个形态）八方向循环走路动画**，矢量 Lottie 格式。

> 形态 2 与形态 1 的 **SWF 结构完全不同**：时间轴条数、character id、走路周期、
> 用到的放置指令类型都不一样。所有转换脚本已按形态参数化（`--form N`），
> 切换形态时 **pet / 时间轴 / 周期 / 取景全部自动检测**，不写死任何 id。

## 文件

| 文件 | 说明 |
|---|---|
| `nono2_down.json` | 向下走 |
| `nono2_leftdown.json` | 左下走 |
| `nono2_left.json` | 向左走 |
| `nono2_leftup.json` | 左上走 |
| `nono2_up.json` | 向上走 |
| `nono2_rightup.json` | 右上走 |
| `nono2_right.json` | 向右走 |
| `nono2_rightdown.json` | 右下走 |
| `meta.json` | 尺寸与体积速查 |
| `manifest.json` | 逐方向的字节数 / sha256 / 帧数 / 图层数 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），画布统一 **76×83**，**19 帧 @ 25fps** 无限循环。
单文件 106~122 KB，八方向合计约 **0.92 MB**，不依赖任何位图。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: true,
  autoplay: true,
  path: 'assets/nono-form2-walk/nono2_up.json',
});
```

- `w` / `h` = 76 / 83，即 SVG viewBox 的尺寸；渲染时按需缩放不失真。
- 坐标原点在**画布左上角**，宠物原点固定在 `(37.02, 76.57)`（见 `meta.json` 的 `petAnchor`）。
- 帧范围 `[ip, op)` = `[0, 19)`；`fr` = 25，与原素材一致。
- 层序：`layers[0]` 是**最上层**（lottie-web 的绘制顺序）。
- **摆放必须按 `petAnchor` 对齐**（与形态 1 的约定一致，详见 [`../../assets/README.md`](../../assets/README.md)）。

## 形态 2 与形态 1 的结构差异（转换时踩到的三个坑）

形态 1 的 `nono_1.swf` 里 **一个 `PlaceObject3` 都没有**，全是 `PlaceObject2`；
形态 2 的 `nono_2.xml` 里有 **312 个 `PlaceObject3`**（形态 3/4/5 分别还有 438 / 168 / 260 个）。
三个坑都踩在形态 2 上，形态 1 完全不受影响（回归时 JSON **sha256 逐字节不变**）。

### 1. 只认 PlaceObject2 → 4 根触角整组丢失

`PlaceObject3` 与 `PlaceObject2` 的语义几乎一样（多了 filter / visible / blendMode），
但形态 2 的**触角那一支整条都是 PO3**：

```
body(157) → 88 → 73 → 87 → 86 → { 76 → 75 → shape74(#33FFFF),
                                  77 → 75 → shape74,
                                  85 → 84 → { 80 → 79(#CCFFFF), 83 → 82 } }
```

`sprite 76 / 77` 的放置指令全是 `PlaceObject3Tag`。早先的 `state_at()` 只处理
`PlaceObject2Tag`，结果这 4 根淡青色触角**一根都没画出来** ——
逐像素差 5.41%，参考帧里明明有 4 根。

### 2. 嵌套的方向时间轴按帧号推进 → 「头 + 触角」在第 8 帧整组消失

每个方向时间轴都是 63 帧、按 `FrameLabel` 切成 8 段（down / leftdown / left / leftup /
up / rightup / right / rightdown），每段 7~8 帧：

```
sprite 23(foot)   f0  LABEL:down | PlaceObject2(6)
                  f7  RemoveObject2(d1) | LABEL:leftdown | PlaceObject2(10)
                  ...
```

客户端 `NonoModel.as` 对 pet 的 5 个子剪辑（`foot` / `color_1` / `body` / `color_2` /
`foot_2`）逐个 `gotoAndStop(方向)`，**方向变了才重新定位**。所以在这段时间里，
方向时间轴必须**一直停在当前方向的段里**。

形态 1 只有 3 条方向时间轴、且都是顶层，按「段首帧的显示列表」取值就对了。
形态 2 有 **9 条**（5 条顶层 + 4 条嵌套：`sprite 6 / 31 / 73 / 87`），
其中嵌套的 `sprite 87` 在第 7 帧会 `RemoveObject` —— 早先的转换按帧号推进
（`lf = g % fc`），结果**第 8 帧起整个「头 + 触角」消失**，参考帧同样如此。
现在嵌套方向时间轴也改用「当前方向的段首帧」，与客户端行为一致。

> 参考 SWF 的构造（`_work/tools/swf_ref13.py`）同步改成**重建全部 9 条方向时间轴**
> （把目标方向的段搬到最前、丢掉段末的 `RemoveObject`、补齐到周期帧数），
> 参考帧才是一段完整的走路循环，而不是「前 7 帧是 down、后面混进别的方向」。

### 3. `colorTransform` 的 RGB 部分被忽略 → 触角不会「渐变成白」

`PlaceObject3` 可以带 `colorTransform`（CXFORM）。形态 2 的触角每一帧都带一个
**RGB 变换**：`mult` 从 228 一路降到 0、`add` 从 28 升到 255，
于是 4 根笔画从 `#33FFFF` 渐变成纯白（实测参考帧第 9 帧是 `(232,254,255)`）。

只算 alpha 的话，Lottie 里触角会一直是满色 `#33FFFF`，逐像素差 ~4%。
现在颜色变换沿层级复合（父级 ∘ 子级），并按帧生成**动画填充色**
（hold 关键帧，与 SWF 的阶跃语义一致）。

> 量纲：`c' = c * mult/256 + add`（`mult`/`add` 都是 8.8 定点数）。
> 这一条是**实测反推**的：逐帧采样参考帧的触角颜色，与上式逐帧吻合
> （第 2~9 帧依次得到 73 / 96 / 119 / 141 / 164 / 186 / 209 / 232）。

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到（根时间轴展到 19 帧、方向段搬到最前、
按 Lottie 画布取景、去背景色），Lottie 帧由 **lottie-web 在浏览器里渲染**，
两边逐像素比对（阈值 40，与形态 1 一致）：

| 方向 | 平均差异 | 方向 | 平均差异 |
|---|---|---|---|
| down | 0.93% | up | 0.47% |
| leftdown | 0.98% | leftup | 0.47% |
| left | 0.98% | rightup | 0.57% |
| right | 0.87% | rightdown | 0.87% |
| **总体** | **0.77%**（7365 / 958816 px） | | |

残差全部是 **1px 的抗锯齿边缘**（FFDec 的 Java2D 抗锯齿与 lottie-web 的 SVG
抗锯齿算法不同，轮廓像素必然有 1px 差异），没有结构性缺失。
形态 1 的同一套流程基线是 0.28%，形态 2 的图形更碎（21 层 vs 13 层），
边缘像素占比自然更高。

> ⚠️ 上表是**触角置顶修正之前**测的。修正只影响 `leftup` / `rightup` 两个方向
> （触角提到本体之上），这两个方向**有意不再与原 SWF 逐像素一致**，
> 其余 6 个方向的差异率不变。

## 语义标签（`nono` 字段）· 染色用

八方向走路资产**已打标**（染色/扩展用）。标签是 Lottie 的自定义字段，
lottie-web 忽略它 ⇒ **渲染逐像素不受影响**（本 README 的逐像素基线仍成立）。

| 层 | 标记内容 |
|---|---|
| fill (`fl`) | `role` + `dye`（`solid` = 本体白/耳根白，`shade` = 灰色阴影；**无 `dye` = 不染色**） |
| group (`gr`) | `role` |
| layer | `role` + `shape`（源 SWF 的 character id）+ `glow`（本形态走路有 3 个形状带辉光，见下） |

形态 2 走路的 `role` 分布：`body`（本体白）、`ear_root`（耳根白）、
`body_shade`（灰阴影）、`accent`（青色触角/鳍）、`dark`（黑面罩/投影）。
**走路没有道具**，所以整段里没有任何 `prop` / `effect`。

**辉光（glow）标签**：源 SWF 里身体的几个变体（`down` 的 shape74、
`left/right` 的 shape105/109、`up` 的 shape147/151、`leftup/rightup` 的 shape133/136）
以及触角根部的 shape 带着 `GLOWFILTER`（strength 1.0、blur 5~9）。
走路转换脚本**不处理滤镜**，所以这些光晕**没有画进 Lottie** ⇒
标签里 `baked: false`，运行时可以按标签自己补上这圈辉光。
（证据图：`_work/evidence/f2_walk_glow_gap.png` —— 左 FFDec 参考有明显光晕，中我们的 Lottie 是硬边。）

> 注意：逐像素基线是**白底 + 阈值 40** 比的，白色辉光叠白底上几乎看不出差别，
> 所以这个偏差**不会**体现在下面的基线数字里。

完整标签规范见 [`../README.md`](../README.md#️-语义标签nono-字段染色--以后扩展用)。
预览：`http://127.0.0.1:8903/preview/dye.html?form=2`

## 音效：无

走路**没有音效**（客户端的走路分支只做 `gotoAndStop(direction)`，完全不碰 `Sound`）。
`manifest.json` 里每个方向的 `audio.status` 为 `none`，如实地标注了这个事实，
没有塞占位音频。音效资产在 [`../nono-sounds/`](../nono-sounds/)。

## 与原 SWF 的差异（有意为之）

| 项 | 说明 |
|---|---|
| **左上 / 右上 的耳朵提到本体之上** | 原资源里这两个方向的**耳朵**分成两半放在不同的时间轴：**body 时间轴最上面那个 sprite**（青翼 + 白耳根 = shape136 / 133 / 130）本来就在最上层；但**color_1 时间轴里的耳根**（shape48(depth5) / shape46(depth3)）在 body **之下**，于是被 body 时间轴里的**本体黑色部分**(shape115 黑月牙 / shape126 黑眼罩)压住 → 耳朵的白色部分被黑色吞掉一小块。这是**原 SWF 自带的层序错误**。修法与形态 1 完全同构：把耳朵整组（body 顶 3 层 + color_1 耳根 2 层）提到最上层，耳朵内部保持原有相对次序。<br>（对照「上」方向：同一份耳根是 shape56/shape54，body 里的黑色 shape144/shape142 是弧形、没盖到它们，所以「上」方向没这个问题。） |
| **未**做形态 1 的耳部镜像修正 | 形态 1 的右耳镜像用的是**形态 1 专属的标定值**（shape47/49/50/51），形态 2 的 shape id 与图形结构完全不同，直接套会改错图形。 |
| `rightdown` 沿用 `right`（部分层） | 原资源的 `rightdown` 段没有自己的动作片段，继续播放 `right` 的内容。已如实还原，未伪造。 |

> 置顶修正在 `_work/tools/swf2lottie.py` 的 `ON_TOP_GROUPS` 里按形态配置，
> 支持两种写法：`[...]` = 整组提到最上层（形态 1 / 形态 2 当前用法），
> `{'raise': [...], 'over': [...]}` = 只提到 `over` 那几层之上、其余层序不动。
> `--no-ears-on-top` 可退回原 SWF 层序。
>
> **怎么认准「耳朵」是哪几层**：形态 1 / 形态 2 同构 —— 耳朵 =
> **body 时间轴最上面几个 sprite** + **color_1 时间轴 depth5/depth3**。
> 形态 1 的 `EAR_GROUPS` 就是这么标的。别凭 shape 名字或颜色猜：
> 形态 2 的 shape11 是**手/脚**、shape136/133 是**触角**、shape44 是**身体**（连着耳朵白壳）。
> 用 `_work/tools/layer_probe.html`（逐层单渲染 / 隐藏图层）核对形状。

## 重建

```bash
# 1) 转换（形态 1/2 自动检测结构，--form 切换）
_work/venv/bin/python _work/tools/swf2lottie.py --form 2

# 2) 重建参考 SWF + FFDec 渲染参考帧（顺序执行，别并发）
bash _work/tools/render_ref_f2.sh

# 3) 浏览器渲染 Lottie 帧落盘（8904 端口的落盘服务）
#    http://127.0.0.1:8903/preview/_dump_action.html?kind=walk&form=2&a=down,leftdown,left,leftup,up,rightup,right,rightdown&f=all&d=0

# 4) 逐像素比对
_work/venv/bin/python _work/tools/cmp_all.py ref_walk_f2 lottiedump_f2
```
