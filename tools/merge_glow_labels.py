#!/usr/bin/env python3
"""把新标注目录里每层的 `nono.glow` 合并进我们的资产（只合并 glow，不动其它字段）。

用法：
    python3 tools/merge_glow_labels.py <labels_dir> [--dry]

- 两边图层数量 / 名字必须完全一致（已校验），按**下标**配对；
- 只写 `nono.glow`；已有 `nono` 的其它字段（role/dye/shape）原样保留；
- 输出保持原来的 key 顺序与紧凑格式（ThorVG 对 key 顺序敏感）。
"""
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, 'assets')


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    labels = sys.argv[1]
    dry = '--dry' in sys.argv
    total = tagged = baked = 0
    changed_files = 0
    for name in sorted(os.listdir(ASSETS)):
        src_dir = os.path.join(ASSETS, name)
        if not name.startswith('nono-form') or not os.path.isdir(src_dir):
            continue
        for fn in sorted(os.listdir(src_dir)):
            if not fn.endswith('.json') or fn in ('manifest.json', 'meta.json'):
                continue
            path = os.path.join(src_dir, fn)
            label_path = os.path.join(labels, name, fn)
            if not os.path.exists(label_path):
                print('缺少标注：', label_path)
                continue
            ours = json.load(open(path, encoding='utf-8'))
            theirs = json.load(open(label_path, encoding='utf-8'))
            ours_layers = ours.get('layers', [])
            their_layers = theirs.get('layers', [])
            if len(ours_layers) != len(their_layers):
                print('图层数不一致，跳过：', name, fn)
                continue
            local = 0
            for a, b in zip(ours_layers, their_layers):
                g = (b.get('nono') or {}).get('glow')
                if not g:
                    continue
                node = a.get('nono')
                if not isinstance(node, dict):
                    node = {}
                    a['nono'] = node
                node['glow'] = g
                tagged += 1
                local += 1
                total += 1
                if g.get('baked'):
                    baked += 1
            if local:
                changed_files += 1
                if not dry:
                    with open(path, 'w', encoding='utf-8') as fh:
                        json.dump(ours, fh, ensure_ascii=False, separators=(',', ':'))
    print('图层总数 %d，写入 glow %d 条（baked %d），涉及 %d 个文件%s' % (
        total, tagged, baked, changed_files, '（dry-run，未写盘）' if dry else ''))


if __name__ == '__main__':
    main()
