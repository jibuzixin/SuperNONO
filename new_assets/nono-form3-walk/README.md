# 超能 NONO · 形态 3 · 八方向走路动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/nono_3.swf` 直接转换而来的
**形态 3（超能 NONO 第三个形态）八方向循环走路动画**，矢量 Lottie 格式。

> 形态 3 与形态 1 / 2 的 **SWF 结构又不一样**：时间轴 id、走路周期、每段帧数、
> 放置指令的种类全都变了。所有转换脚本已按形态参数化（`--form N`），
> 切换形态时 **pet / 时间轴 / 周期 / 取景全部自动检测**，不写死任何 id。

## 文件

| 文件 | 说明 |
|---|---|
| `nono3_down.json` | 向下走 |
| `nono3_leftdown.json` | 左下走 |
| `nono3_left.json` | 向左走 |
| `nono3_leftup.json` | 左上走 |
| `nono3_up.json` | 向上走 |
| `nono3_rightup.json` | 右上走 |
| `nono3_right.json` | 向右走 |
| `nono3_rightdown.json` | 右下走 |
| `meta.json` | 尺寸与体积速查 |
| `manifest.json` | 逐方向的字节数 / sha256 / 帧数 / 图层数 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），画布统一 **77×80**，**16 帧 @ 25fps** 无限循环。
单文件 65~135 KB，八方向合计约 **0.98 MB**，不依赖任何位图。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: true,
  autoplay: true,
  path: 'assets/nono-form3-walk/nono3_up.json',
});
```

- `w` / `h` = 77 / 80，即 SVG viewBox 的尺寸；渲染时按需缩放不失真。
- 坐标原点在**画布左上角**，宠物原点固定在 `(39.40, 71.99)`（见 `meta.json` 的 `petAnchor`）。
- 帧范围 `[ip, op)` = `[0, 16)`；`fr` = 25，与原素材一致。
- 层序：`layers[0]` 是**最上层**（lottie-web 的绘制顺序）。
- **摆放必须按 `petAnchor` 对齐**（与形态 1 / 2 的约定一致，详见 [`../../assets/README.md`](../../assets/README.md)）。

## 形态 3 的结构（自动检测结果）

| 项 | 形态 3 的值 |
|---|---|
| pet | sprite **124**（根时间轴唯一 PlaceObject2） |
| 顶层方向时间轴 | **5 条**：`foot`=45 / `color_1`=56 / `body`=81 / `color_2`=94 / `foot_2`=123 |
| 嵌套方向时间轴 | **0 条**（形态 2 有 4 条） |
| 方向标签帧号 | down=0 / leftdown=7 / left=15 / leftup=23 / **up=31** / rightup=39 / right=47 / rightdown=55 |
| 走路周期 | **16 帧**（形态 1 是 13、形态 2 是 19） |
| `PlaceObject3Tag` 数量 | 438（形态 1 是 0、形态 2 是 312） |
| 画布 | 77×80 |

`up` 方向的显示列表有个特点：**body（sprite 81）和 color_1（sprite 56）在 up 段是空的**
——sprite 81 在第 31 帧 `RemoveObject2(depth=1)` 之后没有任何 `PlaceObject2`，
所以「上」方向的身体完全由 `color_2`(93) 和 `foot_2`(122) 两条链贡献。
这不是转换丢东西，是**原资源就这么设计的**（参考帧同样如此）。

## 转换时踩到的三个坑（形态 3 独有）

形态 1 / 2 的坑在 [`../nono-form2-walk/README.md`](../nono-form2-walk/README.md) 里，
下面三条是形态 3 转换时**新踩**的。

### 1. SWF 的斜切（shear）在 Lottie 里表达不了 → 「上」方向缺一整层轮廓

Lottie 的图层变换只有 **位移 + 旋转 + 缩放**，没有斜切；而 SWF 的 `MATRIX` 里
`rotateSkew0` / `rotateSkew1` 是**两个独立分量**，Adobe 的运动补间会逐帧改变斜切量。
`decompose()` 只能还原出旋转+缩放，**斜切被静默丢弃** —— 表现就是部件位置对、
轮廓宽度对不上。

修法（`_work/tools/swf2lottie.py`）：

1. 把矩阵拆成 `R·S·K`（`K` 是斜切）；
2. 图层变换用 `R·S`；
3. **把斜切烘焙进路径几何** —— 对每个控制点做 `x' = x + k·y`（或 `y' = y + k·x`）。

线性映射与贝塞尔控制点可交换，所以直接变换控制点即可，落点与原 SWF 完全一致。
判据用**像素偏移**（`|k| × 形状局部跨度`），阈值 `SHEAR_PX_EPS = 0.25`，
并且**逐帧拆层**（`ip = g` / `op = g+1`），因为斜切量每帧都在变。

效果：形态 3 走路总体差异 **3.56% → 1.17%**（`leftup` 8.36%→0.79%、
`rightup` 8.24%→0.66%）。

> 动作 / 表情脚本**没有**做斜切烘焙：实测「吃电池」的斜切偏移上界高达 19.10 px，
> 但逐像素差异只有 0.30% —— 说明那批动画的形状跨度大、斜切落在轮廓外侧，
> 烘焙的收益不抵体积与复杂度。**别为了「看起来严谨」就无脑套用**。

### 2. `PlaceObject2` 换角色时必须继承 `colorTransform` → 开机动作差 63%

Flash / FFDec 的 `DepthState` 是**拷贝语义**：`PlaceObject2/3` 带了 `characterId`
但**没有** `colorTransform` 时，新角色沿用该深度上**原有的 CT**。
我们早先把 CT 重置成恒等，于是：

形态 3「开机」里，`sprite56` 的第 28 帧用 `alphaMult=0` 把青色漩涡设为全透明，
之后第 30 / 32 / 34 … 帧**每隔 2 帧换一个 segment 角色、且都不带 CT** ——
继承则此后全程隐形；误重置成恒等则漩涡变成 **158% 缩放的不透明大色块铺满画布**。

修法：`swf2lottie_action.py` 的 `display_lists()` 里，
`if tag.find('colorTransform') is None and old is not None: cx = old.cx`
（透明度同理）。

效果：开机动作 **39.73% → 0.56%**；形态 3 动作总体 **7.18% → 0.64%**。

> 这是「矩阵继承」之外的**同一条铁律的第二条**：铁律 5 说矩阵要继承，
> CT 与透明度**同样**要继承。三者是一起用 `state_at()` 里那份「累积到当前帧的显示列表」
> 表达的。

### 3. 嵌套 clip 必须继承 `place_frame` → 「上」方向的三档触须共用同一缩放

SWF 里同一个 sprite 每被 `PlaceObject` 一次就是**一个独立实例**，
各自从自己的第 1 帧开始播。父时间轴上**晚放置**的子 clip 才开始播，
早放置的已经播了若干帧。

形态 3「上」方向正好踩中：

```
sprite123(foot_2) f31 depth1 char=122
  └ sprite122 f0 depth1/depth7 char=119
      └ sprite119 f0 depth5 / f2 depth3 / f4 depth1 char=118   ← 三档触须，依次晚 2 帧
          └ sprite118 f0 depth1 char=117   ← sprite118 自带缩放动画（1.0 → 0.8 → 0.617 → …）
```

`sprite119` 在第 0 / 2 / 4 帧把 `sprite118` 放到 depth 5 / 3 / 1，
**三档本该各自从 `sprite118` 的第 0 帧起播**（最宽那档拿 1.0）。
早先的走路脚本一律用全局帧号 `lf = g % fc`，等于假设三档在父的第 0 帧同时被放置 ——
三档共用同一缩放，**触须比原 SWF 窄一大截**（实测「上」方向差 2.73%）。

修法（`swf2lottie.py` 的 `Direction._walk()`）：递归时带上**有效时间 `t`**
（本 clip 实例从被放置起已播的帧数），子级的有效时间 =
**父的有效时间 − 自己的 place_frame**，一路往下传、中间层不参与截断。

> 动作脚本**早就是按这个公式做的**（`t_child = t − slot.place_frame`），
> 走路脚本漏了。表情脚本复用动作脚本，所以表情本来就是对的。

效果：「上」方向 **2.73% → 0.48%**，形态 3 走路总体 **1.17% → 0.87%**。

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到（根时间轴展到 16 帧、方向段搬到最前、
按 Lottie 画布取景、去背景色），Lottie 帧由 **lottie-web 在浏览器里渲染**，
两边逐像素比对（阈值 40，与形态 1 / 2 一致）：

| 方向 | 平均差异 | 最差单帧 | 方向 | 平均差异 | 最差单帧 |
|---|---|---|---|---|---|
| down | 0.96% | 111 px (帧 15) | up | **0.48%** | 40 px (帧 14) |
| leftdown | 1.10% | 102 px (帧 15) | leftup | 0.77% | 110 px (帧 15) |
| left | 1.10% | 102 px (帧 15) | rightup | 0.63% | 113 px (帧 15) |
| rightdown | 0.96% | 95 px (帧 15) | right | 0.96% | 95 px (帧 15) |
| **总体** | **0.87%**（6855 / 788480 px） | | | |

残差全部是 **1px 的抗锯齿边缘**（FFDec 的 Java2D 抗锯齿与 lottie-web 的 SVG
抗锯齿算法不同，轮廓像素必然有 1px 差异），没有结构性缺失。
对照基线：形态 1 走路 0.28%、形态 2 走路 0.81%，形态 3 图形更碎
（最多 94 层 vs 形态 2 的 21 层），边缘像素占比自然更高。

## 音效：无

走路**没有音效**（客户端的走路分支只做 `gotoAndStop(direction)`，完全不碰 `Sound`）。
`manifest.json` 里每个方向的 `audio.status` 为 `none`，如实地标注了这个事实，
没有塞占位音频。音效资产在 [`../nono-sounds/`](../nono-sounds/)。

## 与原 SWF 的差异

| 项 | 说明 |
|---|---|
| **未**做耳部镜像修正 | 形态 1 的右耳镜像用的是**形态 1 专属的标定值**（shape47/49/50/51），形态 3 的 shape id 与图形结构完全不同，直接套会改错图形。 |
| **未**做「提到本体之上」修正 | 形态 1 / 形态 2 的耳朵会被本体黑色压住一小块，是原资源自带的层序错误。形态 3 的左上 / 右上**没有这个问题**（耳朵是青色的鳍状触须，与本体黑色分离，已逐帧核对参考帧），所以保持原 SWF 层序、逐像素一致。`ON_TOP_GROUPS` 里也没有形态 3 的条目。 |

## 重建

```bash
# 1) 转换（结构自动检测，--form 切换）
_work/venv/bin/python _work/tools/swf2lottie.py --form 3

# 2) 重建参考 SWF + FFDec 渲染参考帧（顺序执行，别并发，机器会烤热）
bash _work/tools/render_ref_f3.sh

# 3) 浏览器渲染 Lottie 帧落盘（落盘页必须从 8903 打开，落盘服务在 8905）
#    http://127.0.0.1:8903/preview/_dump_action.html?kind=walk&form=3&a=down,leftdown,left,leftup,up,rightup,right,rightdown&f=all&d=0&dump=8905

# 4) 逐像素比对
_work/venv/bin/python _work/tools/cmp_all.py ref_walk_f3 lottiedump_f3
```
