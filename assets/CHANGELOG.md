# 资产变更日志

## 2.1.0 — 2026-11-10

补全形态 1、形态 3 的 GLOW 标注。合并前逐文件对比确认：

- **几何零差异**：图层、形状、关键帧、透明度、时间轴、画布、`petAnchor` 全部不变；
- 仅新增/修改 `nono.glow`（少数既有条目参数微调），`nono.dye` 未动；
- 形态 1 逐层 glow：127 → 369（75 条 baked）
- 形态 3 逐层 glow：250 → 669（13 条 baked）
- `nono --check` 中形态 1/3 均为 **0 警告**；全资产剩余 11 条为形态 2/5 的既有问题
- A/B 渲染抽查形态 1 走路/魔方/生气、形态 3 走路/召唤/魔方，辉光增强但无过曝

## 2.0.1 — 2026-11-10

- 形态 2 开机：`form2/action/boot` 的 `offset_start` 从 `[-0.66, 1.17]` 调整为
  `[-0.66, 1.51]`。在默认 1.5 倍缩放下，开机首帧与关机末帧对齐；1.0 / 2.0 倍下
  误差约 0.5 个画布单位（约 0.5pt）。
- 这次只改 `tuning.json`；Lottie、manifest 与其它调参不变。

## 2.0.0 — 2026-11-10

形态 5 的四个表情此前是按「配置表名字」而不是画面语义命名，导致名字与实际动画不符。
这次从底层统一修正了 **slug / 文件名 / manifest / meta / Lottie 内部 `nono.key` / 音效 ID**，
不是只改显示名。

| 源 id | 旧名字 | 旧 slug / 文件 | 新名字 | 新 slug / 文件 |
|---|---|---|---|---|
| 1 | 开心 | `happy` / `nono5_exp_happy.json` | 悲哀 | `sad` / `nono5_exp_sad.json` |
| 2 | 生气 | `angry` / `nono5_exp_angry.json` | 生气（不变） | `angry` / `nono5_exp_angry.json` |
| 3 | 惊讶 | `surprise` / `nono5_exp_surprise.json` | 喜欢 | `love` / `nono5_exp_love.json` |
| 4 | 悲哀 | `sad` / `nono5_exp_sad.json` | 开心 | `happy` / `nono5_exp_happy.json` |

注意：这是 ID 级变更。外部如果按 `expression:5:happy` / `expression:5:surprise` 调用，
现在会得到修正后的语义（`happy` 是原 id4 的画面，`surprise` 已不存在，改为 `love`）。
动画内容、画布、帧数、锚点、audio 绑定关系均未改，只交换/修正了名字。

## 1.2.7 — 2026-11-10

- 形态 5 召唤的 `glow29`–`glow35`（7 层、每层 6 组 100–300 px 宽描边）在 CPU/ThorVG 上
  每帧约 0.3 s，实际播放只有约 3 fps，是漩涡卡的根因。
- 隐藏这些 baked 描边层，改在主形状 `shape29`–`shape35` 上写
  `nono.glow`（cyan / blur 20 / strength 1.0），由运行时高斯辉光重建。
- 修复后单帧约 0.01 s，25 fps 可稳定播放；代价是光晕比原 baked 描边略锐。

## 1.2.6 — 2026-11-10

- 形态 5 四个表情（happy / angry / surprise / sad）的 `petAnchor` 统一修正为 `(-2, +25)`：
  实测表情本体和地面阴影都比待机低 25 个画布单位；修正后首帧本体相对锚点回到
  `(-39.2)`，地面阴影回到 `(+1)`，与待机一致。
- 形态 5 召唤：`shape29`–`shape35` 及其 glow 层的透明度是逐帧硬切，导致第 30、38 帧
  整个漩涡几乎消失（非透明像素从约 50k 掉到 9k）。把每次“亮一帧立刻归零”改为
  3 帧淡出，空档帧恢复到约 49k 像素，漩涡连续。

## 1.2.5 — 2026-11-10

- 形态 2 召唤：`shape59`（数组第 174 层，`ind=102`，第 33–88 帧，每 6 帧闪一次的眼睛高光）
  填充从 `body/dye:solid` 改为 `other`，不再跟随调色盘染色。
- 形态 5 召唤：实测本体相对 `petAnchor` 偏左约 93、偏下约 144 画布单位；将 `petAnchor`
  从 `[159.6164, 59.95]` 修正为 `[65.1, 203.8]`，召唤结束切回待机时本体位置一致。
- 形态 5 开机：`tuning.json` 增加 `offset_start = [0, 4.2]`、`offset = [0, 9.2]`，
  开机首帧对齐关机末帧，末帧对齐待机（消除关机 → 开机的轻微位置跳变）。

本文件记录 `assets/` 下矢量 Lottie 资产的结构性变更。
版本号见 [`asset-version.json`](asset-version.json)。

## 1.2.0 — 2026-11-09

按 App 侧《GLOW_TAGGING_SPEC.md》重构辉光标注，让 `nono.glow` 与 `nono.glowShapes`
两条信息都能真正驱动运行时辉光，全形态统一。

### 变更

- **逐层 `nono.glow` 成为运行时唯一入口**：源 SWF 的 GLOW 解析结果落到对应图层上，
  每个有辉光的图层都带一份 `nono.glow`（`color` / `blur` / `strength` / `baked` 等）。
- **`nono.shape` 统一为源 SWF character id**：每个图层都写；此前只有被人工标过身份的
  层才有 `nono` 块，运行时兜底查表无从查起（形态 4/5 的 `nono.shape` 与
  `glowShapes` 是两套编号，命中率只有 8 / 2）。
- **顶层 `nono.glowShapes` 降级为源事实表**：按**该资产里真实存在的形状**过滤
  （同一 SWF 里其它时间轴用的形状不再混进来），每条都带 `baked`。
- **`baked` 语义判定收紧**：只有资产内确实存在同 sid 的 `glowN` / `glowmorphN`
  同心描边副本层（光晕已画进画面）才写 `baked:true`；运行时据此跳过，不会过曝。
- **形态 3 的源红色辉光直接写最终显示色** `[50,254,254]`，不再依赖运行时的纯红替换。

### 覆盖范围

- 形态 1–5（超能）、普通 NONO、至尊年费 NONO：共 122 个资产。
- 飞行器 1/2/3/4 与通用飞行器：共 40 个资产（同样补 `nono.shape` 与 `baked`，
  辉光参数逐条核对过源 XML，未改动数值）。
- 合计 162 个资产。

### 验证

- 辉光审计（复刻 App 侧 `nono --check` 的 glow audit 口径）：**警告 0 条**。
- 逐动画统计见 [`GLOW_STATS.md`](GLOW_STATS.md)。
- 改动面核验：148 个变更资产里，**除辉光契约字段（`nono.glow` / `nono.shape` /
  顶层 `glowShapes`）外没有任何差异** —— 几何、图层顺序、时间轴、`nono.roles`
  全部逐字段一致；`nono.dye` 染色节点数与取值**零变化**。
- manifest 仅 `sha256` / `bytes` 变化，几何字段零变化。

### 备注

三类容易误判的资产，均已实测核对：

1. **辉光全部已烘焙（8 个）**：形态 1 的 `boot` / `charge` / `shutdown` / `summon`，
   形态 4 的 `boot` / `shutdown` / `summon`，普通 NONO 的 `boot`。光晕已由转换器
   画进画面，运行时按 `baked` 跳过，**辉光 pass 数本来就是 0**，不是缺陷。
2. **只有人工辉光、顶层表为空（15 个）**：形态 1 的 8 个走路、形态 2 的 `shutdown`
   与 `happy`、形态 5 的 `ball` 与 4 个表情。这些动画源 SWF 里没有 `GLOWFILTER`，
   辉光来自人工标注，走**逐层入口**发光，不依赖顶层表。
3. **表与逐层都为空（34 个）**：29 个源 SWF 里确实没有 `GLOWFILTER`（形态 1 的
   `ball` 与 4 个表情、形态 3 的 `boot` / `shutdown`、普通 NONO 10 个、至尊年费 17 个）；
   另 5 个是普通 NONO 走路的 5 个方向 —— 源 SWF 有辉光，但该方向时间轴没有用到
   那些形状。**都不是漏标**。
