# 超能 NONO · 形态 1 · 表情动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/exp/{1..4}_1.swf` 直接转换而来的
**形态 1（超能 NONO 第一个形态）4 个表情动画**，矢量 Lottie 格式。

> 文件名里的 `_1` 是**形态编号**，不是变体。客户端按
> `exp/<表情ID>_<superStage>` 取资源（`NonoModel.getExpRes()`），
> 所以 `1_1` ~ `4_1` 就是形态 1 玩家能触发的全部 4 个表情。

配套资产：八方向循环走路在 [`../nono-form1-walk/`](../nono-form1-walk/)，
7 个动作在 [`../nono-form1-actions/`](../nono-form1-actions/)，
三者的取景与宠物锚点一致，可以原地互换。

## 文件

| 文件 | 表情 | id | 帧数 | 画布 | 图层 | 体积 |
|---|---|---|---|---|---|---|
| `nono1_exp_happy.json` | 开心 | 1 | 94 | 188×163 | 22 | 242 KB |
| `nono1_exp_angry.json` | 生气 | 2 | 46 | 92×106 | 60 | 297 KB |
| `nono1_exp_surprise.json` | 惊讶 | 3 | 74 | 73×99 | 49 | 200 KB |
| `nono1_exp_sad.json` | 悲哀 | 4 | 124 | 67×96 | 67 | 302 KB |
| `meta.json` | — | — | — | — | — | 尺寸与体积速查 |
| `manifest.json` | — | — | — | — | — | 字节数 / sha256 / 帧数 / 画布 / 图层 / 裁剪原点 / 宠物锚点 / 内容包围盒 / **音效规则** |

表情 ID 的中文名取自客户端配置表 `配置表/NoNo配置表_MachXMLInfo.xml` 的 `<exp>` 段：
`1 开心 / 2 生气 / 3 惊讶 / 4 悲哀`。

全部为**扁平化** Lottie（无 precomp），每个 shape 实例摊成根级 shape 层，
**25fps**，不依赖任何位图。合计约 **1.04 MB**。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: false,              // 表情播一遍就停
  autoplay: true,
  path: 'assets/nono-form1-expressions/nono1_exp_happy.json',
});
anim.addEventListener('complete', () => {
  // 对应客户端 onExpScript：表情播完的通知
});
```

- `w` / `h` 就是各表情的画布尺寸（见上表），坐标原点在**画布左上角**。
- 帧范围 `[ip, op)` = `[0, N)`；`fr` = 25，与原素材一致。
- 层序：`layers[0]` 是**最上层**（lottie-web 的绘制顺序）。
- **`loop` 设 `false`**：表情是单次播放，播完停在末帧。

### 与客户端的一致性

参照客户端 `客户端源码/NonoModel.as`：

| 客户端行为 | Lottie 侧对应 |
|---|---|
| `_expMC.scaleX = _expMC.scaleY = SCALE`（0.8） | **仍需乘 0.8**。本资产与原 SWF 同比例（根矩阵缩放实测 1.0），0.8 是客户端的显示缩放，没有烘焙进来。 |
| `_expMC.gotoAndPlay(1)` | `autoplay: true` + `loop: false` |
| `addFrameScript(totalFrames-1, onExpScript)` | `anim.addEventListener('complete', ...)`；表情播完停在末帧，与 SWF 行为一致 |
| `_expMC` 与 `_bodyMC` 互斥显示 | 表情与走路资产可直接替换，宠物锚点一致 |

配置表 `<exp>` 段**没有** `autoEnd` 字段 —— 表情一律「播一遍即结束」，
所以 4 个表情的 `loop` 都是 `false`。

## 音效：无

**所有 5 个形态的表情 SWF 里都没有内嵌音效**（没有 `sou` 类）。
客户端在 `onResExp()` / `onResFollowExp()` 里仍会调 `_sc = _sound.play(0, 1)`，
**声音需宿主另行注入**。

`manifest.json` 里每个表情的 `audio.status` 如实标为 `missing-from-source`，
`loops` 为 1（客户端写死播一次），**没有塞占位音频**。
音效资产目录见 [`../nono-sounds/`](../nono-sounds/)。

### 摆放位置：宠物锚点

和动作一样，每个表情的画布原点对齐到「宠物原点在画布里落在走路资产同一位置
`(32.70, 90.35)`」的地方；内容向左/向上溢出时画布相应扩展，扩展量记在 `petAnchor` 里。

```js
// 把表情画布摆到与走路资产同一个锚点位置
el.style.left = `${WALK_PET_ANCHOR.x - exp.petAnchor[0]}px`;
el.style.top  = `${WALK_PET_ANCHOR.y - exp.petAnchor[1]}px`;
```

表情的画布比动作小得多（67×96 ~ 188×163），`petAnchor` 也更接近走路的
`(32.70, 90.35)`；`happy` 因为四肢大幅张开，画布宽达 188 px。

## 转换要点

表情 SWF 的结构和动作**完全一样**（根 1 帧 → master sprite → 子 sprite / shape），
所以复用同一套转换核心（`_work/tools/swf2lottie_action.py`），只换路径和清单。

但有一处**表情独有、动作没有**的坑 —— **嵌套 clip 的帧号必须按「父级有效时间」推进**：

表情 SWF 的 master 下面挂的是 **1 帧的容器 sprite**，容器里才是真正有 N 帧的内容 sprite：

```
master (94 帧)
├── sprite7  (1 帧)  ← 容器
│   ├── sprite3 (94 帧)  ← 内容（眼睛/耳朵在动）
│   └── sprite6 (94 帧)
├── sprite15 (1 帧)  ← 容器
└── …
```

如果按「父级当前显示帧」推进子级（`child = (parent_local_frame - place_frame) % fc`），
1 帧容器会把内容永远钉在第 0 帧 —— 4 个表情就全都变成一张**不会动的底脸**
（实测逐像素差异：开心 20.3% / 生气 27.3% / 悲哀 9.5%）。

正确规则是 **子级的有效时间 = 父级的有效时间 − 自己的 place_frame，
一路往下传、中间层不参与截断**；显示帧 = 有效时间 % 自身帧数。
这与 FFDec(JPEXS) 的嵌套 clip 语义一致（详见仓库根目录 `开发注意事项.md` 铁律 10）。

其余要点（矩阵、charRatio、hold 关键帧、Slot 快照、形状叠放顺序……）与动作完全相同，
见 [`../nono-form1-actions/README.md`](../nono-form1-actions/README.md) 的「转换要点」。

## 地面阴影：永远压在身体下面（对原作的有意修正）

原作 SWF 把地面阴影（黑色扁椭圆，19.85×8.35）放在显示列表的**上层** ——
开心表情里影子的 depth 是 11，而身体在 depth 1/5，于是**角色落地时黑色椭圆会盖住脚**，
白色身体被糊成一大块黑。

转换时把影子图层（连同它自己的辉光/模糊层）整块挪到 Lottie 图层数组**末尾**
（Lottie 里 `layers[0]` 是最上层，末尾才是最底层），这样影子永远在身体下面。

影子识别刻意保守：单路径 + 纯黑实心 + 路径只用 M/Q/Z（光滑椭圆、无拐角）+
局部包围盒 15≤w≤26、5≤h≤13。29.9×11.7 的「眼睛」、锯齿状的怒气符号、
79×35 的悲哀黑影都会被这条规则排除。规则实现见
`_work/tools/swf2lottie_action.py` 的 `shadow_shape_ids()`。

这条修正对 4 个表情的影响：

| 表情 | 影子原来在第几层 | 现在 | 像素变化 |
|---|---|---|---|
| happy 开心 | 16 / 22 | 21（最底） | 落地帧的脚部被影子盖住 → 修好后身体正常露出 |
| angry 生气 | 54 / 60 | 59（最底） | 无变化（影子本来就没压到身体） |
| surprise 惊讶 | 43 / 49 | 48（最底） | 无变化 |
| sad 悲哀 | 52 / 67 | 66（最底） | 无变化 |

前后对比图见 [`../../preview/exp_shadow_fix.png`](../../preview/exp_shadow_fix.png)
（左：原 SWF 参考；中：改前；右：改后，含脚部 ×7 放大）。

## 逐像素验证

用 FFDec 直接渲染原 SWF 得到的参考帧，与浏览器里 lottie-web 渲染的结果逐像素比对
（阈值：色差 > 40），**338 帧全量**：

| 表情 | 平均差异 | 总差异 px | 最差单帧 |
|---|---|---|---|
| happy 开心 | 0.44% | 12670 | 330（帧 46） |
| angry 生气 | 1.13% | 5073 | 130（帧 24） |
| surprise 惊讶 | 0.81% | 4349 | 159（帧 5） |
| sad 悲哀 | 0.82% | 6512 | 195（帧 6） |
| **总体** | **0.80%** | **28604 / 4661494（0.61%）** | — |

与动作资产同一量级（动作总体 0.27% ~ 0.44%），剩余差异集中在形状边缘 1~2 px 的
轮廓抗锯齿。

> happy 的 0.38% → 0.44% 是**有意为之**：影子压到底之后，脚部不再被黑椭圆盖住，
> 因此和「影子在上」的原作参考帧在这一小块上必然不同。这部分差异全部落在脚部，
> 其余画面逐像素一致。

## 已知偏差

| 项 | 说明 |
|---|---|
| **轮廓抗锯齿差异** | 剩余差异大多落在形状边缘 1~2 px 上，是 SVG/Canvas 渲染器差异而非几何差异。 |
| **地面影子的叠放顺序（有意偏离原作）** | 影子被压到身体下面，开心表情的落地帧因此与原作 SWF 参考帧在脚部有差异（见上）。 |
| **表情没有做辉光近似** | 原 SWF 里表情的形状没有 glow/blur 滤镜，不需要近似。 |

## 重新生成

需要 JDK 11+ 与 FFDec（JPEXS Free Flash Decompiler）。

```bash
# 1) 导出表情 SWF 的 XML 与 shape SVG（一次性）
cd _work/tools
java -Djava.awt.headless=true -Djava.io.tmpdir="$PWD/../../_work/tmp" \
     -jar ffdec/ffdec-cli.jar -swf2xml ../../_work/xml/exp "../../NONO形态与动作_20261021/资源/nono/super/exp"
java -Djava.awt.headless=true -Djava.io.tmpdir="$PWD/../../_work/tmp" \
     -jar ffdec/ffdec-cli.jar -export shape ../../_work/svgshape/exp "../../NONO形态与动作_20261021/资源/nono/super/exp"

# 2) 生成 4 个表情的 Lottie + manifest
cd ../..
_work/venv/bin/python _work/tools/swf2lottie_exp.py
_work/venv/bin/python _work/tools/build_manifest.py   # 走路 / 动作 / 表情 三个 manifest
```

## 验证

```bash
# 生成 FFDec 参考帧（后台顺序跑，避免高负载）
_work/venv/bin/python _work/tools/ref_exp_frames.py            # 全部
_work/venv/bin/python _work/tools/ref_exp_frames.py happy sad  # 只跑指定表情

# 浏览器渲染 → 落盘 → 与参考帧逐像素比对
_work/venv/bin/python _work/tools/cmp_all.py ref_exp

# 单帧诊断（生成差异图）
_work/venv/bin/python _work/tools/cmp_frames.py happy 46
```

浏览器端对照页：`preview/expressions.html`（表情预览）。

> 参考帧必须用**透明底 + alpha 感知合成**后再比对（先合成到白底再比），
> 否则半透明部件会整体偏色。
