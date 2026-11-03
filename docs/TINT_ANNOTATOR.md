# 调色部件标注（形态逐个标注）

PNG 遮罩是固定分辨率的，放大到高 DPI 会糊，而且角色一移动就对不上。所以调色采用
**按 Lottie 图层（部件）标注**：标注只记录「这个动画里哪些图层要染色」，渲染时直接改这些
图层的矢量填充色——任意缩放都清晰，并且跟着动画走。

## 数据

- `assets/tint_parts.json`：标注结果，格式
  ```json
  { "format_version": 1, "animations": { "form3/walk/down": [
      { "layer": 1, "paths": [0] },
      { "layer": 7, "paths": [2, 3] } ] } }
  ```
  键是动画 id（`form{形态}/walk|action|expr/{slug}`）；每个部件是「图层下标 + 该图层内的
  路径序号组」。同一个 fill 的多条路径会按包含关系拆成“外轮廓 + 洞”和独立小岛（高光、
  眼睛等），所以壳体和高光可以分开选。
- 没有条目的动画回退到内置启发式（本体图层 + 像素补染）；有条目则**只**染这些部件，
  所以标注时要把它自己的白球、耳根、铠甲等全部选上。

## 工具

```bash
# 1. 导出某个形态的动画帧 + 图层色块图（每帧一张）
./target/release/nono --dump-frames /tmp/nono-tint-annotator/data --form 3 --scale 3 --count 12
./target/release/nono --dump-layers /tmp/nono-tint-annotator/data --form 3 --scale 3 --count 12

# 2. 启动标注网页
python3 tools/tint_annotator/serve.py --data /tmp/nono-tint-annotator/data --port 54321
# 浏览器打开 http://127.0.0.1:54321/
```

网页左侧选动画；画面上**点击要染色的部件**，被选中的图层会显示红色高亮并加入列表；
可以切换帧查看动作后段才出现的部件（例如形态 3 玩球时飞到上面的铠甲）；点「保存标注」写入
`assets/tint_parts.json`。不要点小手/鳍、耳朵白光、汗滴、气泡、灯泡、魔方、面罩眼睛高光。

标注完成后重启桌宠（`cargo run --release -p nono-app`）即可生效。

## 原理

`--dump-layers` 把每个图层临时涂成唯一颜色（`r = i & 255, g = (i>>8) & 255, b = 255`），
渲染成色块图；网页点击时按颜色反查图层下标。渲染时读取 `tint_parts.json`，只对选中图层的
近白/中性填充做换色，其他图层、其他颜色一律不动。
