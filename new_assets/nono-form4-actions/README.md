# 超能 NONO · 形态 4 · 动作动画（Lottie 资产）

从原资源 `NONO形态与动作_20261021/资源/nono/super/action/{2..7}_4.swf` 直接转换而来的
**形态 4（超能 NONO 第四个形态）6 个动作动画**，矢量 Lottie 格式。

> ⚠️ **形态 4 的原资源没有充电动作**：`super/action/` 里只有 `2_4` ~ `7_4`，
> **没有 `1_4.swf`**（形态 3 / 5 同样没有 `1_3` / `1_5`）。
> 这是**资源本身的缺口**，如实缺席，没有用别的动作顶替。
>
> 客户端按 `action/<动作ID>_<superStage>` 取资源，所以形态 4 玩家能触发的
> 就是这 6 个动作。

## 文件

| 文件 | 动作 | id | 帧数 | 画布 | 图层 | 体积 | 来源 |
|---|---|---|---|---|---|---|---|
| `nono4_action_boot.json` | 开机 | 2 | 75 | 189×108 | 73 | 1.19 MB | `action/2_4.swf` |
| `nono4_action_shutdown.json` | 关机 | 3 | 75 | 189×104 | 73 | 1.20 MB | `action/3_4.swf` |
| `nono4_action_ball.json` | 玩球 | 4 | 300 | 743×444 | 820 | 3.98 MB | `action/4_4.swf` |
| `nono4_action_battery.json` | 吃电池 | 5 | 277 | 402×161 | 232 | 4.68 MB | `action/5_4.swf` |
| `nono4_action_summon.json` | 召唤 | 6 | 77 | 424×241 | 181 | 1.01 MB | `action/6_4.swf` |
| `nono4_action_cube.json` | 玩魔方 | 7 | 244 | 214×210 | 918 | 3.52 MB | `action/7_4.swf` |
| `meta.json` | — | — | — | — | — | — | 尺寸与体积速查 |
| `manifest.json` | — | — | — | — | — | — | 字节数 / sha256 / 帧数 / 画布 / 图层 / 裁剪原点 / 宠物锚点 / 内容包围盒 / 音效规则 |

全部为**扁平化** Lottie（无 precomp），**25fps**，不依赖任何位图。合计约 **14.3 MB**。

## 接入

```js
const anim = lottie.loadAnimation({
  container: el,
  renderer: 'svg',          // 或 'canvas'
  loop: false,              // 动作播一遍即结束
  autoplay: true,
  path: 'assets/nono-form4-actions/nono4_action_ball.json',
});
```

- **6 个动作一律 `loop: false`**（原资源是 `gotoAndPlay(1)`，播一遍就结束）。
- **摆放按 `petAnchor` 对齐**（见 `meta.json`），不要按画布居中 ——
  各动作画布尺寸差异很大（189×104 ~ 743×436）。
- 动作与八方向走路、表情三者取景与宠物锚点一致，可随时互换。

## 音效

6 个动作在**原资源里带内嵌声音**，音频在 [`../nono-sounds/`](../nono-sounds/)。
`manifest.json` 里每个动作的 `audio` 段直接写明配哪个音效、什么时候播、播几次、怎么播。

> 形态 2 的动作音效文件名带 `_s2` 后缀；形态 4 用**不带后缀**的默认名
> （`action_boot.mp3` 等），与形态 1 / 3 一致。

## 逐像素验证

参考帧由 **FFDec 渲染原 SWF** 得到（根时间轴展到 N 帧、去背景色、按 Lottie 画布取景），
Lottie 帧由 **lottie-web 在浏览器里渲染**，两边逐像素比对（阈值 40）：

| 动作 | 平均差异 | 最差单帧 |
|---|---|---|
| 开机 | 1.67% | 1786 px (帧 33) |
| 关机 | 0.42% | 303 px (帧 13) |
| 玩球 | **0.14%** | 2315 px (帧 145) |
| 吃电池 | 0.75% | 923 px (帧 87) |
| 召唤 | 0.58% | 1213 px (帧 45) |
| 玩魔方 | 0.82% | 734 px (帧 17) |
| **总体** | **0.30%**（412252 / 135729122 px） | |

对照基线：形态 1 动作 0.70%、形态 3 动作 0.64%。形态 4 动作**优于**前两个形态。

> **玩球 / 吃电池 / 召唤 / 玩魔方**这四个动作里也有**变形形状**（角色身上那对青色鳍），
> 早先同样没被导出、整块丢失；补上后差异从 0.33% 降到 **0.30%**。
> 详见 [`../../开发注意事项.md`](../../开发注意事项.md) 铁律 27。

「开机 1.67%」是本批最高的一个，原因是原资源给开机动画里的元素加了
`GLOWFILTER`（`action/2_4.xml` 里 3 个），参考帧外围有一圈柔和辉光、Lottie 是硬边，
辉光边缘的像素必然对不上。

## 已知偏差：GLOW 滤镜未还原

形态 4 的动作 XML 里 `GLOWFILTER` 的数量：

| 资源 | GLOWFILTER | BLURFILTER |
|---|---|---|
| `action/2_4.swf`（开机） | 3 | — |
| `action/3_4.swf`（关机） | 22 | — |
| `action/4_4.swf`（玩球） | 142 | **3** |
| `action/5_4.swf`（吃电池） | 375 | — |
| `action/6_4.swf`（召唤） | 95 | — |
| `action/7_4.swf`（玩魔方） | 129 | — |

Lottie（bodymovin 规范）**没有 glow 滤镜**，只有 Drop Shadow 和 Gaussian Blur
两种滤镜效果；GLU 的 glow 无法直接表达。

> 形态 1 的动作脚本曾用「同心描边」近似 glow，实测在吃电池 / 玩魔方 / 玩召唤上
> **加了反而差异更大、体积翻倍**，因此本批资产**一律不做滤镜近似**，
> 宁可保持矢量干净，也不引入更差的近似。

## 重建

```bash
# 1) 导出素材（FFDec，顺序执行，别并发，机器会烤热）
bash _work/tools/export_f4.sh

# 2) 转换 + 清单
_work/venv/bin/python _work/tools/swf2lottie_action.py --form 4
_work/venv/bin/python _work/tools/build_manifest.py --form 4

# 3) FFDec 渲染动作参考帧（顺序执行）
bash _work/tools/render_ref_f4_actexp.sh

# 4) 浏览器渲染 Lottie 帧落盘（落盘页从 8903 打开，落盘服务在 8915）
#    http://127.0.0.1:8903/preview/_dump_action.html?kind=action&form=4&a=boot,shutdown,ball,battery,summon,cube&f=all&d=0&dump=8915

# 5) 逐像素比对
_work/venv/bin/python _work/tools/cmp_all.py ref_act_f4 lottiedump_f4
```
