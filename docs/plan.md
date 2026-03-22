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
- **→ フィールド仕様・プリセット設計: [docs/spec/project-json.md](spec/project-json.md)**
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

### Task A: 三角形→フルスクリーンクアッド ✅

- Done when
  - フルスクリーン描画でシェーダ出力が画面全域に出る
  - 画面解像度変更でも破綻しない

### Task B: リポジトリ作成 ✅

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

### Task F0: 既存OpenVDB CLI簡易版の監査とギャップ整理（追加） ✅

- 目的
  - 既存CLIを“ベースライン”として再利用できる範囲と、不足（スケール/Transform/ナローバンド/ポリゴン削減/出力/テスト/Windowsビルド）を明確化する
- Done when
  - 既存CLIの入出力・依存・ビルド手順を整理し、計画書のTask F〜Jに対する対応表（OK/要修正/未実装）が1枚で作られている
  - 既知のバグ候補（例: halfWidthの単位、bboxMinのTransform反映、VDB初期化等）がリスト化される

### Task F: OpenVDB CLIツール整備（骨格） ✅

- 目的
  - OpenVDB処理を「距離場→VDB化→メッシュ化→ポリゴン削減（必須）」に限定したC++ CLIとして分離し、Rust側から安定して呼び出せる状態にする
- Done when
  - `genmesh --manifest <path> --in <path> --out <dir>` が動作し、失敗時に理由が分かるエラーを返す
  - `openvdb::initialize()` を含む初期化が明示されている
  - `--iso` `--adaptivity`（ポリゴン削減に使用） `--write-vdb` `--write-stl` など最低限のフラグがある

### Task F2: Windows向けOpenVDBビルド環境の確立（追加） ✅

- 目的
  - Windows（MSVC）でOpenVDBと依存を再現性高くビルドできる手順を確立し、CI/配布に備える
- Done when
  - vcpkg（推奨）または Conan のいずれかで `openvdb` を取得し、ローカルで `genmesh` がビルドできる
  - ビルド手順が `docs/build_windows.md` に記録され、クリーン環境で再現できる
  - 依存（oneTBB/Imath/Blosc等）がロックされる（vcpkg manifest / conan profile）

### Task G: manifestスキーマ（v1）と入出力フォーマット仮決め ✅

- 目的
  - Rust↔CLI間の互換性の軸を作り、アーカイブのSSOTを確立する
- Done when
  - manifest v1（units, aabb, voxelSize, brickSize, halfWidthVoxels, dtype, axis order, sign convention等）が定義される
  - CLI側でmanifestの必須項目検証を行い、不整合を明示できる

### Task H: 統合スモークテスト（最小） ✅

- 目的
  - 「サンプル距離場（小サイズ）」→CLI→STL/GLB が一連で再現できることを確認する
- Done when
  - 例: 64^3 または 128^3 相当の小ケースで、生成物が出力される
  - 生成条件（manifest）と出力ファイルが同一プロジェクト単位に保存される

### Task I: CLI単体テスト用フィクスチャとゴールデンテスト（追加） ✅

- 目的
  - CLIが「ブリック距離場＋manifest」しか受け付けない場合でも、デバッグ可能な最小ケースを用意し、退行を検知する
- Done when
  - `fixtures/` に最小入力（manifest+brick）が複数用意され、`ctest` で実行できる
  - 各フィクスチャについて、(a) 終了コード、(b) 出力メッシュの統計（頂点/三角形数・AABB・表面積の概算など）、(c) エラー文言の一致（必要箇所のみ）を検証できる
  - フィクスチャは「球・箱・トーラス・平面」など解析しやすい形状を含む

### Task J: デバッグ入力モードの追加（追加） ✅

- 目的
  - 本番I/Oを変えずに、CLI側だけで再現可能な入力を作れてデバッグが容易になるようにする
- Done when
  - `genmesh --debug-generate sphere --voxel-size 0.2 --aabb 100 --half-width-voxels 8 ...` のように、内部生成SDFから距離場を作れる（最小で sphere/box）
  - または `--in-dense-raw`（dense入力）を追加し、ブリックI/Oの層を切り分けてテストできる

### ✅ Task K: GUI 統合 Phase G1 — JSON 読み込み + bake + ファイル出力 (f16312b)

- 前提
  - `crates/sdf-baker` の lib API（`config::load_config`, `config::resolve_config`, `compute::*`, `bricks_writer::*`, `genmesh_runner::*`）を GUI から呼び出す
  - GPU デバイスは **独立**（eframe の wgpu device と sdf-baker の `init_gpu()` は別インスタンス）。VRAM 2重消費だが bake 完了後に drop されるため実用上問題なし
  - 非同期モデルは `std::thread::spawn` + `std::sync::mpsc::channel`。tokio は不要（GPU compute 待ちのみ、pollster と競合リスク回避）
  - ファイルダイアログは `rfd` クレート（eframe 公式実績あり、Windows 追加依存なし）
- 変更箇所
  - `Cargo.toml`（ルート）: `sdf-baker = { path = "crates/sdf-baker" }` と `rfd` を依存追加
  - `src/app.rs`: UI 状態（`config_path`, `resolved`, `bake_status`）追加、サイドパネル UI
  - sdf-baker 側の変更は不要
- UI 構成
  - サイドパネル:
    - 「JSON を開く」ボタン → `rfd::FileDialog` でファイル選択 → `load_config` + `resolve_config` で即パース
    - パース結果のパラメータ表示（shader, grid dims, brick_size, voxel_size 等）
    - 「Bake & Export」ボタン → 別スレッドで bake パイプライン実行 → 完了時にステータス更新
    - ステータス表示（Idle / Running / Done(成功/失敗)）
  - メインパネル: 既存のオフスクリーン描画をそのまま維持（G2 でプレビューに差し替え）
- Done when
  - `examples/sphere/sphere.json` を GUI で開き、パラメータが表示される
  - 「Bake & Export」で STL が出力される（UI はブロックしない）
  - bake 中にプログレス（またはスピナー）が表示され、完了/失敗がステータスに反映される
  - エラー時（不正 JSON、shader コンパイル失敗等）にエラーメッセージが UI に表示される

### ✅ Task L: GUI 統合 Phase G2 — SDF リアルタイムプレビュー (92f2cca, 3276342)

- 前提
  - Task K 完了後に着手
  - ユーザーの SDF 関数をレイマーチングフラグメントシェーダに埋め込み、リアルタイム描画する
  - **WGSL / GLSL 両対応**: テンプレートは WGSL のみ。GLSL ユーザーSDF はダミー compute シェーダに包み naga で WGSL 変換、SDF 関数を抽出して WGSL テンプレートに挿入する
    - WGSL ユーザーSDF → `raymarch_template.wgsl` に挿入 → そのまま使用
    - GLSL ユーザーSDF → `glsl_sdf_to_wgsl()` で SDF 関数のみ WGSL 変換 → `raymarch_template.wgsl` に挿入
    - 既存の `sdf_baker::shader_compose::{load_shader, glsl_to_wgsl, validate_wgsl}` を再利用
  - GPU デバイスは **独立を維持**（eframe と sdf-baker は共に wgpu 27 だが、eframe 内部の device は egui-wgpu が管理しており直接共有は非自明。将来的にデバイス共有を検討可能）
- サブタスク
  - **L1**: レイマーチテンプレート作成（`src/shaders/raymarch_template.wgsl`）
    - Uniform: `camera_pos`, `camera_target`, `camera_up`, `aabb_min`, `aabb_size`, `resolution`, `time`
    - `{{USER_SDF}}` プレースホルダ（compute テンプレートと同一方式）
    - レイマーチループ + Phong 簡易ライティング（後から差し替え可能な構造にする）
    - レイマーチパラメータ: `MAX_STEPS=256`, `MIN_DIST=0.001`, `MAX_DIST` は AABB 対角線長から自動算出（固定で開始、後から UI 調整可能にする前提）
    - 背景: ダークグレーのグラデーション
  - **L2**: シェーダ合成モジュール（`src/preview_compose.rs`）
    - `compose_preview(lang, user_sdf) -> Result<String>` (WGSL ソースを返す)
    - `load_shader` は `sdf_baker::shader_compose` から再利用
    - naga validate / GLSL→WGSL 変換関数も再利用
  - **L3**: Uniform 拡張 + パイプライン再構築
    - `GlobalsUniform` をカメラ + AABB パラメータに拡張
    - JSON 読み込み時またはシェーダ変更時に `render_pipeline` を再作成
    - コンパイルエラー時は前のパイプラインを維持（フォールバック）
  - **L4**: カメラ操作
    - orbit（左ドラッグ）/ zoom（ホイール）/ pan（中ボタンドラッグ）
    - 初期カメラ: `target = aabb_min + aabb_size * 0.5`, `distance = length(aabb_size) * 1.5`, `yaw=45°, pitch=30°`
    - 感度は固定（合理的なデフォルト）で開始
  - **L5**: AABB + ブリック分割の可視化
    - 初期実装: レイマーチ内で SDF ベース box-frame 描画（別パス不要）
    - 将来ラインレンダリングに移行可能な構造にする
    - AABB / ブリック境界のそれぞれにトグル（チェックボックス）
  - **L5.1**: AABB クリッピング
    - `scene_sdf() = max(sdf(), aabb_sdf())` でジャイロイド等の無限周期 SDF を AABB で切り取る
    - 「AABB クリップ」チェックボックスでオン/オフ切替（デフォルト: オン）
    - Globals uniform に `clip_aabb: u32` フィールド追加
  - **L6**: オフスクリーンテクスチャのリサイズ対応
    - available size が前フレームと異なる場合のみテクスチャ再作成
    - 最小 64×64、最大 4096×4096 にクランプ
  - **L7**: 静止時の repaint 抑制
    - カメラ操作中・SDF 変更直後のみ連続再描画、静止時は `request_repaint()` を止める
- 変更箇所
  - `src/shaders/raymarch_template.wgsl` (新規)
  - `src/preview_compose.rs` (新規)
  - `src/graphics/uniform.rs`: GlobalsUniform 拡張
  - `src/graphics/pipeline.rs`: create_render_pipeline にバインドグループ変更
  - `src/app.rs`: パイプライン再構築ロジック + カメラ状態 + リサイズ
  - `src/shader.wgsl`: 不要 → 削除（動的生成に移行）
  - sdf-baker 側の変更は不要
- Done when
  - WGSL の JSON を開くと SDF がレイマーチでリアルタイムプレビューされる
  - GLSL の JSON を開いても同様にプレビューされる
  - カメラ操作でインタラクティブに視点変更できる
  - shader コンパイルエラー時は前のパイプラインで描画継続（落ちない）
  - AABB 範囲とブリック分割が可視化される
  - パネルリサイズ時にプレビュー解像度が追従する
  - JSON 未読み込み時はプレビュー非表示（空のパネル）

### Task M0: GUI config 保持リファクタリング（プリセット前段）

- 目的
  - 現在 GUI は JSON から表示用の `ConfigInfo` を作るだけで、`ConfigFile` 自体を保持していない。Bake 時にディスクから再読み込みしている。この構造ではプリセット切替・GUI からのパラメータ編集・Bake への反映ができないため、先にデータフローを改善する
- 現状の問題
  - `ConfigFile` は `load_config_file()` 内のローカル変数として消費され、`MyApp` には残らない
  - `ConfigInfo` は表示専用（read-only）で、Bake パイプラインはこれを使わない
  - `spawn_bake()` は `config_path: PathBuf` のみ受け取り、JSON をディスクから再パースする
  - デフォルト値の `.unwrap_or()` が `ConfigInfo::from_config()` と `run_bake_pipeline()` の2箇所に重複している
- 変更内容
  1. `ConfigFile` に `Clone` を derive
  2. `MyApp` に `config: Option<ConfigFile>` フィールドを追加し、ロード時に保持する
  3. `ConfigInfo` を `config` から都度 derive する形に統一（計算ロジックの SSOT 化）
  4. `spawn_bake()` のシグネチャを `ConfigFile` + `config_dir: PathBuf` 受け取りに変更し、ディスク再読み込みを廃止
  5. デフォルト値適用を一箇所に集約（`ConfigFile` → 解決済みパラメータの変換関数）
- Done when
  - 既存の動作（JSON ロード→表示→Bake）が変わらず動く
  - `MyApp.config` に `ConfigFile` が保持され、Bake がそこから値を取得する
  - ディスク再読み込みが廃止されている

### Task M1: プリセットデータ構造と JSON パース

- 目的
  - project.json v2 のプリセット配列をパースし、ベースとの差分マージを行えるようにする
- 仕様参照
  - [docs/spec/project-json.md §2](spec/project-json.md)
- 変更内容
  1. `ConfigFile` に `version: Option<u32>` と `presets: Option<Vec<PresetEntry>>` を追加
  2. `PresetEntry` 構造体: `name: String` + 各セクション（`grid`, `bake`, `mesh`, `genmesh`, `out`, `shader`）をすべて `Option` で持つ
  3. マージ関数 `merge_preset(base: &ConfigFile, preset: &PresetEntry) -> ConfigFile`
     - `Option` レベルでマージ: プリセットの `Some` のみベースを上書き、`None` はベースを継承
     - `.unwrap_or()` デフォルト適用はマージ後に一度だけ行う
  4. JSON Schema v2 更新: `presets` 配列 + `version` フィールド追加
  5. `version` 未設定の JSON は v1 として扱う（後方互換）
- Done when
  - v1 JSON（presets なし）が従来通りパースできる
  - v2 JSON（presets あり）がパースでき、`merge_preset()` で正しいマージ結果が得られる
  - マージのユニットテスト: ベースの値をプリセットが上書き / プリセット省略でベース継承 / ネスト（grid 内の一部フィールドのみ上書き）

### Task M2: GUI プリセット選択 UI

- 目的
  - サイドパネルにプリセット選択ドロップダウンを追加し、選択に応じてパラメータ表示・プレビューを切り替える
- 変更内容
  1. `MyApp` に `selected_preset: Option<usize>` を追加（`None` = ベース設定）
  2. サイドパネル上部（JSON パス表示の直下）にドロップダウン: "(base)" + 各プリセット名
  3. 選択変更時:
     - `merge_preset()` でマージ済み `ConfigFile` を計算
     - `ConfigInfo` を再 derive → パラメータ表示更新
     - AABB が変わった場合はカメラをリセット（`OrbitCamera::from_aabb()`）
     - `shader` が変わった場合のみパイプライン再構築
     - `out` がプリセットで指定されていれば `out_dir_override` を更新
  4. presets が空または未定義の場合、ドロップダウンは非表示
- プレビュー連動の判定
  - AABB 変更 → uniform 更新（次フレームで自動反映）+ カメラリセット。パイプライン再構築は不要
  - shader 変更 → パイプライン再構築 + カメラリセット
  - mesh/bake パラメータ変更 → 表示更新のみ（プレビューに影響なし）
- Done when
  - v2 JSON を開くとドロップダウンにプリセット一覧が表示される
  - プリセット切替でパラメータ表示が更新される
  - AABB の異なるプリセットに切り替えるとカメラがリセットされ、プレビューの AABB が変わる
  - v1 JSON ではドロップダウンが表示されない

### Task M3: プリセット付き Bake

- 目的
  - 選択中のプリセットでマージ済みのパラメータを使って Bake を実行する
- 変更内容
  1. Bake ボタンクリック時に `selected_preset` に応じたマージ済み `ConfigFile` を `spawn_bake()` に渡す
  2. 出力先はベースの `out` の下にサブディレクトリを自動生成する:
     - ベース選択時 → `{base.out}/default/`
     - プリセット選択時 → `{base.out}/{preset_name_slug}/`
     - プリセットに `out` が明示されている場合 → プリセットの `out`（上記ルールを上書き）
  3. `preset_name_slug`: プリセット名をファイルシステム安全な形に変換（スペース→ハイフン、記号除去等）
- Done when
  - ベース選択で Bake すると `{base.out}/default/` に出力される
  - プリセット選択で Bake するとプリセットのパラメータが使われ、`{base.out}/{preset_name_slug}/` に出力される
  - 異なるプリセットで Bake した結果が異なるサブディレクトリに保存される
  - v1 JSON（presets なし）では従来通り `{out}/` 直下に出力される（サブディレクトリなし）

### 設計上の懸念と対策

| # | 懸念 | 対策 |
|---|------|------|
| 1 | **Bake がディスクから JSON を再読みする** — マージ済みパラメータを渡す手段がない | Task M0 で `spawn_bake` を `ConfigFile` 受け取りに変更し、ディスク再読み込みを廃止 |
| 2 | **デフォルト値の二重適用** — `ConfigInfo::from_config()` と `run_bake_pipeline()` に同じ `.unwrap_or()` が散在 | Task M0 でデフォルト適用関数を一箇所に集約。Task M1 のマージは `Option` レベルで行い、デフォルト適用はマージ後に一度だけ |
| 3 | **プリセットごとの出力先ルール** — `out` をプリセットで上書きするか、ベースの `out` + プリセット名サブディレクトリか | **[D] 決定**: ベースの `out` の下にサブディレクトリを自動生成。ベース→ `default/`、プリセット→ `{preset_name_slug}/`。プリセットに `out` が明示されていれば上書き。v1（presets なし）は従来通り直下出力 |
| 4 | **プリセット選択状態の永続化** — GUI 再起動時にどのプリセットを選んでいたか | **[H] 仮説**: v2 初版では保存しない（常にベースで起動）。将来 `last_selected` を別ファイルに保存可能 |
| 5 | **プリセット名の一意性** — 同名プリセットが複数ある場合 | パース時に重複を警告。UI はインデックスで管理するため動作は壊れない |
| 6 | **ConfigFile に `Serialize` がない** — テンプ JSON 書き出しやデバッグ表示に不便 | Task M0 で `Clone` の derive を追加。`Serialize` は必要時に追加（方式 A では不要） |

## 9. 主要な意思決定ポイント

### 9.0 決定済み

- **WGSL/GLSL**: 両方対応。WGSL を一次入力、GLSL は naga 経由変換（R4 で実装済み）
- **OpenVDB 連携の I/O 形式**: ブリック距離場（f32 LE）+ manifest.json（R2/R5 で実装済み）
- **GPU デバイス（G1/G2）**: 独立（eframe と sdf-baker は共に wgpu 27 だが、eframe 内部の device は egui-wgpu が管理しており直接共有は非自明。当面は独立で運用）
- **非同期モデル**: `std::thread` + `mpsc`（tokio 不要）
- **ファイルダイアログ**: `rfd` クレート
- **プレビュー方式**: レイマーチ（フラグメントシェーダ）
- **プレビューテンプレート**: WGSL / GLSL 両方用意（compute テンプレートと同一パターン）
- **プリセット（v2）**: config を GUI が保持 → プリセットは `Option` レベルの差分マージ → Bake には `ConfigFile` を直接渡す
- **AABB とプレビューの連動**: AABB は uniform で渡されるため、プリセット切替時にパイプライン再構築は不要（shader 変更時のみ再構築）

### 9.1 未確定

- GPU デバイス共有は eframe の wgpu device を sdf-baker に渡す方法が判明次第、再検討
- ボリューム表現（3D texture / buffer / スパース）— プレビューがレイマーチなので当面不要
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

