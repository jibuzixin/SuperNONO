# NONO 动画资产（Lottie）

超能 NONO 的动画资产，**矢量 Lottie**，不依赖位图。目前已提取 **形态 1 / 2 / 3 / 4**
（形态 5 未开工；三个转换脚本都已按 `--form N` 参数化，接上形态 5 只需补 XML/SVG 导出）。

## 形态 1（第一个形态）

形态 1（超能 NONO 第一个形态）的全部动画资产。每个子目录放在同一套摆放约定下，
可以原地互换：

| 目录 | 内容 | 规格 |
|---|---|---|
| [`nono-form1-walk/`](nono-form1-walk/) | 八方向循环走路（down / leftdown / left / leftup / up / rightup / right / rightdown） | 各 ~56 KB，66×96，13 帧 @25fps，合计 456 KB |
| [`nono-form1-actions/`](nono-form1-actions/) | 7 个动作（充电 / 开机 / 关机 / 玩球 / 吃电池 / 召唤 / 玩魔方） | 34 KB ~ 4.6 MB，合计 12 MB |
| [`nono-form1-expressions/`](nono-form1-expressions/) | 4 个表情（开心 / 生气 / 惊讶 / 悲哀） | 200 KB ~ 302 KB，67×96 ~ 188×163，合计 1.04 MB |

## 形态 2（第二个形态）

| 目录 | 内容 | 规格 |
|---|---|---|
| [`nono-form2-walk/`](nono-form2-walk/) | 八方向循环走路 | 各 ~120 KB，76×83，**19 帧** @25fps，合计 0.95 MB |
| [`nono-form2-actions/`](nono-form2-actions/) | 7 个动作 | 18 KB ~ 4.9 MB，合计 10.3 MB |
| [`nono-form2-expressions/`](nono-form2-expressions/) | 4 个表情 | 206 KB ~ 678 KB，合计 1.5 MB |

> 形态 2 与形态 1 的 **SWF 结构完全不同**（时间轴条数 3 vs 5、走路周期 13 vs 19 帧、
> 大量使用 `PlaceObject3`），转换脚本已全部参数化，pet / 时间轴 / 周期 / 取景**自动检测**。
> 形态 2 走路资产的三个关键修复（PO3、嵌套方向时间轴、RGB 色彩变换）见
> [`nono-form2-walk/README.md`](nono-form2-walk/README.md)。

## 形态 3（第三个形态）

| 目录 | 内容 | 规格 |
|---|---|---|
| [`nono-form3-walk/`](nono-form3-walk/) | 八方向循环走路 | 各 65~140 KB，77×80，**16 帧** @25fps，合计 1.0 MB |
| [`nono-form3-actions/`](nono-form3-actions/) | **6 个动作**（**没有充电**） | 73 KB ~ 2.9 MB，合计 6.4 MB |
| [`nono-form3-expressions/`](nono-form3-expressions/) | 4 个表情 | 376 KB ~ 490 KB，合计 1.7 MB |

> 形态 3 的**原资源没有充电动作**：`super/action/` 里只有 `2_3` ~ `7_3`，
> **没有 `1_3.swf`**（形态 4 / 5 同样没有 `1_4` / `1_5`）。
> 这是**资源本身的缺口**，如实缺席，没有用别的动作顶替。
>
> 形态 3 的 SWF 结构又不一样：pet=124、5 条顶层方向时间轴、**0 条嵌套**方向时间轴、
> 走路周期 16 帧、`PlaceObject3` 有 438 个。形态 3 转换时新踩的四个坑
> （SWF 斜切丢失、`PlaceObject2` 的 `colorTransform` 继承、嵌套 clip 的 `place_frame` 继承、
> 左上/右上两条青色鳍的**开合不同步**）见 [`nono-form3-walk/README.md`](nono-form3-walk/README.md)。
>
> **表情名字的单一事实来源是配置表**
> `NONO形态与动作_20261021/配置表/NoNo配置表_MachXMLInfo.xml`：
> `<superExp>` 是全部超能形态的默认名字，**`<superExpStage>` 是形态专属覆盖**
> （各形态的表情画面各不相同，名字按画面定、逐形态确认，没说的不改）。
> 客户端 `MachXMLInfo.getExpName()` 已改成「先查 stage 覆盖，再回落默认」，
> 我们的资产命名也直接读这份配置表，三边一致。
>
> 目前只有**形态 3** 做了覆盖（`1_3` 画的是「头晕」、`3_3` 画的是「玩乐」、
> `4_3` 画的是「开心」），形态 1 / 2 / 4 / 5 沿用默认名字：
>
> | 形态 | id | 源文件 | slug | 名称 |
> |---|---|---|---|---|
> | 3 | 1 | `exp/1_3.swf` | `dizzy` | 头晕 |
> | 3 | 2 | `exp/2_3.swf` | `angry` | 生气 |
> | 3 | 3 | `exp/3_3.swf` | `play` | 玩乐 |
> | 3 | 4 | `exp/4_3.swf` | `happy` | 开心 |
>
> **id 与源文件编号永远不动** —— 客户端按 `exp/<id>_<stage>.swf` 寻址。

## 形态 4（第四个形态）

| 目录 | 内容 | 规格 |
|---|---|---|
| [`nono-form4-walk/`](nono-form4-walk/) | 八方向循环走路 | 各 168~263 KB，**189×104**，**21 帧** @25fps，合计 1.65 MB |
| [`nono-form4-actions/`](nono-form4-actions/) | **6 个动作**（**没有充电**） | 1.01 MB ~ 4.68 MB，合计 15.5 MB |
| [`nono-form4-expressions/`](nono-form4-expressions/) | 4 个表情 | 242 KB ~ 301 KB，合计 1.06 MB |

> 形态 4 与形态 3 的**方向时间轴结构完全一致**（5 条顶层、各 63 帧、
> 方向标签 down=0 / leftdown=7 / left=15 / leftup=23 / up=31 / rightup=39 / right=47 / rightdown=55），
> 但 **pet=152**、走路周期 **21 帧**（形态 3 是 16）、画布 **189×104**（形态 3 是 77×80）。
> 形态 4 的原资源同样**没有充电动作**（`action/1_4.swf` 缺失）。
>
> 形态 4 的三个特有现象（**都不是转换缺陷**）：
> `left` 与 `leftdown` 画面完全相同、`right` 与 `rightdown` 画面完全相同（源资源如此）；
> 4 个方向的斜切要逐帧拆层（层数 207~211 vs 另 4 个方向的 56，**拆层反而更准**）；
> 资源带 `GLOWFILTER`（走路 168 / 表情 220 / 动作 766 个）而 Lottie 没有 glow 滤镜。
> 详见 [`nono-form4-walk/README.md`](nono-form4-walk/README.md)。
>
> ### 🩹 变形形状（morph shape）：表情头顶那两只青色耳朵
>
> 形态 4 的 4 个表情、以及「玩球 / 吃电池 / 召唤 / 玩魔方」这 4 个动作里，
> 角色身上那对青色鳍是 **`DefineMorphShapeTag`（变形形状）**。
> **FFDec 的 `-export shape` 不导出变形形状** ⇒ 早先这些形状**整个没被导出**，
> 表情头顶的耳朵**彻底消失**（只有用普通形状兜底的那一帧还会闪一下）。
> 已用 `-export morphshape` 单独导出、按 **PlaceObject 的 `ratio/65536`** 取形变进度、
> Lottie 侧逐帧拆层实现（见 [`../开发注意事项.md`](../开发注意事项.md) 铁律 27）。
>
> 修完之后：表情逐像素 **2.09% → 1.92%**、动作 **0.33% → 0.30%**（都是**变好**，
> 因为补回来的本来就是原图的一部分）；形态 3 的「开机/关机」也一并修好（**0.56% → 0.25%**）。

### ⭐ 走路的真实播放语义（**四个形态通用**，别按整段循环去理解）

读客户端源码 `NonoModel.as` 的结论：

```actionscript
_bodyUpColorMC.gotoAndStop(direction);   // 定位到该方向的段首帧后停住
private function onWalkStart(e:Event):void { if(_loc2_.currentFrame == 1) _loc2_.gotoAndStop(2); }
private function onWalkEnd(e:Event):void   { if(_loc2_.currentFrame == 2) _loc2_.gotoAndStop(1); }
```

- 客户端**只定位到方向段的段首帧**，并不连续播放整条时间轴；
- 走路的「动」是 **第 1 帧 ⇄ 第 2 帧** 两个姿态的切换，由 `WALK_START` / `WALK_END` 驱动；
- `currentFrame == 1` 只在 **down 方向**成立（其余方向的 `currentFrame` 是 8/16/24/…，
  两个回调对它们是 no-op）。

⇒ **客户端真正会显示的，只有 down 的第 1、2 帧与其它 7 个方向的段首帧。**

我们交付的走路资产是「把该方向段连续播 N 帧」的循环（四个形态都如此），
覆盖了客户端不会显示的画面。这不影响资产可用性，但**看逐像素数字时要分两种口径**：

| 形态 | 整段循环（21 / 16 / 19 / 13 帧） | 只看客户端实际显示的画面 |
|---|---|---|
| 形态 3 | 0.87% | **0.99%** |
| 形态 4 | 1.50% | **1.12%** |

形态 4 的 1.50% 是被那 19 个「客户端不会显示的帧」拉高的，
**同口径下（1.12% vs 0.99%）形态 4 与形态 3 处在同一水平**。

### 表情名字（逐形态确认）

各形态的表情画面互不相同，名字要**逐形态确认**：

| 形态 | id | 源文件 | slug | 名称 |
|---|---|---|---|---|
| 3 | 1 | `exp/1_3.swf` | `dizzy` | 头晕 |
| 3 | 2 | `exp/2_3.swf` | `angry` | 生气 |
| 3 | 3 | `exp/3_3.swf` | `play` | 玩乐 |
| 3 | 4 | `exp/4_3.swf` | `happy` | 开心 |
| 4 | 1 | `exp/1_4.swf` | `smug` | 得意 |
| 4 | 2 | `exp/2_4.swf` | `angry` | 生气 |
| 4 | 3 | `exp/3_4.swf` | `dizzy` | 头晕 |
| 4 | 4 | `exp/4_4.swf` | `play` | 玩乐 |

形态 1 / 2 / 5 沿用默认名字。形态 4 的画面核对图见
[`../_work/evidence/f4_exp_strips.png`](../_work/evidence/f4_exp_strips.png)
（每个表情 6 帧）与
[`../_work/evidence/f4_exp_names.png`](../_work/evidence/f4_exp_names.png)。
形态 4 的名字已按画面定好并写进配置表 `<superExpStage>/<stage id="4">`；
改动顺序是「改配置表 → 重跑 `swf2lottie_exp.py --form 4` → 重跑
`build_manifest.py --form 4` → 重渲参考帧 → 重做逐像素比对」。

## ⭐ 语义标签（`nono` 字段）· 染色 / 以后扩展用

Lottie 的 JSON 允许**自定义字段**（lottie-web 会忽略未知字段，渲染完全不受影响）。
我们用统一的自定义字段 `nono` 把「这一块是什么」写进素材，
这样运行时**不用猜颜色**，直接按标签换色即可。

> **当前状态：形态 1 / 2 / 3 全部 56 个资产已打标**（走路 + 动作 + 表情），
> 标签来源是**人工标注优先**（`_work/labels/form1.json` 325 条 / `form2.json` 110 条 /
> `form3.json` 202 条），自动规则只兜底。形态 4 / 5 未打标。
> 形态 4 / 5 的资产里**没有 `nono` 字段** —— 整个资产按「不染色」看待，不会出错；
> 要染色得先补标注（形态 4 还要额外处理 GLOW 滤镜）。

### 标签长什么样

```json
"nono": {
  "role": "body",          // 语义角色（见下面的角色表）
  "tags": ["dyeable"],     // 自由标签数组 —— 以后加别的用途往这里塞字符串
  "dye": "solid"          // 染色方式：solid(实色) / shade(阴影)；**没有这个键 = 不染色**
}
```

写在三个层级，权威程度从高到低：

| 位置 | 放什么 | 作用 |
|---|---|---|
| **fill (`fl`)** | `role` + `dye` | **换色就在这里做** —— 真正带颜色的那个对象 |
| **group (`gr`)** | `role` | 按组查询时用（例如「这个组是耳根」） |
| **layer** | `role` + `shape`（源 SWF 的 character id）+ `glow`（**人工标了 GLOW 时**才有） | 按图层查询时用 |

顶层还有一个 `nono` 段，记录 `tagVersion` / `form` / `asset` / `key`、
**角色表本身**（`roles`）、以及 `glowShapes`（源 SWF 的辉光参数，**只读参考**）——
运行时不用硬编码角色含义。

### 角色表（可扩展）

| role | dye | 含义 |
|---|---|---|
| `body` | `solid` | 本体（白色）—— 要染成目标色 |
| `ear_root` | `solid` | 耳根（青色尖耳根部那一小段白）—— 同样要染 |
| `body_shade` | `shade` | 本体灰色阴影 —— 染成目标色的**深暗版** |
| `accent` | — | 青色耳朵 / 鳍 / 触角 —— **不染色** |
| `dark` | — | 黑色面罩 / 眼睛 / 投影 —— 不染色 |
| `eye` | — | 眼睛（瞳孔 / 笑眼）—— **不染色**（白色眼睛也要保持白色） |
| `prop` | — | 道具（球 / 电池 / 魔方）—— **不染色** |
| `effect` | — | 白色闪光 / 特效 / 开机漩涡 —— **不染色** |
| `glow` | — | **GLOW 部件** —— 程序自己画光晕，**不染色**（见下面「GLOW 辉光」） |
| `other` | — | 其它颜色 —— 不染色 |

> 加新角色只要在 `_work/tools/nono_tags.py` 的 `ROLES` 里加一条，
> 再在 `FORM_RULES` 里给它派活；运行时逻辑不用改。

### 染色规则（`dye`）

- `solid` —— 填充色直接换成目标色
- `shade` —— 填充色换成目标色的深暗版（倍率由运行时决定，素材不管具体色值）
- **没有 `dye` 键 = 永远不染**（道具、白色特效、黑色面罩、青色耳朵）

标签**跟着图层走**：任意姿势、旋转精灵、透明度动画都不影响，
因为判断依据是「这个 fill 属于哪一类」，不是「它现在长什么样」。

### 哪些部分需要染色（染色清单）

**判定只看一个字段**：素材里每个带颜色的填充（`ty:"fl"`）上都有 `nono`：

| `nono.dye` | 含义 | 谁 |
|---|---|---|
| `"solid"` | **要染**（换成目标色） | 本体白色、耳朵根部那一小段白 |
| `"shade"` | **要染成深暗色**（目标色的暗版） | 本体的灰色阴影 |
| 没有这个键 | **永不染色** | 青色装饰、黑色面罩、眼睛、道具、白色闪光特效、其它杂色 |

> 白色本体和白色闪光在画面里**颜色一样**，但一个要染、一个不染 ——
> 所以判断必须看标签，不能按颜色猜。

**当前规模**（形态 1/2/3 共 56 个资产）：

| | 数量 |
|---|---|
| 要染（实色） | 708 个填充 |
| 要染（深暗） | 376 个填充 |
| 不染色 | 17467 个填充 |
| 其中颜色带关键帧的（换色时要保住动画） | 61 个 |

标签**跟着图层走**：同一个形状的多个实例带同一套标签，
所以「第几帧」「什么姿势」都不影响判定。

**想知道某个资产具体哪些地方会染**，两种办法：

1. 看素材 JSON：搜 `"dye"` 就能列出全部可染填充。
2. 肉眼看：打标台顶栏勾「标记可视化」——
   **洋红 = 可染实色、橙 = 可染阴影、灰 = 不染色**；
   或开 `preview/dye.html` 实时染色对比。

### 美术素材怎么用

| 问题 | 答案 |
|---|---|
| 素材在哪 | `assets/nono-form<N>-{walk,actions,expressions}/`，每个目录一个 JSON = 一个动画 |
| 清单在哪 | 同目录 `manifest.json`（文件名、帧数、图层数、画布尺寸、包围盒、sha256、配的音效）；`meta.json` 是体积汇总 |
| 音频怎么关联 | 动画 manifest 的 `audio` 段写了配哪个音效、何时播、播几次（音频本体在 `assets/nono-sounds/`） |
| 标签在哪看 | 每个资产顶层 `nono` 段：`tagVersion` / `form` / `asset` / `key` / **`roles`（角色含义对照表）** |
| 角色含义 | `doc.nono.roles` 里写好了每个角色的中文含义和是否染色，**不用问人** |
| lottie-web 会不会被这些字段影响 | 不会。`nono` 是自定义字段，lottie-web 直接忽略，素材不染色时和普通 Lottie 完全一样 |
| 形态 4/5 | **还没打标**，素材里没有 `nono` 段 ⇒ 整段视为「不染色」，不会出错；要染色得先补标注 |
| 改完标注要做什么 | 重跑 `tag_assets.py --form N`（写回资产）→ `tag_verify.py --form N`（校验）→ `build_manifest.py --form N`（刷新清单里的 sha256/字节数） |

### GLOW 辉光（程序自己画）

Lottie 没有 glow 滤镜，所以带 `GLOWFILTER` 的资源**画不出原样的辉光**
（形态 4 走路 168 / 表情 220 / 动作 766 个滤镜）。
**辉光由程序自己画**，所以要**人工标出哪些部件要辉光** —— 打标台里的 `GLOW` 角色（快捷键 `0`）。

标完之后，该图层上才会出现：

```json
"nono": { "role": "glow", "shape": "106",
           "glow": {"color": [50,254,254], "alpha": 255,
                    "blur": 7.0, "blurY": 7.0, "strength": 1.0,
                    "inner": false, "compositeSource": true, "passes": 3,
                    "baked": false, "pulse": true, "manual": true} }
```

| 字段 | 含义 |
|---|---|
| `color` / `alpha` | 辉光颜色（源 SWF 的 `glowColor`） |
| `blur` / `blurY` | 滤镜半径的**峰值**（源里是脉动的，见 `pulse`） |
| `strength` | 辉光强度峰值 |
| `inner` | 是否内发光 |
| `compositeSource` | 源滤镜的 `compositeSource` 开关 |
| `passes` | 源滤镜的 `passes` |
| `baked` | **true = 这个 Lottie 资产里已经用同心描边把辉光画出来了**（转换脚本的近似机），程序**不要**再叠一层；false = 资产里没有，需要程序自己渲染 |
| `pulse` | true = 源里的辉光是**脉动**的（strength 随帧起落，blur 与它同步缩放），而且强度与**该图层的不透明度严格成正比** —— 程序用 `strength × 当前层不透明度` 就能还原脉动 |
| `manual` | true = 这一条是**人工标**的 GLOW（不是自动推出来的） |

> **有 `nono.glow` = 程序要在这里画辉光**；
> **没有 `nono.glow` = 永远不画**。
> 自动从 SWF 滤镜推出来的辉光**不再写进图层** —— 否则每个带滤镜的部件都会亮，
> 程序就没法按人的意思挑了。

#### 源里的辉光参数去哪了：`nono.glowShapes`

顶层 `nono.glowShapes` 存着**源 SWF 里带 GLOWFILTER 的形状**及其参数：

```json
"nono": { "glowShapes": { "106": {"color": [50,254,254], "blur": 7.0, ...} } }
```

这是**只读参考**，不是画辉光的指令。用处有两个：

1. 打标台据此提示「这一段源里带辉光」，人工照着一个个标（`GLOW` 快捷键 `0`）；
2. 人工标了 GLOW 时，**直接继承源里的真实参数**（颜色 / 半径 / 强度），
   而不是用默认值 —— 源里这一段没有 GLOWFILTER 时才退回默认值。

运行时可以自己按 `nono.glow` 的参数实现辉光（描边叠加 / 滤镜合成），**不用回头去解析 SWF**。

> **两条坑**：
> 1. 源 SWF 里 `strength=0` 的辉光**是看不见的**（作者拿它当占位，只把 blur 递增）。
>    这类**不会**被记进 `glowShapes` —— 标了反而会让程序画出一圈不该有的光晕。
> 2. `baked` 一定要看：开机 / 召唤的辉光**已经烘焙在资产里**了，
>    程序若再叠一层就会变成双重辉光（比原图更亮）。

### 怎么看效果

```bash
python3 preview/server.py 8903
# http://127.0.0.1:8903/preview/dye.html?form=2
```

左栏选资产，中间实时染色，右栏可以调目标色 / 阴影倍率，
勾「标记可视化」会画成 **品红 = 可染色、橙 = 灰色阴影、其余 = 不染色**。

### 重新打标

```bash
# 标签是**后处理**：转换脚本不动，标签单独一层，重跑转换后再跑一遍即可
_work/venv/bin/python _work/tools/tag_assets.py --form 2          # 打标
_work/venv/bin/python _work/tools/tag_assets.py --form 2 --dry    # 只报告不改
_work/venv/bin/python _work/tools/tag_verify.py --form 2         # 自动校验（漏染/多染）
_work/venv/bin/python _work/tools/tag_preview.py --form 2         # 染色效果总表
_work/venv/bin/python _work/tools/build_manifest.py --form 2      # 重建清单（sha256 会变）
```

`tag_assets.py` 是**幂等**的：重复跑不会叠加，只会把 `nono` 字段重写一遍。

### 人工打标（最高优先级，覆盖自动规则）

自动规则靠颜色猜语义，会猜错。最典型的一次误判：**魔方的魔法火焰**
（青 + 橙 + 黄 + 奶白 + 纯白的多彩 shape）被规则当成「青色触角旁边那段白 = 耳根」，
结果整团火焰的白色被染成橙色。所以：**凡是多彩的 shape，白色多半是特效，不是本体。**

为此做了一个**网页打标台**，人工逐帧、逐部件打标，标注落盘后由脚本写进资产：

```bash
# 带 API 的服务（打标台要用它读写标注）
nohup python3 preview/server.py 8931 > _work/label_server.log 2>&1 &

# 打标台：手点画面上的部件 → 选角色 → 保存
# http://127.0.0.1:8931/preview/label.html
# 染色验收（深链可直接甩某一帧）
# http://127.0.0.1:8931/preview/dye.html?form=2&kind=actions&asset=cube&frame=190&zoom=fit
```

- 标注落在 `_work/labels/form<N>.json`，`tag_assets.py --form N` 读它并写进资产。
- **人工标注优先于自动规则**（`nono_tags.match_manual`）。
- **`layer` 锚点是精确到图层实例的**，对不上就整条不生效，**不会退回按 shape 匹配**。
  打标台那边用同一套规则 —— 两边预览的染色结论必须一致。
- ⚠️ **辉光层 / 形变层 / 模糊层没有填充**，染色只发生在填充上，
  所以标在这些层身上等于没标（写回时会被忽略）。
  要给「某个部件」定身份，得标到**同形状的实体层**（`shapeN`）上 ——
  打标台现在会自动帮你落到实体层；存量标注用 `_work/tools/fix_glow_annos.py` 迁移。
- 标注粒度 = `图层序号`（可选 `fill` 序号）+ `shape id` + 可选 `ip/op` 帧区间。
  用**图层序号**而不是 shape id 当锚点，因为同一个 shape id 在一个资产里会有
  多个图层实例，甚至有内容完全一致的孪生层 —— `(shape, ip, op)` 三元组并不唯一。
- 三种生效范围：**本图层实例** / **同 shape 全部** / **同 shape 指定帧区间**
  （区间型只对「可见期整体落在该区间内」的图层实例生效）。
- 打标台里还能开「染色预览」实时看效果；「标记可视化」用颜色标出每个角色的归属。

配套工具：

```bash
_work/venv/bin/python _work/tools/suspect.py --form 2   # 扫「多彩 shape 里的白填充」等疑似误标
_work/venv/bin/python _work/tools/white_layers.py 2     # 列出各资产的白/近白填充层清单
_work/venv/bin/python _work/tools/test_tags.py          # 标注匹配规则的单测
```

> `suspect.py` 用的是「同一个 shape 里出现 ≥3 种颜色族」当判据 ——
> 正本体只有白/灰/黑两三种，火焰、魔法阵、碎片、爆炸这类特效必然是多彩的。
> （别用「不透明度是动画的」当判据：身体各部件也全是动画不透明度，噪声太大。）

### 图层顺序：「特效被本体压住 / 道具该沉到本体下面」怎么修

Lottie 的 `layers` 数组是**上层在前**：`layers[0]` 画在最上面。所以

- 「提到最上层」= 把目标层挪到数组**最前面**；
- 「沉到最底层（本体之下）」= 把目标层挪到数组**最后面**。

形态 2 的「玩魔方」踩过三次坑，方向各不相同：

1. 魔方（`shape94`）排在**黑色月牙（`shape177`）** 和 **两个灰色椭圆（`shape182`/`shape183`）**
   之下，魔方被压掉一块（148/149、156/157、164/165、172/173、180/181、189/190 这几帧异常）。
   当时误以为是「魔法层」，把 `shape94` 提到了最上层 —— 方向没错，但**粒度太粗**。
2. 有人以为「魔方是背对镜头的 ⇒ 整段动画都该沉到身体下面」，用 `raise_layers.py --bottom`
   把 7 个魔方实例全沉到最底层 —— **错**：只有那 12 帧才是背对镜头的，其余帧魔方照旧在上层。
3. 正解：**按帧切片**。那 12 帧里魔方沉到本体下面，其余帧留在原层序。

```bash
# 提到最上层（特效用）：保持彼此相对顺序，其余层相对顺序不变
_work/venv/bin/python _work/tools/raise_layers.py \
    --asset assets/nono-form2-actions/nono2_action_summon.json --shapes 199-209
# 也可按标签挑：--roles effect,prop
# --dry 先看会变成什么样

# 按帧沉到本体下面（道具用）：只有 --frames 列出的帧会沉，其余帧保持原位
_work/venv/bin/python _work/tools/frames_below.py \
    --asset assets/nono-form2-actions/nono2_action_cube.json --shapes 94 \
    --frames 148,149,156,157,164,165,172,173,180,181,189,190

# 按帧沉到某个层「下面」（不是沉到本体）：形态3 的魔方在球出现的那段要被球挡住
_work/venv/bin/python _work/tools/frames_below.py \
    --asset assets/nono-form3-actions/nono3_action_cube.json --shapes 63 \
    --frames 122-190 --below-shape 50
```

`frames_below.py` 的做法：把命中的图层按帧区间**切片**——
落在指定帧里的片段复制一份挪到数组**末尾**（本体之下），其余片段留在原来的层序位置。
切片后每段都是一层，只是 `ip`/`op` 限定了它出现的帧区间，渲染效果等价于「这几帧换了个 z 序」。
加 `--below-shape N` 时，片段插到 **`shapeN` 那一层的紧下方**（而不是数组末尾）。

> ⚠️ **别整段动画一起改**：层序是**整段生效**的，改一个实例 = 改它出现的那一整段时间。
> 要「只有某几帧特殊」，就必须按帧切片，否则会把不相干的帧一起改掉。

改完层序后**一定要**看图确认道具没被整个埋掉：

- 形态 2：魔方在这 12 帧沉到本体之下（148/156/190 帧实测：魔方只从头顶露出一点点），
  其余帧（147/150 帧）魔方照旧在前面。
- 形态 3：魔方在 **116~190 帧**沉到**球的碎片和球**下面 —— 116~121 帧是球的碎片阶段
  （碎片盖住魔方），122 帧起球成型把魔方整个挡住（原 SWF 参考帧就是这样，改完才对齐）；
  113 帧（窗口之前）魔方照旧在前面 ✓。

提层/沉层/切片会改**图层序号**，所以：

- 工具会自动把 `_work/labels/form<N>.json` 里**按图层序号锚定**的标注同步到新序号；
- **克隆出来的层会自动继承源层的标注**（不然新层会丢掉染色标签）；
- 更稳的做法是把标注**降级成 shape 级**（同一个 shape 的所有实例角色一致时）；
- 提层后**必须**重跑 `tag_assets.py --form N` + `tag_verify.py --form N` 复核。

查「谁盖住了谁」用浏览器里的层序检查页（离线渲染器只画 fill 不画描边，看不准）：

```
http://127.0.0.1:8931/preview/zorder.html?form=2&kind=actions&asset=cube&frames=148,156,164
```

它对每帧逐层在路径上采样，输出「层 A 被层 B 盖住了百分之多少」。

### 单层探针（定位某个图层到底画了什么）

离线渲染器（`layer_raster.py`）**只画 fill、不画描边**，所以描边类的特效它看不出来。
要确认「某一层画的是什么东西」，用浏览器里的单层探针：

```
http://127.0.0.1:8931/preview/layergrid.html?frame=190&layers=305,313,380
```

一次把若干个图层各自单独渲染成一格，一眼就能看出哪个是火焰、哪个是魔法阵、哪个是耳朵。


## 音效

音效**单独放在** [`nono-sounds/`](nono-sounds/)，不跟动画混在一起；
关联关系写在**两个方向**的清单里：

- **动画 manifest** —— 每个动画条目带一个 `audio` 段，直接写明「配哪个音效、什么时候播、播几次、怎么播」：
  ```json
  "audio": {
    "soundId": "action_boot",
    "file": "nono-sounds/sounds/action_boot.mp3",
    "status": "ok",
    "trigger": "动作开始播放时（与动画 goToAndPlay(0) 同时）",
    "loops": 1,
    "stop": "下一个动作/表情开始前停止（clearActionMC() 里 _sc.stop()）",
    "playAs3": "_sc = _sound.play(0, 1);",
    "playWeb": "const a = new Audio(url); a.loop = false; a.play();"
  }
  ```
- **音效 manifest** —— `assets/nono-sounds/manifest.json` 的 `bindings[]` 反向列出
  「这个动画 → 那个音效 + 播放规则」，`sounds[]` 列出每个 MP3 的时长/采样率/sha256。

两边的数据来自**同一个数据源** `_work/tools/audio_spec.py`，不会写岔。
形态 2 的动作音效文件名带 `_s2` 后缀（`action_boot_s2.mp3` 等）。

> **哪些没有音效**：充电、4 个表情、八方向走路在**原资源里就没有**声音
> （SWF 里没有内嵌 `sou` 类），清单里标注为 `missing-from-source` / `none`，
> **没有编造占位音频**。详见 [`nono-sounds/README.md`](nono-sounds/README.md)。

## 摆放约定（最重要的一条）

各动画的**宠物原点在画布内的位置 `petAnchor` 相同或略有偏移**（内容向左/向上溢出时
画布会相应扩展，偏移量记在 `petAnchor` 里）。**按 `petAnchor` 对齐后，所有动画里的
宠物在屏幕上完全重合**，动作与走路可以随时互换。

`petAnchor` **每个形态都不一样**（形态 1 是 `(32.70, 90.35)`，形态 2 是 `(37.02, 76.57)`，
形态 3 是 `(39.40, 71.99)`，形态 4 是 `(92.05, 94.02)`），
一律从对应目录的 `meta.json` 读，不要写死：

```js
// 桌面端：先按锚点对齐，再统一缩放（原始客户端用的是 0.8）
const WALK_ANCHOR = [37.02, 76.57];        // 走路的宠物原点在画布内的位置（形态 2）
el.style.left = `${WALK_ANCHOR[0] - action.petAnchor[0]}px`;
el.style.top  = `${WALK_ANCHOR[1] - action.petAnchor[1]}px`;
el.style.transform = `scale(${SCALE})`;   // SCALE = 0.8；transform-origin 设 0 0
```

- 缩放与落点在**客户端**做，资产保持与原 SWF **1:1**，不要在资产里预缩。
- **不要按画布居中**：各动作画布尺寸差异很大（66×96 ~ 877×480），居中会让宠物乱跳。
- 播放语义：**7 个动作一律 `loop: false`**（SWF 的 `gotoAndPlay` 只播一遍，播完停在末帧）。
  其中**充电（id=1）和关机（id=3）的 `autoEnd=0`** —— 客户端在这两个动作播完时
  **不派发 `PLAY_COMPLETE`、不切回走路**，动作会停在末帧保持占用状态；
  其余 5 个（开机/玩球/吃电池/召唤/玩魔方）`autoEnd=1`，播完派发完成事件并切回走路。
  4 个表情一律**播一遍即结束**（配置表 `<exp>` 段没有 `autoEnd`）。

## 预览

```bash
python3 preview/server.py 8903
# http://127.0.0.1:8903/preview/index.html        八方向走路（?form=2 看形态 2，?form=3 看形态 3，?form=4 看形态 4）
# http://127.0.0.1:8903/preview/actions.html      7 个动作（上方大舞台 + 下方七格同屏对比）
# http://127.0.0.1:8903/preview/expressions.html  4 个表情（播一遍即结束）
```

管线细节、验证方法与踩坑记录见根目录 [`开发注意事项.md`](../开发注意事项.md)。
