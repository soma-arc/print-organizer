# コンフィグ ホットリロード計画

対応タスク: plan.md Task D（シェーダホットリロードの続き）

> **前提**: シェーダホットリロード（Step 1–4）は実装済み。本文書はコンフィグ JSON のホットリロードを扱う。

## 1. 目的

コンフィグ JSON（`.json`）の保存を検知し、設定変更（AABB・シェーダ・voxel_size 等）を手動再オープンなしでプレビューに即時反映する。

## 2. 現状

| 項目 | 現在の仕組み |
|------|-------------|
| コンフィグ読み込み | ファイルダイアログ or D&D → `load_config_file()` で一度のみ |
| コンフィグ変更の反映 | 手動（JSON を再度開く必要あり） |
| コンフィグ監視 | **なし** |
| シェーダ監視 | あり（`_watcher` / `watcher_rx` / `pending_reload`） |

### 課題

- コンフィグ JSON（AABB・voxel_size・shader パス等）を編集しても、プレビューに反映されない

## 3. 設計

### 3.1 ファイル監視

- **既存クレート**: `notify`（シェーダ監視で使用済み）
- **監視対象**: `config_path`（現在ロード中の JSON ファイルの絶対パス）
- **イベント**: `Modify` / `Create` / `Remove` — シェーダ監視と同じ方式
- **通知経路**: `notify::Watcher` → `mpsc::Sender<()>` → `update()` で `try_recv()`

```
┌──────────┐  Modify/Create  ┌──────────┐  mpsc   ┌─────────┐
│ FS       │  /Remove        │ notify   │ ──────> │ MyApp   │
│ (config) │ ──────────────> │ Watcher  │         │ update()│
└──────────┘                 └──────────┘         └─────────┘
                                                       │
                                              reload_config(device)
                                              ├── パース成功 → ConfigInfo 更新
                                              │   ├── シェーダパスが変わった場合 → start_watching_shader()
                                              │   └── selected_preset を維持（範囲外なら None にリセット）
                                              └── パース失敗 → config_error 表示、他の状態は変えない
                                                  Watcher は維持（次の保存で再試行）
```

### 3.2 リロードフロー

```
1. update() で config_watcher_rx.try_recv() を確認
2. pending_config_reload = Some(Instant::now())
3. request_repaint_after(200ms)
4. 200ms 経過後: reload_config(device) 呼び出し
5. reload_config() — load_config_file とは別ロジック（§3.6 参照）
   a. config_path から JSON をパース
   b. パース成功:
      - ConfigInfo・camera・out_dir_override を更新
      - config_error = None
      - シェーダパスが変わっていれば start_watching_shader() を再起動
      - selected_preset: プリセット数が減って範囲外 → None にリセット、それ以外 → 維持
      - selected_preset が Some の場合 → apply_preset() で effective config を適用
   c. パース失敗:
      - config_error にエラーメッセージを設定
      - config / config_info / config_dir / selected_preset / Watcher は一切変更しない
6. request_repaint()
```

**`reload_config` と `load_config_file` の違い:**
- `load_config_file` はファイルダイアログ/D&D からの初回ロード用。失敗時に状態を全クリアし、`selected_preset` を常に `None` にリセットする（意図通り）
- `reload_config` はホットリロード用。失敗時に前の状態を保持し、`selected_preset` を可能な限り維持する

### 3.3 デバウンス

シェーダ監視と同じ 200ms / `request_repaint_after` 方式を採用する（既実装パターンの再利用）。

### 3.4 Watcher のライフサイクル

| イベント | 動作 |
|---------|------|
| `load_config_file(path)` 成功 | `start_watching_config(path)` を呼ぶ（前の Watcher を drop してから開始） |
| `load_config_file(path)` 失敗 | `start_watching_config(path)` を呼ぶ（パスは既知なので、修正後の再保存を検知するため監視を継続） |
| `reload_config` パース成功 | Watcher はそのまま維持（同じパスを監視中） |
| `reload_config` パース失敗 | Watcher はそのまま維持（次の保存で再試行） |
| 別の JSON を開く（`load_config_file`） | 旧 Watcher を drop → 新パスで再開始 |
| アプリ終了 | `_config_watcher` の drop で自動停止 |

### 3.5 MyApp への追加フィールド

```rust
// --- config hot-reload ---
/// File watcher for the config JSON
_config_watcher: Option<notify::RecommendedWatcher>,
/// Receives notifications from the config file watcher
config_watcher_rx: Option<mpsc::Receiver<()>>,
/// Debounce: timestamp of last config watcher event
pending_config_reload: Option<Instant>,
```

### 3.6 新規メソッド

```rust
fn start_watching_config(&mut self, config_path: &Path)
fn stop_watching_config(&mut self)

/// ホットリロード専用。load_config_file とは異なり、パース失敗時に前の状態を保持する。
/// 成功時も selected_preset を可能な限り維持する。
fn reload_config(&mut self, device: &wgpu::Device)
```

`reload_config` の疑似コード:
```rust
fn reload_config(&mut self, device: &wgpu::Device) {
    let Some(path) = self.config_path.clone() else { return };
    let cfg_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let cfg = match sdf_baker::config::load_config(&path) {
        Ok(cfg) => cfg,
        Err(e) => {
            self.config_error = Some(format!("{e:#}"));
            self.needs_repaint = true;
            return;  // 他の状態は変更しない、Watcher も維持
        }
    };

    let info = ConfigInfo::from_config(&cfg, &cfg_dir);
    self.camera = OrbitCamera::from_aabb(info.aabb_min, info.aabb_size);
    self.out_dir_override = /* ... */;

    // シェーダ再読み込み（load_config_file と同じロジック）
    // ...

    // selected_preset の境界チェック
    if let Some(idx) = self.selected_preset {
        let count = cfg.presets.as_ref().map_or(0, |p| p.len());
        if idx >= count {
            self.selected_preset = None;
        }
    }

    self.config = Some(cfg);
    self.config_dir = Some(cfg_dir);
    self.config_info = Some(info);
    self.config_error = None;

    // プリセット選択中なら effective config を適用
    if self.selected_preset.is_some() {
        self.apply_preset(self.selected_preset, device);
    }

    self.needs_repaint = true;
}
```

### 3.7 update() への追加（シェーダ監視と対称的な実装）

```rust
// Config hot-reload (debounced)
if let Some(rx) = &self.config_watcher_rx {
    if rx.try_recv().is_ok() {
        while rx.try_recv().is_ok() {}
        self.pending_config_reload = Some(Instant::now());
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}
if let Some(t) = self.pending_config_reload {
    if t.elapsed() >= Duration::from_millis(200) {
        self.pending_config_reload = None;
        if let Some(ref device) = device {
            self.reload_config(device);
        }
    }
}
```

## 4. 実装ステップ

### Step 1: フィールド追加と Watcher 起動

- `MyApp` に `_config_watcher`, `config_watcher_rx`, `pending_config_reload` を追加
- `start_watching_config()` / `stop_watching_config()` を実装（`start_watching_shader` のパターンを踏襲）
- `load_config_file()` の末尾で成功・失敗どちらの場合も `start_watching_config(path)` を呼ぶ
- **Done when**: コンフィグ JSON を保存すると `config_watcher_rx` にメッセージが届く（ログで確認可）

### Step 2: `reload_config()` 実装と `update()` 受信ロジック追加

- `reload_config()` を §3.6 の疑似コードに沿って実装（`load_config_file` とは別ロジック）
  - パース成功: config / camera / shader / config_info を更新、`selected_preset` は範囲チェック付きで維持
  - パース失敗: `config_error` のみ更新、他の状態は保持
- `update()` 内のシェーダ監視ブロックの直後に §3.7 のコードブロックを追加
- repaint ガード条件に `self.pending_config_reload.is_some()` を追加
- **Done when**: コンフィグ JSON を保存すると AABB・シェーダ設定がプレビューに自動反映される。パース失敗時は `config_error` が表示され、前のプレビューが維持される。プリセットを減らしてもクラッシュしない

## 5. リスク・注意点

- **Watcher の二重起動**: `load_config_file` から `start_watching_config` が呼ばれるため、`start_watching_config` は必ず `stop_watching_config` を先に呼ぶこと（既存の `start_watching_shader` と同じ方式）
- **シェーダ Watcher との干渉**: コンフィグが変更されると `reload_config` → `start_watching_shader` が再実行されシェーダ Watcher も再起動する。これは正常動作（シェーダパスが変わりうるため）
- **同一ディレクトリの二重監視**: シェーダとコンフィグが同じディレクトリにある場合（典型的）、シェーダ Watcher とコンフィグ Watcher が同じ親ディレクトリを監視する。`notify` は同一ディレクトリの複数 Watcher をサポートするが、各 Watcher が全ファイルのイベントを受信する。パスフィルター（`event.paths.iter().any(|p| p == &filter_path)`）で対処するため実害なし
- **配信遅延**: JSON の書き込みが部分的な状態（未完成）でイベントが来た場合、パースに失敗する。デバウンス 200ms で概ね吸収されるが、巨大ファイルや低速ストレージでは失敗表示が瞬間的に出ることがある。パース失敗時に前状態を維持するため実害は限定的

## 6. 未決事項

- [ ] コンフィグエラー表示の改善（現状は1行の `config_error: Option<String>`）は別タスクとする
