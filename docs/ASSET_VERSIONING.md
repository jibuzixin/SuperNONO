# 素材版本管理

素材包（`assets/`）从 **1.0.0** 起纳入版本管理。

## 版本文件

| 文件 | 作用 |
|---|---|
| `assets/asset-version.json` | 当前版本、发布日期、变更摘要 |
| `assets/CHANGELOG.md` | 按版本记录素材变更 |
| `assets/catalog.json` 的 `asset_version` | 由构建工具写入，运行时读取 |
| `assets/tuning.json` | 随素材一起管理（它是素材的运行时修正，不是代码） |

## 版本号（语义化）

- **MAJOR**：不兼容变更，如 catalog 结构、形态/动画 ID、标签语义变化。
- **MINOR**：新增形态 / 动画 / 方向，或新增、改变染色标签。
- **PATCH**：素材修正、`tuning.json` 修正、标签微调（不改 ID/结构）。

## 更新流程

1. 替换 `assets/` 下的 Lottie JSON 和对应 `manifest.json`（新增形态还要补 `FORM_NAMES`/配置上限）。
2. `python3 tools/build_catalog.py`
3. 更新 `assets/asset-version.json` 的 `version` / `released` / `notes`。
4. 在 `assets/CHANGELOG.md` 顶部加一条记录。
5. 再跑一次 `python3 tools/build_catalog.py`，把新版本写进 `catalog.json`。
6. `python3 tools/build_catalog.py --check` 和 `cargo test --workspace`。
7. 提交后打标签：`git tag assets-vX.Y.Z`。

## 运行时如何使用

- `nono --check` 会打印 `assets vX.Y.Z`；启动日志带 `asset_version=...`。
- 染色缓存按源文件的 mtime/大小失效，换素材后会自动重建，无需手动清缓存。

## 约定

- 不手工编辑 `catalog.json`，它只由 `tools/build_catalog.py` 生成。
- `tuning.json` 不要被上游素材整包覆盖；它是本地对齐修正。
- 标签只认人工写入的 `nono.dye`；没有 `dye` 的填充永远不染色。
