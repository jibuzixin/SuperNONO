# 超能 NONO · 形态 1 · 动作动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/action/{1..7}_1.swf` 直接转换而来的
**形态 1（超能 NONO 第一个形态）7 个动作动画**，矢量 Lottie 格式。

> 文件名里的 `_1` 是**形态编号**，不是变体。客户端按
> `action/<动作ID>_<superStage>` 取资源（`NonoModel.getActionRes()`），
> 所以 `1_1` ~ `7_1` 就是形态 1 玩家能触发的全部 7 个动作。

配套资产：八方向循环走路在 [`../nono-form1-walk/`](../nono-form1-walk/)，
两者取景与宠物锚点一致，可以原地互换。

## 文件

| 文件 | 动作 | 帧数 | 画布 | 图层 | 体积 |
|---|---|---|---|---|---|
| `nono1_action_charge.json` | 充电 | 62 | 66×96 | 9 | 34 KB |
| `nono1_action_boot.json` | 开机 | 170 | 132×243 | 208 | 2.92 MB |
| `nono1_action_shutdown.json` | 关机 | 165 | 190×246 | 91 | 332 KB |
| `nono1_action_ball.json` | 玩球 | 206 | 526×303 | 80 | 643 KB |
| `nono1_action_battery.json` | 吃电池 | 185 | 399×186 | 84 | 1.58 MB |
| `nono1_action_summon.json` | 召唤 | 106 | 370×132 | 111 | 517 KB |
| `nono1_action_cube.json` | 玩魔方 | 263 | 276×318 | 206 | 4.70 MB |
| `meta.json` | — | — | — | — | 尺寸与体积速查 |
| `manifest.json` | — | — | — | — | 字节数 / sha256 / 帧数 / 画布 / 图层 / 裁剪原点 / 宠物锚点 / 内容包围盒 / **音效规则** |

全部为**扁平化** Lottie（无 precomp），每个 shape 实例摊成根级 shape 层，
**25fps**，不依赖任何位图。合计约 **8.7 MB**。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: false,              // 动作播一遍就停
  autoplay: true,
  path: 'assets/nono-form1-actions/nono1_action_charge.json',
});
anim.addEventListener('complete', () => {
  // 对应客户端 onActionScript：动作播完的通知
});
```

- `w` / `h` 就是各动作的画布尺寸（见上表），坐标原点在**画布左上角**。
- 帧范围 `[ip, op)` = `[0, N)`；`fr` = 25，与原素材一致。
- 层序：`layers[0]` 是**最上层**（lottie-web 的绘制顺序）。
- **`loop` 设 `false`**：动作是单次播放，播完停在末帧。
  lottie-web 的 `complete` 事件对应客户端 `addFrameScript(totalFrames-1, onActionScript)`。

### 与客户端的一致性

参照客户端 `客户端源码/NonoModel.as`：

| 客户端行为 | Lottie 侧对应 |
|---|---|
| `_actionMC.scaleX = _actionMC.scaleY = SCALE`（0.8） | **仍需乘 0.8**。本资产与原 SWF 同比例（根矩阵缩放实测 1.0），0.8 是客户端的显示缩放，没有烘焙进来。 |
| `_actionMC.gotoAndPlay(1)` | `autoplay: true` + `loop: false` |
| `addFrameScript(totalFrames-1, onActionScript)` | `anim.addEventListener('complete', ...)`；动作播完停在末帧，与 SWF 行为一致 |
| `_actionMC` 与 `_bodyMC` 互斥显示 | 动作与走路资产可直接替换，宠物锚点一致 |

各动作 SWF 的**根时间轴只有 1 帧**，真正的动画在根放置的那个 sprite 里
（`master`，即 Lottie 里的 N 帧）。客户端 `gotoAndPlay(1)` 播的是根，
根上的 master 会自己播完 —— Lottie 侧对应 `loop: false`。

客户端配置表 `配置表/NoNo配置表_MachXMLInfo.xml` 里每个动作还有两个开关：

| id | 动作 | autoEnd | souLoops | 动画播完后 |
|---|---|---|---|---|
| 1 | 充电 | **0** | 999999 | 停在末帧，不派发 `PLAY_COMPLETE`，不切回走路 |
| 2 | 开机 | 1 | 1 | 派发 `PLAY_COMPLETE`，切回走路，清理动作 |
| 3 | 关机 | **0** | 1 | 停在末帧，不派发 `PLAY_COMPLETE`，不切回走路 |
| 4 | 玩球 | 1 | 1 | 派发 `PLAY_COMPLETE`，切回走路，清理动作 |
| 5 | 吃电池 | 1 | 1 | 派发 `PLAY_COMPLETE`，切回走路，清理动作 |
| 6 | 召唤 | 1 | 1 | 派发 `PLAY_COMPLETE`，切回走路，清理动作 |
| 7 | 玩魔方 | 1 | 1 | 派发 `PLAY_COMPLETE`，切回走路，清理动作 |

**`autoEnd=0` 不是「循环播放」**，这一点容易搞反。客户端的实现是
（`客户端源码/NonoModel.as` 的 `onActionScript()`，即末帧回调）：

```actionscript
this._actionMC.addFrameScript(this._actionMC.totalFrames - 1, null);
if (!MachXMLInfo.getActionIsAutoEnd(this._actionID)) {
    return;                       // ← 提前返回：不派发完成事件、不切回走路、不清理
}
NonoManager.dispatchEvent(new DynamicEvent(NonoEvent.PLAY_COMPLETE, this._actionID));
this._actionID = 0;
if (this._bodyMC) addChild(this._bodyMC);
this.clearActionMC();
```

SWF 的 `gotoAndPlay` 本来就**只播一遍**、到末帧停住。`autoEnd=0` 的效果是
**动作播完停在末帧并保持占用状态**（充电停在充电姿态、关机停在关机画面），
后续切回走路要由宿主自己决定。

因此：

- **7 个动作的 Lottie 一律 `loop: false`** —— 播完停在末帧。
- **充电和关机的 `complete` 事件不能当作「动作结束、请切回走路」的信号**；
  客户端在这两个动作上根本不会派发 `PLAY_COMPLETE`。
  只有其余 5 个动作（开机 / 玩球 / 吃电池 / 召唤 / 玩魔方）可以用
  `anim.addEventListener('complete', ...)` 对应 `onActionScript`。

### 摆放位置：宠物锚点

每个动作的画布原点**不一定是舞台原点**，而是对齐到「宠物原点在画布里落在
走路资产同一位置 `(32.70, 90.35)`」的地方（内容向左/向上溢出时画布相应扩展，
扩展量记在 `petAnchor` 里）。

`manifest.json` 给出了两个数：

- `crop`：该动作的裁剪原点（原 SWF 960×560 舞台坐标）。
- `petAnchor`：宠物原点在**画布内**的坐标。

要叠加/替换动画时，用 `petAnchor` 对齐锚点即可：

```js
// 把动作画布摆到与走路资产同一个锚点位置
el.style.left = `${WALK_PET_ANCHOR.x - action.petAnchor[0]}px`;
el.style.top  = `${WALK_PET_ANCHOR.y - action.petAnchor[1]}px`;
```

各动作的 `crop + petAnchor` 都精确等于该动作 SWF 里宠物根矩阵的平移
（实测误差 ≤ 0.02 px，即 SWF 的 twip 量化粒度），可用来反推原舞台位置。

## 音效

音效资产**单独存放**在 [`../nono-sounds/`](../nono-sounds/)，
每个动作的 `manifest.json` 条目里都带 `audio` 段，写明配哪个音效、什么时候播、播几次：

| 动作 | 音效 | 时长 | 循环 | 状态 |
|---|---|---|---|---|
| 充电 charge | — | — | 999999（无限） | **原资源就没有音效**（SWF 里无 `sou` 类），宿主需自行注入 |
| 开机 boot | `nono-sounds/sounds/action_boot.mp3` | 5.668 s | 1 次 | ✅ |
| 关机 shutdown | `nono-sounds/sounds/action_shutdown.mp3` | 2.080 s | 1 次 | ✅ |
| 玩球 ball | `nono-sounds/sounds/action_ball.mp3` | 9.152 s | 1 次 | ✅ |
| 吃电池 battery | `nono-sounds/sounds/action_battery.mp3` | 8.788 s | 1 次 | ✅ |
| 召唤 summon | `nono-sounds/sounds/action_summon.mp3` | 5.460 s | 1 次 | ✅ |
| 玩魔方 cube | `nono-sounds/sounds/action_cube.mp3` | 12.012 s | 1 次 | ✅ |

客户端行为（`客户端源码/NonoModel.as`）：

```actionscript
// onResAction()：动画开始之后立刻播音效
this._actionMC.gotoAndPlay(1);
this._sc = this._sound.play(0, MachXMLInfo.getActionSouLoops(this._actionID));
// 下一个动作/表情开始前
// clearActionMC()：this._sc.stop();
```

`getActionSouLoops()` 读的是配置表 `<action>` 段的 `souLoops`；
`souLoops <= 0` 则不播。Web 侧接法见 [`../nono-sounds/README.md`](../nono-sounds/README.md)。

## 转换要点

从 SWF 到 Lottie 有几处**必须做对否则整段动画错位**的地方：

1. **FFDec 的 `<matrix>` 不是「缩放+角度」**。
   `scaleX / scaleY / rotateSkew0 / rotateSkew1` 这四个数**已经是合成后的 2×2 矩阵元素**
   （Flash 把缩放折进了旋转项）。映射关系由 FFDec `MATRIX.apply` 字节码确定：
   `x' = scaleX·x + rotateSkew1·y + tx`，`y' = rotateSkew0·x + scaleY·y + ty`。
   误当成角度会让魔方从 0.98 倍缩成 0.4 px（实测差异 67%）。
2. **PlaceObject2 带 characterId 但不带矩阵时，新字符沿用该深度已有矩阵**。
   召唤动作 master 深度 13 先用 `scale(-0.25, 0.25)` 放 shape33，
   之后 shape34~41 不带矩阵地替换同一深度 —— 用单位矩阵会让图形放大 4 倍并跑到右上角。
   修正后召唤画布 1267×552 → 370×132。
3. **charRatio（PlaceObject2 的 Ratio 字段）按规范忽略**。
   它是「形变进度 / 子 sprite 帧号」，不是缩放。FFDec 渲染器只把它透传给 morph 形变，
   sprite 放置直接丢弃（`DefineSpriteTag.getOutline` 字节码证实）。
4. **SWF 阶跃语义必须用 hold 关键帧（`h:1`）表达**。
   SWF 的显示列表是离散的：`PlaceObject` 在某帧整体替换矩阵，中间帧保持旧值。
   而 **lottie-web 默认开启 subframe**（`subframeEnabled = true`），播放时按**小数帧**求值；
   如果关键帧之间是线性插值，播放时每个部件都会停在「两帧之间」的姿态 ——
   肉眼就是**耳朵闪动、错位、旋转**（暂停/截图却是正常的，因为那时是整数帧）。
   lottie-web 的 `interpolateValue` 里 `1===s.h ? s.s[d] : s.s[d]+(g[d]-s.s[d])*l`
   正是 hold：带 `h:1` 的关键帧在任意时刻（含小数帧）都取自己的值。
   动画属性的关键帧 `s` 必须是数组 `[值]`（单关键帧降为静态时 `k` 写标量 ——
   lottie-web 对 1 关键帧动画属性会抛异常并退化成 -999999，图层直接消失）。
5. **每帧显示列表必须对 Slot 做快照**（PlaceObject2 无 characterId 的矩阵更新
   会倒灌历史帧）。
6. **嵌套 clip 按自身帧数取模循环**（实测周期 15、误差 0），不是 clamp 到末帧。
7. **颜色动画只在 colorTransform 的 RGB 部分变化时启用**，且动画颜色至少 2 个关键帧。
8. 每条 SVG path 单独一个 Lottie group 以保留各自的 fill-rule；
   **多个描边共用一份路径**（体积 1/N）。

## 地面阴影：永远压在身体下面（对原作的有意修正）

原作 SWF 把地面阴影（黑色扁椭圆，19.85×8.35）放在显示列表的**上层**，
角色落地时黑椭圆会盖住脚。转换时把影子图层（连同它自己的辉光/模糊层）整块挪到
Lottie 图层数组**末尾**（`layers[0]` 是最上层，末尾才是最底层），
规则与表情资产一致（`swf2lottie_action.py` 的 `shadow_shape_ids()`，
详见 `../nono-form1-expressions/README.md` 与根目录 `开发注意事项.md` 铁律 12）。

7 个动作的影子层级变化：

| 动作 | 影子原来在第几层 | 现在 |
|---|---|---|
| charge 充电 | 8 / 9 | 8（本来就在最底） |
| boot 开机 | 176, 177 / 208 | 206, 207（最底） |
| shutdown 关机 | 84 / 91 | 90（最底） |
| ball 玩球 | 77, 79 / 80 | 78, 79（最底） |
| battery 吃电池 | 79 / 86 | 85（最底） |
| summon 召唤 | 73, 101 / 111 | 109, 110（最底） |
| cube 玩魔方 | 57, 191 / 206 | 204, 205（最底） |

## 逐像素验证

用 FFDec 直接渲染原 SWF 得到的参考帧，与浏览器里 lottie-web 渲染的结果逐像素比对
（阈值：色差 > 40），**1157 帧全量**（不是抽样）：

| 动作 | 平均差异 | 总差异 px | 最差单帧 |
|---|---|---|---|
| charge 充电 | 0.66% | 2602 | 85（帧 7） |
| boot 开机 | 1.00% | 54764 | 1439（帧 56） |
| shutdown 关机 | 0.14% | 10636 | 235（帧 12） |
| ball 玩球 | 0.07% | 22632 | 223（帧 90） |
| battery 吃电池 | 0.35% | 47642 | 578（帧 134） |
| summon 召唤 | 0.18% | 9412 | 397（帧 44） |
| cube 玩魔方 | 0.34% | 77774 | 1107（帧 227） |
| **总体** | **0.39%** | **225462 / 88379334（0.26%）** | — |

「影子压底」这条修正对动作侧的影响极小：**battery 完全没变**（0 px），
**shutdown 全帧只多了 90 px**（10546 → 10636，约每帧 0.5 px），
其余动作没有旧渲染留档可比，但影子本身只在脚边，形状与位置都没动。

结构诊断（`diag_struct.py` 比较参考帧与 Lottie 的内容包围盒）：
ball / summon 全部帧吻合，其余动作 1~3 帧有 1~4 px 的轻微偏移。

### 播放期一致性（subframe 回归）

只比整数帧是不够的：lottie-web 默认按**小数帧**求值，而 SWF 是阶跃的。
因此额外做了一项回归 —— 渲染 `第 g 帧` 与 `第 g+0.5 帧`，比对两者的 SVG DOM
（`preview/_verify_half_dom.html`）。DOM 一致 == 画面一致 == 播放时不会有插值涂抹。

| | 修复前 | 修复后 |
|---|---|---|
| 7 个动作（1157 帧） | 1073 帧不一致 | **0** |
| 8 个走路方向（104 帧） | 99 帧不一致 | **0** |

修复前的典型差异（就是肉眼看到的闪动）：

| 资产 | 整数帧 | 半帧（插值中间态） |
|---|---|---|
| 走路·down | `matrix(1,0,0,1,14.82,59.80)` | `matrix(0.9967,0.0766,-0.0766,0.9967,…)`（旋转被插值 4.4°） |
| shutdown | `matrix(0.9765,-0.2125,0.2125,0.9765,…)` | `matrix(0.9912,-0.1555,0.1545,0.9848,…)`（旋转被插值 3°） |
| ball | `matrix(1,0,0,1,148.6,297.6)` | `matrix(1.0128,0,0,1.0128,…)`（缩放被插值） |
| charge | 图层 `display:none` | 图层 `display:block` + `opacity 0.07`（显隐被插值） |

## 已知偏差

最差帧的差异构成（`cmp_frames.py`，红 = 参考有墨而 Lottie 没有，
青 = Lottie 有墨而参考没有，黄 = 两边都有墨但颜色不同）：

| 动作 / 帧 | 总差异 | 参考独有 | Lottie 独有 | 颜色不同 |
|---|---|---|---|---|
| boot 56 | 1686 | **807** | 3 | 876 |
| cube 231 | 1098 | 190 | 164 | 744 |
| battery 135 | 497 | 109 | 67 | 321 |
| shutdown 104 | 377 | 192 | 4 | 181 |
| ball 119 | 195 | — | — | — |
| summon 44 | 397 | 1 | 268 | 128 |
| charge 0 | 44 | 10 | 1 | 33 |

| 项 | 说明 |
|---|---|
| **辉光是近似，不是真滤镜** | Lottie 没有 glow/blur 滤镜，用一组同心描边按 `(1-d/R)^decay` 的剖面叠出光晕 —— 视觉上是**一圈圈硬边描边**，不是连续的柔化衰减。参数逐像素标定过（`GLOW_CFG`）。 |
| **辉光覆盖半径偏小** | 开机最差帧 1686 px 里 **807 px 是「参考有墨、Lottie 没有」**（红色区域沿轮廓外圈分布），参考的光晕往外铺得更宽更淡，本资产的同心描边覆盖不到那一圈。这是各动作差异的主要来源。 |
| **吃电池 / 玩魔方完全没做辉光近似** | 实测加上后差异反而变大（吃电池 3289→4031、玩魔方 6175→6545），体积却翻倍（1.5→6.2 MB、4.6→9.5 MB）。代价是**小球缺了一圈柔光晕**，电弧也显得偏细偏硬。 |
| **召唤的辉光略强** | 召唤最差帧里「Lottie 有墨而参考没有」36 px、「参考有墨而 Lottie 没有」仅 3 px —— 本资产的辉光比参考多铺了一点。 |
| **电弧偏细** | 吃电池、玩魔方的青色电弧是逐段直线拼接，Flash 用的是带辉光的曲线，边缘更糊。 |
| **轮廓抗锯齿差异** | 剩余「颜色不同」的像素大多落在形状边缘 1~2 px 上，是渲染器差异而非几何差异。 |

## 重新生成

需要 JDK 11+ 与 FFDec（JPEXS Free Flash Decompiler）。

```bash
# 1) 导出动作 SWF 的 XML 与 shape SVG（一次性）
cd _work/tools
java -Djava.awt.headless=true -Djava.io.tmpdir="$PWD/../../_work/tmp" \
     -jar ffdec/ffdec-cli.jar -swf2xml ../../_work/xml/action "../../NONO形态与动作_20261021/资源/nono/super/action"
java -Djava.awt.headless=true -Djava.io.tmpdir="$PWD/../../_work/tmp" \
     -jar ffdec/ffdec-cli.jar -export shape ../../_work/svgshape/action "../../NONO形态与动作_20261021/资源/nono/super/action"

# 2) 生成 7 个动作的 Lottie + manifest
cd ../..
_work/venv/bin/python _work/tools/swf2lottie_action.py
_work/venv/bin/python _work/tools/build_manifest.py   # 同时更新走路与动作两个 manifest
```

常用开关：

```bash
--only 3                 # 只生成一个动作（注意：会把 meta.json 覆盖成只含该动作，收尾前必须全量跑一遍）
--ratio-mode ignore      # charRatio 处理口径，默认 ignore（scale/frame 是实验口径）
--glow-cfg '{"2":{"steps":6,"peak":0.35,"rmult":1.2,"decay":2.2}}'   # 覆盖某动作的辉光参数
```

## 验证

```bash
# 生成 FFDec 参考帧（后台顺序跑，避免高负载）
_work/venv/bin/python _work/tools/ref_action_frames.py            # 全部
_work/venv/bin/python _work/tools/ref_action_frames.py boot cube  # 只跑指定动作

# 浏览器渲染 → 落盘 → 与参考帧逐像素比对
_work/venv/bin/python _work/tools/cmp_all.py

# 单帧诊断（生成差异图）
_work/venv/bin/python _work/tools/cmp_frames.py boot 56

# 结构诊断（比较内容包围盒，定位结构性错位）
_work/venv/bin/python _work/tools/diag_struct.py
```

浏览器端对照页：`preview/actions.html`（动作预览）、
`preview/_verify_action.html`（逐像素验证）、`preview/_cmp_action.html`（差异图）。

> 参考帧必须用**透明底 + alpha 感知合成**后再比对（先合成到白底再比），
> 否则半透明部件会整体偏色。
