# SDF→3Dプリント パイプライン計画書

## 0. 目的とスコープ

- **目的**: 任意SDF（シェーダで定義）から、3Dプリント用メッシュ（STL）を再現性高く生成し、プレビュー・アーカイブ・高速化まで含めた一連の制作フローを確立する。
- **運用ルール（仮決めを安全にする）**
  - **[H] 仮説**: いまの暫定案。いつでも変更可。
  - **[E] 実験設定**: 次の短い作業単位（2時間〜半日）で検証する条件。
  - **[D] 決定**: 比較・測定の根拠が揃ってから確定する。
- **MVPスコープ**（優先順）
  1. SDFプレビュー（切り出し範囲・解像度の検討ができる）
  2. ボクセル化（Compute）→ SDFボリューム生成
  3. OpenVDBでメッシュ化（＋ポリゴン削減：必須）
  4. メッシュプレビュー
  5. 出力（STL）

## 1. 現状と課題（要約）

- 現状: sphairahedronのjson→obj出力。ZBrushでデシメーション。別途テクスチャ生成プログラム。
- 課題
  - 他のSDFフラクタルが生成できない（汎用入力がない）
  - プレビューがなく、生成後に見直しが必要
  - アーカイブ不備（パラメータと成果物の紐付けが手作業）
  - CPU計算で遅い
  - ボクセル粒度（分割密度）を事前に把握できない

## 2. 技術スタック案（前提）

- Windows向けネイティブアプリ: Rust
- GUI: egui
- GPU: wgpu（将来的にWeb/WASM展開も視野）
- メッシュ化: OpenVDB（FFIを避け、最小のC++ CLIツールとして分離）
  - **前提**: C++ OpenVDB CLIの簡易版が既に存在する（これをベースに改修・整備する）

## 3. 主要ユースケース

1. **SDFを読み込む**（glsl/wgsl）
2. **プレビュー**
   - カメラ操作・ライティング
   - 切り出し（AABB/中心・スケール）
   - 解像度（ボクセルサイズ）を調整し、品質/コストを見積もる
3. **生成**
   - ComputeでSDFサンプル→ボリューム（距離場/符号付き距離）を出力
   - OpenVDBでメッシュ化→簡易デシメーション
4. **メッシュ確認**
   - manifoldチェック指標（穴/法線/境界）
   - スケール確認（mm単位）
5. **出力**
   - STL
6. **アーカイブ**
   - 入力（シェーダ・パラメータ）と出力（STL/プレビュー画像/ログ）を一体化して保存

## 4. データ設計（案）

### 4.1 プロジェクト単位

- 1作品=1プロジェクトフォルダ（または1つのアーカイブ）
- 推奨構造（例）
  - project.json: メタ情報・生成条件（v1）
  - schemas/: manifest.v1.schema.json（推奨）
  - shaders/ : sdf.wgsl（またはglsl）
  - outputs/ : mesh.stl, mesh.obj(任意), preview\.png, report.json
  - cache/ : volume.raw（任意）

### 4.2 必須メタ情報（最低限）

- git commit hash（またはビルドID）
- シェーダファイルハッシュ
- 生成パラメータ（解像度、AABB、iso値、スケール、単位）
- OpenVDB処理パラメータ（メッシュ化設定、デシメーション率など）
- 実行ログ（時間、GPU/CPU情報、警告）

## 5. アーキテクチャ（案）

- Rustアプリ（UI+GPU+プレビュー）

  - Renderer（wgpu）
  - Shader loader（ホットリロード）
  - SDF preview pipeline（レイマーチ/サーフェス近似）
  - Volume compute pipeline（ブリック単位の距離場生成を想定）
  - Exporter（OpenVDB CLI呼び出し + STL保存）
  - Archive manager（成果物とメタ情報の紐付け）

- C++ CLI（OpenVDB）

  - 役割を「**距離場→VDB化→メッシュ化（＋最小限の後処理）**」に限定
  - 入力: Rustが出力した距離場データ（ブリック推奨）＋ manifest
  - 出力: mesh（stl/obj/glb）＋ vdb（アーカイブ用に推奨）

### 5.1 データ受け渡し（現時点の方針）

- **[E] 推奨**: `ブリック距離場 + manifest` をRustが書き出し、C++がVDB構築とメッシュ化を担当
- **用語（ブリック）**
  - **ブリック（brick）**: 距離場ボクセル格子を、固定サイズ（例: 64×64×64）の立方体ブロックに分割した“チャンク”。
  - 目的: GPU計算・I/O・アーカイブをブロック単位にし、空領域のスキップやストリーミングを容易にする。
  - 注意: OpenVDB内部のleaf（典型8×8×8）とは別概念。受け渡し・計算の単位として任意に選ぶ。
- 理由
  - Rust側でOpenVDB実装（FFI/依存）を抱えない
  - AABB/voxelSize/符号規約などを manifest で固定でき、アーカイブと再現性が高い

### 5.2 型定義のSSOT（JSON Schema）

- **正**: `docs/schemas/` に配置する JSON Schema ファイルが、Rust↔C++ 間の契約の唯一の正（SSOT）となる。
- **スキーマファイル**:
  - `manifest.v1.schema.json` — manifest（project.json）の型定義
  - `bricks-index.v1.schema.json` — bricks.index.json の型定義
  - `report.v1.schema.json` — report.json の型定義
- **各言語での扱い**:
  - **Rust** (`crates/core`): `serde` で型定義を実装。CIで正規スキーマとの整合を検証（`schemars` で Rust 型からスキーマ生成→正規スキーマと diff、または `jsonschema` クレートでラウンドトリップ検証）
  - **C++** (`tools/genmesh`): `nlohmann_json` で手動パース。CIで fixtures を `docs/schemas/*.json` に対してバリデーション
- **運用ルール**:
  - フィールドの追加・変更は JSON Schema を先に更新し、各言語の実装を追従させる
  - fixtures の JSON は CI で常にスキーマバリデーションを通す

## 6. MVP仕様（暫定）

### 6.1 Input

- 固定パスでのシェーダ読み込み（最初は**WGSL**を一次入力として仮置き）
- シェーダに以下の関数を要求（インターフェースを固定）
  - `fn sdf(p: vec3<f32>) -> f32`（距離）

### 6.2 Output

- STL（バイナリ）
- 併せて `project.json`（生成条件）と `preview.png` を保存

### 6.3 実験用ベースライン（現時点の仮置き）

- **[E] プリント想定**: FDM。0.2mmノズル対応まで視野。
- **[E] AABB**: 100mm（基準ケース）
- **[E] voxelSize**: 0.2mm（まず“感覚”を得るための現実的基準）
- **[H] 参考（過大側の物差し）**: voxelSize=0.1mm は 1000^3 相当になり得るため、当面は必須としない
- **[E] ブリック**: `brickSize = 64^3`（可変パラメータとして設計。代替: 32^3 / 128^3）
- **[E] ナローバンド**: `halfWidthVoxels = 8`（帯域はケチらない）
- **[E] iso**: 0.0
- **[E] adaptivity**: 0.0（品質優先の基準。比較用に 0.2 / 0.4 も試す）

### 6.4 プレビュー要件

- 2種類の表示モード
  1. SDFサーフェス表示（レイマーチ等）
  2. ボクセル分割の可視化（グリッド/セルサイズ表示、推定メモリ量）

## 7. OpenVDB CLI 要件

**→ [docs/spec/genmesh_v1.md](spec/genmesh_v1.md) に分離。**

コーディングエージェントへ渡す際は spec ファイルを単独で参照すること。
スキーマ定義は [docs/schemas/](schemas/) を参照。

## 8. 2時間〜半日タスクの具体化（初期）

### Task A: 三角形→フルスクリーンクアッド

- Done when
  - フルスクリーン描画でシェーダ出力が画面全域に出る
  - 画面解像度変更でも破綻しない

### Task B: リポジトリ作成

- Done when
  - `cargo run` でウィンドウが出る
  - CI（任意）でフォーマットとビルドが通る

### Task C: 固定パスの任意シェーダで絵を出す

- Done when
  - `shaders/sdf.wgsl` を読み込み、コンパイル失敗時はエラー表示
  - 1つのサンプルSDFで動く

### Task D: ファイル読み込み＋ホットリロード

- Done when
  - 保存すると自動で再コンパイルされ反映
  - エラー時は前のバイナリで描画継続（落ちない）

### Task E: 解像度・コスト見積もりパネル（追加）

- 目的
  - voxelSize と AABB を入力したときに、ボクセル数・推定メモリ・概算コストを“生成前”に把握する
- Done when
  - `AABB(mm)` と `voxelSize(mm)` の入力で、`dims`、`N^3`、推定メモリ（f16/f32）を即時計算して表示
  - 例: AABB=100mm, voxel=0.2mm（=500^3）など基準ケースをワンクリックでセットできる

### Task F0: 既存OpenVDB CLI簡易版の監査とギャップ整理（追加）

- 目的
  - 既存CLIを“ベースライン”として再利用できる範囲と、不足（スケール/Transform/ナローバンド/ポリゴン削減/出力/テスト/Windowsビルド）を明確化する
- Done when
  - 既存CLIの入出力・依存・ビルド手順を整理し、計画書のTask F〜Jに対する対応表（OK/要修正/未実装）が1枚で作られている
  - 既知のバグ候補（例: halfWidthの単位、bboxMinのTransform反映、VDB初期化等）がリスト化される

### Task F: OpenVDB CLIツール整備（骨格）

- 目的
  - OpenVDB処理を「距離場→VDB化→メッシュ化→ポリゴン削減（必須）」に限定したC++ CLIとして分離し、Rust側から安定して呼び出せる状態にする
- Done when
  - `genmesh --manifest <path> --in <path> --out <dir>` が動作し、失敗時に理由が分かるエラーを返す
  - `openvdb::initialize()` を含む初期化が明示されている
  - `--iso` `--adaptivity`（ポリゴン削減に使用） `--write-vdb` `--write-stl` など最低限のフラグがある

### Task F2: Windows向けOpenVDBビルド環境の確立（追加）

- 目的
  - Windows（MSVC）でOpenVDBと依存を再現性高くビルドできる手順を確立し、CI/配布に備える
- Done when
  - vcpkg（推奨）または Conan のいずれかで `openvdb` を取得し、ローカルで `genmesh` がビルドできる
  - ビルド手順が `docs/build_windows.md` に記録され、クリーン環境で再現できる
  - 依存（oneTBB/Imath/Blosc等）がロックされる（vcpkg manifest / conan profile）

### Task G: manifestスキーマ（v1）と入出力フォーマット仮決め

- 目的
  - Rust↔CLI間の互換性の軸を作り、アーカイブのSSOTを確立する
- Done when
  - manifest v1（units, aabb, voxelSize, brickSize, halfWidthVoxels, dtype, axis order, sign convention等）が定義される
  - CLI側でmanifestの必須項目検証を行い、不整合を明示できる

### Task H: 統合スモークテスト（最小）

- 目的
  - 「サンプル距離場（小サイズ）」→CLI→STL/GLB が一連で再現できることを確認する
- Done when
  - 例: 64^3 または 128^3 相当の小ケースで、生成物が出力される
  - 生成条件（manifest）と出力ファイルが同一プロジェクト単位に保存される

### Task I: CLI単体テスト用フィクスチャとゴールデンテスト（追加）

- 目的
  - CLIが「ブリック距離場＋manifest」しか受け付けない場合でも、デバッグ可能な最小ケースを用意し、退行を検知する
- Done when
  - `fixtures/` に最小入力（manifest+brick）が複数用意され、`ctest` で実行できる
  - 各フィクスチャについて、(a) 終了コード、(b) 出力メッシュの統計（頂点/三角形数・AABB・表面積の概算など）、(c) エラー文言の一致（必要箇所のみ）を検証できる
  - フィクスチャは「球・箱・トーラス・平面」など解析しやすい形状を含む

### Task J: デバッグ入力モードの追加（追加）

- 目的
  - 本番I/Oを変えずに、CLI側だけで再現可能な入力を作れてデバッグが容易になるようにする
- Done when
  - `genmesh --debug-generate sphere --voxel-size 0.2 --aabb 100 --half-width-voxels 8 ...` のように、内部生成SDFから距離場を作れる（最小で sphere/box）
  - または `--in-dense-raw`（dense入力）を追加し、ブリックI/Oの層を切り分けてテストできる

## 9. 主要な意思決定ポイント（未確定）

- **WGSL/GLSLどちらを一次入力にするか**
- プレビューは「レイマーチ」か「メッシュ簡易生成（スライス/等値面）」か
- ボリューム表現（3D texture / buffer / スパース）
- OpenVDB連携のI/O形式（raw、vdb、他）
- 解像度の上限、メモリ制約の扱い

## 10. インタビュー（詰めるための質問リスト）

### 9.1 目的・利用シーン（現時点の回答を反映）

- **[E] プリント方式**: FDM（0.2mmノズル対応まで視野）
- **[E] 生成時間の目安**: まずは「数分オーダー」をターゲットにし、解像度を上げすぎない
- **[H] 解像度の感覚**: 以前は“1ボクセル=0.001”のように指定していたが、N^3換算の実感が薄いため、まずUIで見積もり可能にする
- **[E] 基準ケース**: AABB=100mm、voxelSize=0.2mm、halfWidthVoxels=8
- **[H] 参考（過大側の物差し）**: AABB=100mm、voxelSize=0.1mm（=1000^3相当）は当面必須にしない

### 9.2 SDF仕様

- 距離関数 `sdf(p)` を必須とし、色/ID等は後回し
- 座標系は「world=mm」を基本仮定（AABBをmm指定）

### 9.3 プレビュー要件

- 生成前に「切り出し範囲」「ボクセル粒度」「コスト見積もり」が分かることを最優先

### 9.4 ボクセル化とメッシュ化

- 表面近傍のみ扱うナローバンドを前提（帯域はケチらない）
- OpenVDB CLIは、入力（距離場＋manifest）→VDB→meshに責務を限定

### 9.5 アーカイブ

- manifestをSSOTとして、入力（シェーダ/パラメータ）と出力（STL/プレビュー/ログ）を同一単位で保存

## 11. 次の進め方（運用ルール案）

- 毎回の打ち合わせで
  - 「今回決めること（最大3つ）」
  - 「作業タスク（2時間〜半日）を2〜4個」
  - 「Done条件」
  - 「リスクと回避策」
    を確定する。

---

## 付録: 最小インターフェース例（草案）

- WGSL
  - `fn sdf(p: vec3<f32>) -> f32`
  - `fn bounds() -> vec3<f32>`（任意: 推奨）
  - `fn default_iso() -> f32`（任意）

