# 超能 NONO · 形态 4 · 八方向走路动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/nono_4.swf` 直接转换而来的
**形态 4（超能 NONO 第四个形态）八方向循环走路动画**，矢量 Lottie 格式。

> 形态 4 的 **SWF 结构**与形态 1 / 2 / 3 又不一样：pet=152、5 条顶层方向时间轴
> （26 / 34 / 66 / 80 / 151）、走路周期 **21 帧**、画布 **189×104**（形态 1 是 66×96、
> 形态 2 是 76×83、形态 3 是 77×80）。
> 所有转换脚本已按形态参数化（`--form N`），切换形态时
> **pet / 时间轴 / 周期 / 取景全部自动检测**，不写死任何 id。

## 文件

| 文件 | 说明 |
|---|---|
| `nono4_down.json` | 向下走 |
| `nono4_leftdown.json` | 左下走 |
| `nono4_left.json` | 向左走 |
| `nono4_leftup.json` | 左上走 |
| `nono4_up.json` | 向上走 |
| `nono4_rightup.json` | 右上走 |
| `nono4_right.json` | 向右走 |
| `nono4_rightdown.json` | 右下走 |
| `meta.json` | 尺寸与体积速查 |
| `manifest.json` | 逐方向的字节数 / sha256 / 帧数 / 图层数 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），画布统一 **189×104**，**21 帧 @ 25fps** 无限循环。
单文件 173~264 KB，八方向合计约 **1.65 MB**，不依赖任何位图。

| 方向 | 图层数 | 体积 | 方向 | 图层数 | 体积 |
|---|---|---|---|---|---|
| down | 213 | 264 KB | up | 209 | 248 KB |
| leftdown | 61 | 174 KB | rightup | 211 | 250 KB |
| left | 61 | 174 KB | right | 61 | 172 KB |
| leftup | 211 | 250 KB | rightdown | 61 | 172 KB |

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: true,
  autoplay: true,
  path: 'assets/nono-form4-walk/nono4_up.json',
});
```

- `w` / `h` = 189 / 104，即 SVG viewBox 的尺寸；渲染时按需缩放不失真。
- 坐标原点在**画布左上角**，宠物原点固定在 `(92.05, 94.02)`（见 `meta.json` 的 `petAnchor`）。
- 帧范围 `[ip, op)` = `[0, 21)`；`fr` = 25，与原素材一致。
- 层序：`layers[0]` 是**最上层**（lottie-web 的绘制顺序）。
- **摆放必须按 `petAnchor` 对齐**（与形态 1 / 2 / 3 的约定一致，详见 [`../../assets/README.md`](../../assets/README.md)）。

## 形态 4 的结构（自动检测结果）

| 项 | 形态 4 的值 | 对照形态 3 |
|---|---|---|
| pet | sprite **152** | 124 |
| 顶层方向时间轴 | **5 条**：foot=26 / color_1=34 / body=66 / color_2=80 / foot_2=151 | 45 / 56 / 81 / 94 / 123 |
| 嵌套方向时间轴 | **0 条** | 0 条 |
| 方向时间轴总长 | 63 帧 | 63 帧 |
| 方向标签帧号 | down=0 / leftdown=7 / left=15 / leftup=23 / **up=31** / rightup=39 / right=47 / rightdown=55 | 同上 |
| 走路周期 | **21 帧** | 16 帧 |
| `PlaceObject3Tag` 数量 | 168 | 438 |
| `GLOWFILTER` 数量 | 168 | — |
| 画布 | **189×104** | 77×80 |

5 条顶层时间轴与形态 3 **一一对应**（按内容判定：foot 含 2 个 shape、
color_1 / color_2 各 1 个、body 含 4 个、foot_2 含 2 个），
所以客户端 `NonoModel.as` 里那套 `getChildByName("color_1"/"color_2"/"body"/"foot"/"foot_2")`
在形态 4 上照样对得上。

## ⚠️ 走路的**真实播放语义**（跨形态通用，影响怎么看逐像素数字）

读客户端源码 `NONO形态与动作_20261021/客户端源码/NonoModel.as` 得到的结论：

```actionscript
_bodyUpColorMC.gotoAndStop(direction);   // 定位到该方向的段首帧
…
private function onWalkStart(e:Event):void {
    if(_loc2_.currentFrame == 1) { _loc2_.gotoAndStop(2); }   // 走路开始 → 推到第 2 帧
}
private function onWalkEnd(e:Event):void {
    if(_loc2_.currentFrame == 2) { _loc2_.gotoAndStop(1); }   // 走路结束 → 回到第 1 帧
}
```

也就是说：

- 客户端对各部件只做 `gotoAndStop(direction)` —— **定位到方向段的段首帧后停住**，
  并不连续播放整条时间轴；
- 走路的「动」是 **第 1 帧 ⇄ 第 2 帧** 两个姿态之间的切换，由 `WALK_START` /
  `WALK_END` 事件驱动；
- `currentFrame == 1` 这个判断只在 **down 方向**成立（down 的标签在第 0 帧 ⇒
  `currentFrame` 为 1；其余方向的 `currentFrame` 是 8 / 16 / 24 / 32 / 40 / 48 / 56，
  `onWalkStart` / `onWalkEnd` 对它们是 no-op）。

⇒ **客户端真正会显示的，只有 down 的第 1、2 帧与其它 7 个方向的段首帧。**

本目录的 21 帧循环是「把 down 段连续播 21 帧」的产物（形态 1 / 2 / 3 同理），
它覆盖了客户端不会显示的画面。这也是为什么下面的逐像素数字要分两种口径看。

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到（方向段搬到最前、按 Lottie 画布取景、去背景色），
Lottie 帧由 **lottie-web 在浏览器里渲染**，两边逐像素比对（阈值 40，与形态 1 / 2 / 3 一致）。

### 口径一：整个 21 帧循环

| 方向 | 平均差异 | 最差单帧 | 方向 | 平均差异 | 最差单帧 |
|---|---|---|---|---|---|
| down | 1.42% | 379 px (帧 17) | up | 1.38% | 360 px (帧 17) |
| leftdown | 1.62% | 441 px (帧 16) | rightup | 1.43% | 386 px (帧 16) |
| left | 1.62% | 441 px (帧 16) | right | 1.57% | 443 px (帧 17) |
| leftup | 1.39% | 376 px (帧 16) | rightdown | 1.57% | 443 px (帧 17) |
| **总体** | **1.50%**（49630 / 3302208 px） | | | |

### 口径二：只看客户端实际显示的画面

| 方向 | 显示帧 | 差异 | 方向 | 显示帧 | 差异 |
|---|---|---|---|---|---|
| down | 1, 2 | 1.07% | up | 1 | 1.00% |
| leftdown | 1 | 1.18% | rightup | 1 | 1.09% |
| left | 1 | 1.18% | right | 1 | 1.17% |
| leftup | 1 | 1.12% | rightdown | 1 | 1.17% |
| **总体** | | **1.12%**（1975 / 176904 px） | | | |

**同口径对照形态 3：0.99%（550 / 55440 px）。**

⇒ 形态 4 的转换精度与形态 3 处在**同一水平**：整段 1.50%，客户端实际显示的画面 1.12%。

逐帧差异率呈「先升后落」的形态（down：帧 0 的 1.01% → 帧 17 的 1.83% → 帧 20 的 1.03%），
是因为参考帧与 Lottie 在这 21 帧里的**相位**逐渐拉开再合拢，差异跟着相位差走。

残差集中在**青色鳍的轮廓边缘**，是 1px 级的抗锯齿差异
（FFDec 的 Java2D 抗锯齿与 lottie-web 的 SVG 抗锯齿算法不同），
没有结构性缺失 —— 放大对比可见鳍的形状、长度、开合全都一致。

## 形态 4 的五个特有现象（都不是转换缺陷）

### 1. `left` 与 `leftdown` 画面完全相同，`right` 与 `rightdown` 画面完全相同

对**参考帧**（FFDec 直接渲染原 SWF）逐字节核对：`left/1.png` 与 `leftdown/1.png`
的 md5 相同，`right` 与 `rightdown` 同理。8 个方向时间轴是 8 个不同的 SWF 符号
（md5 互不相同），但这 4 个方向的**最终画面**一样。

⇒ 这是**原资源本身的表现**，不是转换丢东西。

### 2. 4 个方向的图层数暴增（207~211 层 vs 另 4 个的 56 层）

`down` / `leftup` / `up` / `rightup` 这 4 个方向的 SWF 矩阵带**斜切（shear）**，
Lottie 表达不了，于是按形态 3 的做法**逐帧拆层 + 把斜切烘焙进路径几何**
（`ip = g` / `op = g+1`）。拆层数 = 该方向的帧数 × 受斜切影响的实例数，
所以层数从 56 涨到 209~213、文件从 172 KB 涨到 264 KB。

另 4 个方向没有斜切，走的是普通单层路径（61 层）；之所以比早前的 56 层多 5 层，
是因为其中 5 个 shape 的有效帧**不连续**（见下节「旧 shape 没有被移除」），被拆成了多段。

### 3. 旧 shape 没有被移除 → 屏幕上多出一个「完全不动的青色手臂」（已修复）

原 SWF 里，走路飘带是**一串接力放置**的 shape（`nono_4.swf` 的 sprite15）：

```
f0   Place shape5 @depth7
f2   RemoveObject depth7 ;  Place shape6 @depth8
f4   RemoveObject depth8 ;  Place shape7 @depth9
f6   RemoveObject depth9 ;  Place shape8 @depth11
f8~f16  shape9→shape13 依次替换 depth11
f18  RemoveObject depth11 ;  Place shape14 @depth10
```

同一时刻**只有一个**飘带 shape 在显示列表里，后一个顶掉前一个。

转换时曾把一个 shape 的可见区间取成「[第一个有效帧, 最后有效帧+1]」。
而 shape5 的有效帧号是 **[0, 1, 20]**（第 2 帧被 `RemoveObject2` 移除，
第 20 帧是循环回起点又出现）—— 于是它被画成了 `[0, 21)`，
中间那 19 帧里它**本该消失却一直留在画面上**，而且因为后面没有关键帧，
lottie-web 把它的变换**保持**在第 1 帧的位置 ⇒ 屏幕上多出一条
**从头到尾一动不动**的青色手臂。

> 这不是「多余部件」，而是**原 SWF 里明确 `RemoveObject2` 掉的形状没被移除**。

修复：把一个 shape 的有效帧号切成**连续区段**，每个区段单独出一层
（`swf2lottie.py` 的 `frame_segments()`）。形态 4 八方向共 34 处、形态 1/2/3 **零处**。

修复后：

- 整段逐像素差异 **2.14% → 1.50%**（70795 → 49630 px）；
- 客户端实际显示的画面仍是 1.12%（那两个帧本来就是旧 shape 合法可见的帧）；
- 「恒定青色像素块」检测：修复前 `left` 方向有一块 **173 px** 的恒定飘带，
  修复后只剩原图本来就静止的耳朵。

证据：`_work/evidence/f4_left_strip_before_after.png`（原图 / 修复前 / 修复后 × 5 帧）。

### 4. 带 `GLOWFILTER`（168 个）

Lottie（bodymovin 规范）没有 glow 滤镜。本批资产**一律不做滤镜近似**
—— 形态 1 的动作脚本曾用「同心描边」近似 glow，实测在吃电池 / 玩魔方 上
**加了反而差异更大、体积翻倍**（吃电池 3289→4031 px、1.5 MB→6.2 MB；
玩魔方 6175→6545 px、4.6 MB→9.5 MB），因此宁可保持矢量干净。

走路脚本 `swf2lottie.py` **从来没有实现过** glow 近似（是遗漏，不是决定）。
实测「加辉光」的收益上界（用真高斯模糊模拟 Flash 的 glow，取最优参数）：
`left` 方向 1.62% → **1.41%**，即整段只能省约 **0.2 个百分点**；
硬边「同心描边」达不到这个理想值。

### 5. 循环「最后一帧接第一帧」看着突兀 —— 已排查，**不是缺陷，保持现状**

用户反馈八方向走路的最后一帧接第一帧不自然、耳朵在最后一下会突然合并。
逐帧排查结论（证据图 `_work/evidence/seam_*.png`）：

| 方向 | down | up | left | right | leftup | rightup | leftdown | rightdown |
|---|---|---|---|---|---|---|---|---|
| 接缝差异（帧21→帧1） | 254 | 284 | 216 | 216 | 288 | 270 | 216 | 216 |
| 普通相邻差异均值 | 557 | 723 | 684 | 691 | 693 | 686 | 684 | 691 |
| 接缝 ÷ 均值 | 0.46× | 0.39× | 0.32× | 0.31× | 0.42× | 0.39× | 0.32× | 0.31× |

- **周期长度来自源**：形态 4 的走路 clip 就是 `nono_4.swf` 的 **sprite20、21 帧**
  （pet 时间轴 `tl1` 的 left 段 frames 15–22 放的就是它）⇒ 21 帧是忠实的
- **接缝是整段最平滑的一步**：八个方向的接缝差异都只有相邻均值的 0.31~0.46×，
  没有任何方向例外；接缝处也没有形状丢失（2px 容差下缺失面积 ≤ 9px）
- **缩短循环反而更糟**：21 帧接缝 225 → 改 20 帧 **889** → 改 19 帧 **1019**
  （原 SWF 同向：413 → 1249 → 1308）⇒ 21 帧不能动
- **「耳朵合并」是常驻形态**：两耳内缘间距 down 全段 9~12px，侧视（left/right/
  leftdown/rightdown）全程只有 1px，up 全程 2px —— 没有任何一帧真的贴合
- **步态本身是小步/大步 2 帧交替**，21 帧是奇数 ⇒ 循环边界天然多半个节拍，
  节奏上会「绊一下」，这是原资源的编排
- **原游戏从不播这个循环**：`NonoModel.as` 走路分支只 `gotoAndStop(direction)`，
  `WALK_START`/`WALK_END` 在第 1 帧 ⇄ 第 2 帧切换 ⇒ 游戏里每方向只显示 1~2 帧，
  「末帧接首帧」在原产品里从没发生过。我们交付的 21 帧整段循环是给桌宠用的**超集**
- **跨形态一致**：形态 1 接缝差异 0、形态 2 down/up 0、形态 3 down 23 / left 129 /
  up 302 —— 每个形态都跟自己原 SWF 对得上

> **决定（2026-11-02，用户确认）：保持 21 帧忠实循环，不加交叉淡化帧、不加重复首帧。**
> 现有 `assets/nono-form4-walk/*.json` 未做任何改动。

## 与原 SWF 的差异

| 项 | 说明 |
|---|---|
| **未**做耳部镜像修正 | 形态 1 的右耳镜像用的是**形态 1 专属的标定值**（shape47/49/50/51），形态 4 的 shape id 与图形结构完全不同，直接套会改错图形。 |
| **未**做「提到本体之上」修正 | 形态 1 / 2 的耳朵会被本体黑色压住一小块，是原资源自带的层序错误。形态 4 的青色鳍与本体黑色分离，**没有这个问题**（已逐帧核对参考帧），保持原 SWF 层序。`ON_TOP_GROUPS` 里没有形态 4 的条目。 |
| **未**做鳍的同步修正 | 曾试过把左上 / 右上的两条青色鳍改成同一条开合曲线，**效果并不好**（少了那圈翻转后形體不像原版），已撤回，保持源 SWF 原样。 |

## 音效：无

走路**没有音效**（客户端的走路分支只做 `gotoAndStop(direction)`，完全不碰 `Sound`）。
`manifest.json` 里每个方向的 `audio.status` 为 `none`，如实地标注了这个事实，
没有塞占位音频。音效资产在 [`../nono-sounds/`](../nono-sounds/)。

## 重建

```bash
# 1) 导出素材（FFDec，顺序执行，别并发，机器会烤热）
bash _work/tools/export_f4.sh

# 2) 转换（结构自动检测，--form 切换）
_work/venv/bin/python _work/tools/swf2lottie.py --form 4

# 3) 重建参考 SWF + FFDec 渲染参考帧（顺序执行）
bash _work/tools/render_ref_f4.sh

# 4) 浏览器渲染 Lottie 帧落盘（落盘页必须从 8903 打开，落盘服务在 8915）
#    http://127.0.0.1:8903/preview/_dump_action.html?kind=walk&form=4&a=down,leftdown,left,leftup,up,rightup,right,rightdown&f=all&d=0&dump=8915

# 5) 逐像素比对
_work/venv/bin/python _work/tools/cmp_all.py ref_walk_f4 lottiedump_f4
```
