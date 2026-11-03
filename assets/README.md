# NONO 动画资产（Lottie）

超能 NONO 的动画资产，**矢量 Lottie**，不依赖位图。目前已提取 **形态 1 / 2 / 3**
（形态 4~5 未开工；三个转换脚本都已按 `--form N` 参数化，接上形态 4 只需补 XML/SVG 导出）。

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
形态 3 是 `(39.40, 71.99)`），
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
# http://127.0.0.1:8903/preview/index.html        八方向走路（?form=2 看形态 2，?form=3 看形态 3）
# http://127.0.0.1:8903/preview/actions.html      7 个动作（上方大舞台 + 下方七格同屏对比）
# http://127.0.0.1:8903/preview/expressions.html  4 个表情（播一遍即结束）
```

管线细节、验证方法与踩坑记录见根目录 [`开发注意事项.md`](../开发注意事项.md)。
