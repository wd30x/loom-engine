# loom-engine

[English](README.md) | [简体中文](README.zh-CN.md) | 日本語

Rust でゼロから書いたゲームエンジン（winit + wgpu + glam）です。AI エージェントが直接操作し、検証できるように設計しています。AI はシーンを読み込み、ボタンを押し、シミュレーションを 1 tick ずつ進め、状態全体を JSON で読み、スクリーンショットを確認できます。すべてウィンドウなしで動き、同じ入力からは常に同じ結果が得られます。

2D と 3D に対応した汎用エンジンを目指していますが、機能は実際のゲームが必要としたときにだけ追加します。現在は 2 つのゲームが開発を牽引しています。ソウルライクの戦闘プロトタイプと、小さな 2D ローグライクです。

> **現状：** 初期段階の実験的なプロジェクトです。フォーマットや API は頻繁に変わります。開発とテストは Windows でのみ行っています。

## 設計原則

- **すべてテキスト**：シーン、設定、データ、入力の記録は RON か JSON で書きます。AI がそのまま読み、差分を取り、編集できます。
- **ヘッドレス**：すべての機能が `--headless` で、ウィンドウを作らずに動きます。
- **決定論的**：シミュレーションは固定の 60 Hz の tick で進みます。実時間を読まず、乱数には必ずシードを与え、超越関数は `libm` を使います。同じ入力からはバイト単位で同じ出力が得られます。
- **観測可能**：どの機能も、スクリーンショット、JSON の状態ダンプ、tick ごとのトレース、ログのいずれかで確認できます。
- **機械向けの出力**：`loom` コマンドの stdout には JSON だけを出力し、人間向けのログは stderr に出します。

## 機能

**シミュレーションと入力**
- 固定ステップのクロックと、名前付きの入力アクション・軸
- RON スクリプトによる入力の記録と再生
- JSON の状態ダンプと、イベントと状態を tick ごとに書き出す JSONL トレース

**シーン**
- RON のシーンファイル：トランスフォーム、メッシュ、スプライト、タイルマップ、カメラ、ゲームごとのプロパティを持つノードツリー
- ウィンドウモードでのホットリロード。シーンファイルの解析に失敗しても、元のシーンのまま動き続けます

**描画**
- wgpu による描画。常にまずオフスクリーンに描くので、ウィンドウとヘッドレスの出力が一致します
- 3D の基本形状と glTF モデル。スケルタルアニメーション（クリップ、ブレンド、再生速度）に対応
- Aseprite のスプライトシートを使った 2D スプライトとタイルマップ
- ドット絵フォントの HUD と、デバッグ用ワイヤーフレーム（やられ判定、コライダー、ロックオンなど）

**AI 向けツール**
- `loom serve`：stdin と stdout で JSON-RPC をやり取りする、ヘッドレスのリモート操作セッション
- `loom_mcp`：`loom serve` の上に作った MCP サーバー。シーンの読み込みと編集、入力、tick の実行、状態の取得、スクリーンショットなどのツールを提供します
- `loom schema`：すべてのファイル形式と出力形式の JSON Schema

## 2 つのゲーム

**`games/souls`**：3D のソウルライク戦闘プロトタイプ。ダークソウル風の回り込みカメラで騎士を操作します。ローリング、バックステップ、ダッシュ、攻撃でスタミナを消費し、弱攻撃と強攻撃には先行入力があります。ロックオン、強靭度と怯み、回復の瓶、篝火と血痕もあり、距離に応じて技を選び、体力が半分になると速くなる第 2 形態に移るボスがいます。

**`games/rogue`**：『Cataclysm: Dark Days Ahead』を参考にした、小さなターン制の 2D ローグライク。タイルマップの上をテンキーか vi キーで 8 方向に移動します。ゾンビがプレイヤーを追いかけてきます。ダメージは固定で、倒すと死体が残り、メッセージログもあります。

## クイックスタート

必要なもの：

- Rust（ツールチェーンは `rust-toolchain.toml` で固定されており、`rustup` が自動でインストールします）
- [Git LFS](https://git-lfs.com/)：バイナリ素材のため、クローンする前にインストールしてください
- [PowerShell 7](https://github.com/PowerShell/PowerShell)（`pwsh`）：`scripts/` のスクリプト用

```powershell
git clone https://github.com/wd30x/loom-engine.git
cd loom-engine

# ボス戦をウィンドウで遊ぶ
cargo loom run --scene assets/scenes/boss.ron

# ローグライクをウィンドウで遊ぶ
cargo loom run --scene assets/scenes/rogue.ron

# ヘッドレスで 2 秒動かし、スクリーンショットと状態全体を保存する
cargo loom run --headless --frames 120 --scene assets/scenes/boss.ron --screenshot out/boss.png --dump-state out/boss.json

# フォーマット、lint、テスト：コミット前に毎回実行
pwsh scripts/verify.ps1
```

`cargo loom` は `loom` バイナリの cargo エイリアスです（`.cargo/config.toml` を参照）。スクリーンショット、ダンプ、記録などの実行時の出力は `out/` に置かれ、git では無視されます。

## コマンドライン

| コマンド | 内容 |
|---|---|
| `loom run` | シーンをウィンドウかヘッドレスで実行します。スクリーンショット（`--screenshot`）、状態ダンプ（`--dump-state`）、入力の再生（`--input`）と記録（`--record`）、トレース（`--trace`）、デバッグ用ワイヤーフレーム（`--debug-draw`）、解像度（`--size`）を指定できます |
| `loom view` | シミュレーションせずに、1 つまたは複数の tick でシーンを描画し、スクリーンショットを保存します |
| `loom serve` | ヘッドレスのリモート操作セッションを開始します。stdin で JSON-RPC のリクエストを受け、stdout に 1 行ずつ返信します |
| `loom schema` | シーンファイル、プロパティ、状態ダンプ、トレース行、イベントの JSON Schema を出力します |
| `loom model` | glTF バイナリモデル（`.glb`）をインポート規則に照らして検査し、内容を表示します |
| `loom clip` | `.glb` モデルのアニメーション 1 つを、loom のクリップファイル（RON）に変換します |
| `loom info` | 描画せずに GPU アダプターの情報を表示します |

各コマンドのすべてのオプションは `cargo loom <コマンド> --help` で確認できます。

## AI エージェントからエンジンを動かす

リポジトリには `.mcp.json` が含まれています。これを読む MCP クライアント（たとえばリポジトリで開いた Claude Code）は、`scripts/mcp.ps1` を通して `loom` の MCP サーバーを起動します。ツールは一連の流れをすべてカバーしています：

- `load_scene`、`reset`、`scene`、`node`、`edit_node`、`add_node`、`remove_node`、`edit_scene`、`save_scene`：シーンの読み込み、確認、編集
- `input`、`step`：アクションと軸を入力したまま N tick 進め、その間に起きたイベントを受け取る
- `state`、`screenshot`、`record`、`trace`：状態を読む、画面を見る、入力スクリプトやトレースを保存する
- `describe`、`schema`：アクション名とすべての形式を調べる

MCP を使わない場合は、`loom serve` に同じプロトコルで直接話しかけられます：

```console
$ echo '{"jsonrpc":"2.0","id":1,"method":"describe"}' | loom serve
{"jsonrpc":"2.0","id":1,"result":{"tick_hz":60,"tick":0,"game":"souls","scene":null,"edited":false,"actions":["attack","drink","heavy","interact","lock","roll"],"axes":["look_x","look_y","move_x","move_y"]}}
```

## ディレクトリ構成

| クレート | 役割 |
|---|---|
| `crates/loom_core` | 固定ステップのクロック、tick、入力状態 |
| `crates/loom_platform` | winit のイベントループとウィンドウ。OS のイベントを core の入力に変換します |
| `crates/loom_render` | wgpu による描画。常にまずオフスクリーンに描きます |
| `crates/loom_scene` | シーンファイルの解析とノードツリー |
| `crates/loom_engine` | ファサードクレートと `loom` コマンド。シーンが指定するゲームを選んで組み立てます |
| `crates/loom_mcp` | MCP アダプター。エンジンとは `loom serve` を通してのみ通信します |
| `games/souls`、`games/rogue` | ゲームロジック。描画やプラットフォーム層、互いのクレートには依存できません |

素材は `assets/` にあります。素材の各ディレクトリには、ファイルの作り方を説明する README があります。ほとんどの素材は Aseprite の Lua スクリプトか Blender の Python スクリプトから作り直せます。

## ドキュメント

プロジェクトのドキュメントは中国語で書かれています。

- [CLAUDE.md](CLAUDE.md)：プロジェクトのルールと作業の流れ
- [docs/architecture.md](docs/architecture.md)：アーキテクチャの決定事項と、すべての形式とシステムの仕様
- [docs/godot-reference.md](docs/godot-reference.md)：loom と Godot の概念の対応表

## コントリビュート

[CONTRIBUTING.md](CONTRIBUTING.md)（英語）をご覧ください。

## ライセンス

次のいずれかのライセンスを選んで利用できます：

- Apache License 2.0（[LICENSE-APACHE](LICENSE-APACHE)）
- MIT ライセンス（[LICENSE-MIT](LICENSE-MIT)）

特に明示しない限り、本プロジェクトへの取り込みを意図して提出されたコントリビューション（Apache-2.0 の定義による）は、追加の条項や条件なしに、上記のデュアルライセンスで提供されるものとします。
