# genmesh

Rust 側が生成したブリックベースの符号付き距離場 (SDF) データを OpenVDB 経由で等値面メッシュに変換し、バイナリ STL として出力する CLI ツール。

## 概要

```
manifest (project.json)
bricks.index.json + bricks.bin   ──►  genmesh  ──►  mesh.stl
                                                     report.json
                                                     volume.vdb (optional)
```

1. manifest + ブリックインデックス/バイナリを読み取り・検証する
2. OpenVDB FloatGrid を構築する（voxel_size / aabb_min 反映）
3. `volumeToMesh` で等値面を抽出し、quad → 2 triangle に分割する
4. バイナリ STL に書き出す（80B ヘッダ + 法線再計算）
5. `report.json` に統計・タイミング・エラーを記録する

## 前提条件

| ツール | バージョン要件 |
|--------|---------------|
| **CMake** | ≥ 3.20 |
| **vcpkg** | `$env:VCPKG_ROOT` が設定済みであること |
| **MSVC** (Windows) | Visual Studio 2022 / Build Tools 17.x |
| **C++ 標準** | C++17 |

vcpkg が管理する依存ライブラリ（`vcpkg.json` 参照）:

- **OpenVDB** — VDB グリッド構築・メッシュ化
- **nlohmann-json** — manifest / bricks.index.json パース

## ビルド

```powershell
cd tools/genmesh

# configure（vcpkg 依存を自動インストール）
cmake --preset default

# build
cmake --build --preset default

# test
ctest --preset default
```

Debug ビルドの場合:

```powershell
cmake --preset debug
cmake --build --preset debug
ctest --preset debug
```

ビルド成果物は `build/Debug/genmesh.exe`（または `build/RelWithDebInfo/genmesh.exe`）に出力される。

## 使い方

### 基本

```powershell
genmesh --manifest path/to/project.json --in path/to/data/ --out output/
```

### CLI オプション

| フラグ | 必須 | デフォルト | 説明 |
|--------|------|-----------|------|
| `--manifest <path>` | ✔ | — | manifest (project.json) のパス |
| `--in <path>` | ✔ | — | bricks.bin + bricks.index.json を含む入力ディレクトリ |
| `--out <dir>` | ✔ | — | 出力ディレクトリ（存在しなければ作成） |
| `--write-stl` | — | `true` | STL 出力を有効化 |
| `--no-write-stl` | — | — | STL 出力を無効化 |
| `--write-vdb` | — | `false` | `volume.vdb` も出力する |
| `--iso <float>` | — | manifest 値 or `0.0` | 等値面の値 |
| `--adaptivity <float>` | — | manifest 値 or `0.0` | メッシュ簡略化レベル (0.0–1.0) |
| `--force` | — | `false` | 既存出力ファイルを上書き許可 |
| `--log-level <level>` | — | `info` | `error` / `warn` / `info` / `debug` |
| `--debug-generate <shape>` | — | — | テスト用距離場を内部生成 (`sphere` / `box`) |
| `--help` | — | — | ヘルプ表示 |

`--debug-generate` 使用時は `--manifest` / `--in` は不要（`--out` のみ必須）。

### クイックスタート（debug-generate）

外部データなしでツールの動作を確認できる:

```powershell
mkdir out
genmesh --debug-generate sphere --out out/ --write-vdb
```

`out/` に以下が生成される:

- `mesh.stl` — 球体の三角形メッシュ
- `volume.vdb` — OpenVDB グリッド（`--write-vdb` 指定時）
- `report.json` — 実行統計 *(Phase 6 で実装予定)*

生成された `mesh.stl` は MeshLab 等で確認可能。

## 入出力ファイル

### 入力

| ファイル | 形式 | 説明 |
|---------|------|------|
| `project.json` | JSON | manifest — グリッド解像度・座標系・SDF パラメータ等 |
| `bricks.index.json` | JSON | ブリックのオフセット/サイズ/CRC のインデックス |
| `bricks.bin` | バイナリ | ブリック化された距離場データ (f16 / f32) |

### 出力

| ファイル | 形式 | 条件 |
|---------|------|------|
| `mesh.stl` | バイナリ STL | `--write-stl`（デフォルト有効） |
| `volume.vdb` | OpenVDB | `--write-vdb` 指定時 |
| `report.json` | JSON | 常に出力 |

出力ファイル名は v1 では固定（カスタマイズ不可）。

## 終了コード

| コード | 意味 |
|-------|------|
| 0 | 成功 |
| 1 | 一般エラー（未分類） |
| 2 | バリデーション失敗（manifest / bricks 不整合） |
| 3 | I/O エラー（ファイル読み書き） |
| 4 | 環境エラー（OpenVDB 初期化失敗等） |
| 5 | 処理エラー（VDB 構築 / メッシュ化失敗） |

## プロジェクト構成

```
tools/genmesh/
├── CMakeLists.txt
├── CMakePresets.json
├── vcpkg.json
├── include/genmesh/       # ヘッダ
│   ├── cli.h
│   ├── manifest.h
│   ├── output.h
│   ├── bricks_index.h
│   ├── bricks_data.h
│   ├── debug_generate.h
│   ├── vdb_builder.h
│   ├── mesher.h
│   ├── exit_code.h
│   ├── error_code.h
│   └── log.h
├── src/                   # 実装
│   ├── main.cpp
│   ├── cli.cpp
│   ├── manifest.cpp
│   ├── output.cpp
│   ├── bricks_index.cpp
│   ├── bricks_data.cpp
│   ├── debug_generate.cpp
│   ├── vdb_builder.cpp
│   └── mesher.cpp
└── tests/                 # テスト
    ├── test_phase0.cpp
    ├── test_cli.cpp
    ├── test_manifest.cpp
    ├── test_output.cpp
    ├── test_bricks_index.cpp
    ├── test_bricks_data.cpp
    ├── test_debug_generate.cpp
    ├── test_vdb_builder.cpp
    ├── test_mesher.cpp
    └── fixtures/
        ├── valid_manifest.json
        └── valid_bricks_index.json
```

## 仕様

詳細仕様は [docs/spec/genmesh_v1.md](../../docs/spec/genmesh_v1.md) を参照。

JSON スキーマ:

- [manifest.v1.schema.json](../../docs/schemas/manifest.v1.schema.json)
- [bricks-index.v1.schema.json](../../docs/schemas/bricks-index.v1.schema.json)
- [report.v1.schema.json](../../docs/schemas/report.v1.schema.json)

## ライセンス

プロジェクトルートの [LICENSE](../../LICENSE) を参照。
