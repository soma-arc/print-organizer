# project.json 仕様

## 0. 位置づけ

`project.json` は、1つのSDF作品を定義する**単一の入力ファイル**である。
現在 `csg.json` などの名前で使っているファイルの後継であり、GUI が「開く」唯一のファイルとなる。

- **ユーザーが手書きする入力**（シェーダパス、グリッド設定など）
- 将来的には**ベイク結果の記録を追記**する出力も兼ねる（v2 以降）
- ファイル名は自由（例: `csg.json`, `my-torus.json`）。`project.json` は規約名ではなく概念名

### 関連ファイルとの関係

```
<project-dir>/
├── project.json       ← このドキュメントが定義するファイル（ユーザー管理）
├── csg.wgsl           ← shader フィールドで参照
└── output/
    ├── manifest.json  ← ベイク完了後に sdf-baker が自動生成（docs/spec/genmesh_v1.md 参照）
    ├── bricks.bin
    └── mesh.stl
```

---

## 1. v1 フィールド定義

現在の実装（`sdf_baker::config::ConfigFile`）に対応する。**すべてのフィールドはオプション**。省略時はデフォルト値が使われる。

```jsonc
{
    // SDF シェーダファイルへのパス（project.json からの相対パス）
    // 省略時: built-in sphere SDF
    "shader": "csg.wgsl",

    // 出力先ディレクトリ（project.json からの相対パス）
    // 省略時: GUI の出力先フィールドで上書き必須
    "out": "output/csg",

    // グリッド設定
    "grid": {
        "aabb_min":   [0.0, 0.0, 0.0],   // AABB 原点 (mm)。省略時: [0,0,0]
        "aabb_size":  [128, 128, 128],    // AABB サイズ (mm)。省略時: [64,64,64]
        "voxel_size": 0.5,                // ボクセルサイズ (mm)。省略時: 1.0
        "brick_size": 64                  // ブリックサイズ (voxel)。省略時: 64
    },

    // ベイク設定
    "bake": {
        "half_width": 3,                  // ナローバンド半幅 (voxel)。省略時: 3
        "dtype": "f32"                    // 距離場データ型 "f32"|"f16"。省略時: "f32"
    },

    // メッシュ化設定
    "mesh": {
        "iso":         0.0,               // iso 値 (mm)。省略時: 0.0
        "adaptivity":  0.0                // ポリゴン削減強度 [0.0–1.0]。省略時: 0.0
    },

    // genmesh (C++ CLI) 設定
    "genmesh": {
        "path":      "tools/genmesh/build/Release/genmesh.exe",  // 実行ファイルパス
        "write_vdb": false,               // .vdb も出力するか。省略時: false
        "skip":      false                // genmesh 呼び出しをスキップ。省略時: false
    }
}
```

### デフォルト値一覧

| フィールド | デフォルト | 備考 |
|-----------|-----------|------|
| `shader` | (built-in sphere) | |
| `out` | — | GUI 入力必須 |
| `grid.aabb_min` | `[0, 0, 0]` | |
| `grid.aabb_size` | `[64, 64, 64]` | mm |
| `grid.voxel_size` | `1.0` | mm |
| `grid.brick_size` | `64` | voxel |
| `bake.half_width` | `3` | voxel |
| `bake.dtype` | `"f32"` | |
| `mesh.iso` | `0.0` | |
| `mesh.adaptivity` | `0.0` | |
| `genmesh.write_vdb` | `false` | |
| `genmesh.skip` | `false` | |

---

## 2. プリセット設計（v2 候補）

メモ（`tmp/project-json-plan.md`）での要求：
> 複数の条件（AABB位置・adaptivity等）を比較検討したり呼び出したりできる管理がしたい。

### 設計方針

- v1 の単一フラットな設定に、`presets` 配列を追加する
- `default` は presets 未指定時に使うベースライン
- 各プリセットは差分のみを持つ（ベースとのマージ）

```jsonc
// v2 イメージ（未実装）
{
    "shader": "gyroid.wgsl",
    "grid": { "aabb_size": [64, 64, 64] },
    "mesh": { "iso": 0.0, "adaptivity": 0.0 },

    "presets": [
        {
            "name": "low-res draft",
            "grid": { "voxel_size": 1.0 }
        },
        {
            "name": "high-res final",
            "grid": { "voxel_size": 0.2 },
            "mesh": { "adaptivity": 0.2 }
        }
    ]
}
```

GUI での扱いイメージ：
- サイドバーにプリセットのドロップダウン
- 選択するとそのプリセットのパラメータを上書き表示
- 各プリセットで独立してベイク＆出力先を持つ

### Uniform 変数（さらに将来）

> 将来的には uniform 変数を変えることで形状をそれぞれ変える。

SDF シェーダが uniform パラメータを持てるようになったとき、プリセットがその値を保持できる構造にしておく：

```jsonc
// v3 イメージ（未実装）
{
    "presets": [
        {
            "name": "thin wall",
            "uniforms": { "wall_thickness": 1.5 }
        },
        {
            "name": "thick wall",
            "uniforms": { "wall_thickness": 3.0 }
        }
    ]
}
```

---

## 3. バージョニング方針

- v1: 現在の実装。バージョンフィールドなし（省略＝v1 として扱う）
- v2 以降でフィールドを追加する際は `"version": 2` を追加
- **後方互換性**: v2 パーサーは `version` 未設定ファイルを v1 として読める
- **前方互換性**: 未知フィールドは無視（`#[serde(default)]` で現在も保証済み）

---

## 4. v1 スコープ外（将来）

| 機能 | バージョン |
|------|-----------|
| プリセット管理 | v2 |
| ベイク結果の追記（`result` セクション） | v2 |
| Uniform 変数 | v3 |
| アーカイブ（シェーダハッシュ・ツールバージョン記録） | v2 |

---

## 5. 既存ファイルとの対応

現在プロジェクト内にある `*.json` ファイルは **そのままの形式が v1 に対応**している。
`project.json` へのリネームは任意。フィールドの追加・変更なしに v1 として読み込める。

| 既存ファイル | v1 対応状況 |
|-------------|------------|
| `examples/csg/csg.json` | ✅ そのまま v1 |
| `examples/gyroid/gyroid.json` | ✅ そのまま v1 |
| `examples/linked-torus/linked-torus.json` | ✅ そのまま v1 |
| `examples/sphere/sphere.json` | ✅ そのまま v1 |
