# 超能 NONO · 形态 1 · 八方向走路动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/nono_1.swf` 直接转换而来的
**形态 1（超能 NONO 第一个形态）八方向循环走路动画**，矢量 Lottie 格式。

## 文件

| 文件 | 说明 |
|---|---|
| `nono1_down.json` | 向下走 |
| `nono1_leftdown.json` | 左下走 |
| `nono1_left.json` | 向左走 |
| `nono1_leftup.json` | 左上走 |
| `nono1_up.json` | 向上走 |
| `nono1_rightup.json` | 右上走 |
| `nono1_right.json` | 向右走 |
| `nono1_rightdown.json` | 右下走 |
| `meta.json` | 尺寸与体积速查 |
| `manifest.json` | 逐方向的字节数 / sha256 / 帧数 / 图层数 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），画布统一 **66×96**，**13 帧 @ 25fps** 无限循环。
单文件 46~58 KB，八方向合计约 **397 KB**，不依赖任何位图。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: true,
  autoplay: true,
  path: 'assets/nono-form1-walk/nono1_up.json',
});
```

- `w` / `h` = 66 / 96，即 SVG viewBox 的尺寸；渲染时按需缩放不失真。
- 坐标原点在**画布左上角**，脚底落在 y≈94~96（正好贴底）。
- 帧范围 `[ip, op)` = `[0, 13)`；`fr` = 25，与原素材一致。
- 层序：`layers[0]` 是**最上层**（lottie-web 的绘制顺序）。

## 与客户端的一致性

参照客户端 `客户端源码/NonoModel.as`：

- 原始素材是 SWF 精灵表（`nono_1.swf`，舞台 960×560），客户端把 `_bodyMC` 放到
  `scale 0.8` 的容器里。这份 Lottie **只烘焙了取景偏移，比例与原 SWF 完全 1:1**
  （pet 根矩阵缩放实测为 100%，内容包围盒换算回 stage 坐标与 SWF 逐像素吻合）。
  因此客户端的 `_bodyMC.scaleX = _bodyMC.scaleY = 0.8` **仍需照搬**，
  只需把摆放原点从「舞台中心」换成「取景后的画布左上角」。
- 取景偏移 = 原素材的 `(-437.2, -189.2)`，即从 960×560 舞台里裁出角色周围
  66×96 的一块，八方向共用同一取景，保证切换方向时角色不跳动。
  画布内宠物原点固定在 `(32.70, 90.35)`（= stage `(469.90, 279.55)` − 取景偏移），
  与 `manifest.json` 里的 `walkPetAnchor` 一致。
- 客户端 `onWalkStart` 把 4 个子 clip 从帧 1 切到帧 2、`onWalkEnd` 切回。
  本资产已经把这段行为**烘焙进动画本身**，无需在外部做帧偏移。

## 音效：无

走路**没有音效**，客户端的走路分支只做 `gotoAndStop(direction)`，完全不碰 `Sound`。
`manifest.json` 里每个方向的 `audio.status` 为 `none`，如实地标注了这个事实，
没有塞占位音频。音效资产在 [`../nono-sounds/`](../nono-sounds/)。

## 与原 SWF 的差异（有意为之）

| 项 | 说明 |
|---|---|
| 「上」方向右耳改用左耳水平镜像 | 原资源左右耳**不是同一套图形**，右耳的灰色衬底比青翼宽，沿青翼上沿露出约 1px 橄榄灰边（肉眼可见的「接缝」）。镜像后两耳完全对称。位置与运动矩阵一律沿用原值。 |
| 「上 / 左上 / 右上」耳朵图层提到本体之上 | 原资源里白色/灰色耳根在 `color_1` 时间轴（root depth 3），会被 `body` 时间轴（root depth 5）的黑色头盖压住，看着像耳朵被身体挡住。置顶后耳朵完整可见，耳朵内部相对次序不变。 |
| `rightdown` 沿用 `right` | 原资源的 `rightdown` 段**没有自己的动作片段**（三条时间轴都只有 ShowFrame），继续播放 `right` 的动作。已如实还原，未伪造。 |

这三处都带开关，可退回严格 1:1 复刻原资源：

```bash
_work/venv/bin/python _work/tools/swf2lottie.py --no-ear-symmetry --no-ears-on-top
```

## 播放期一致性（关键）

**所有关键帧都带 `h:1`（hold）**，这是本资产能「原样播放」的前提。

SWF 的显示列表是离散的：`PlaceObject` 在某帧整体替换矩阵，中间帧保持旧值。
而 **lottie-web 默认开启 subframe**（`subframeEnabled = true`），播放时按**小数帧**求值。
若关键帧之间是线性插值，播放时每个部件都会停在「两帧之间」的姿态：

| | 整数帧（暂停/截图看到的） | 半帧（播放时实际显示的） |
|---|---|---|
| 走路·down | `matrix(1,0,0,1,14.82,59.80)` | `matrix(0.9967,0.0766,-0.0766,0.9967,…)` |

上面这 4.4° 的旋转差就是**耳朵闪动、错位**的来源 —— 走路时耳朵跟着身体转，
插值出来的中间角度让耳朵歪着闪。8 个方向**每一帧**都受影响（13 帧里 12 帧不一致）。

改成 hold 后回归验证（渲染 `第 g 帧` 与 `第 g+0.5 帧` 比对 SVG DOM，
`preview/_verify_half_dom.html`）：

| | 修复前 | 修复后 |
|---|---|---|
| 8 个方向（104 帧） | 99 帧不一致 | **0** |

## 重新生成

```bash
# 需要 JDK 11+ 与 FFDec；一次性导出 XML 与 shape SVG
cd _work/tools
java -Djava.awt.headless=true -Djava.io.tmpdir="$PWD/../../_work/tmp" \
     -jar ffdec/ffdec-cli.jar -swf2xml ../../_work/xml nono_1.swf
java -Djava.awt.headless=true -Djava.io.tmpdir="$PWD/../../_work/tmp" \
     -jar ffdec/ffdec-cli.jar -export shape ../../_work/svgshape nono_1.swf

# 生成 8 个方向的 Lottie + manifest
cd ../..
_work/venv/bin/python _work/tools/swf2lottie.py
_work/venv/bin/python _work/tools/build_manifest.py
```

## 验证

- 逐帧逐像素对照原 SWF：`_work/tools/compare_all.py`
  （8 方向 × 13 帧 = 104 帧，每帧硬差异 171~202 px / 6336 px，全在轮廓抗锯齿上）。
- 浏览器端对照：`preview/_verify.html`。
- 细节与实现说明见 `preview/README.md`。
