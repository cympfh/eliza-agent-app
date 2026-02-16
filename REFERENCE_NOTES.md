# Reference Implementation Notes

このプロジェクトの参考実装（winh, vrchatbox）を調査して分かったこと

## winh (~/git/winh/src/) - 音声認識アプリ

### アーキテクチャ概要

Windows11ネイティブのRust製GUI音声認識アプリケーション (egui使用)

**処理フロー:**
```
マイク入力 (cpal) -> 無音検出で自動停止 -> WAV保存 -> OpenAI Whisper API -> テキスト化
                                                                              ↓
                                                              クリップボード or 自動入力
```

### 主要モジュール

#### audio.rs - 音声録音
- **cpal** ライブラリでクロスプラットフォーム音声入力
- モノラル録音 (マルチチャンネルの場合は平均化してモノラル変換)
- リアルタイム無音検出:
  - `silence_threshold` (デフォルト: 0.01) を超える振幅を「音」として検出
  - 録音開始後3秒間は **grace period** (無音判定しない)
  - grace period 後、`silence_duration_secs`(デフォルト: 2秒) 経過で自動停止
- 音量レベルのリアルタイム表示（指数移動平均で平滑化）
- 前後の無音をトリミング（ただし0.2秒は保持）
- WAV形式 (16bit, mono) で一時ファイルに保存

#### openai.rs - Whisper API連携
- **reqwest** で同期的にHTTP通信
- エンドポイント: `https://api.openai.com/v1/audio/transcriptions`
- モデル: `gpt-4o-transcribe`（設定可能）
- カスタムプロンプト対応（デフォルト: "A Japanese is speaking. Transcribe it."）
- multipart/form-data でWAVファイルを送信
- レスポンスから `text` フィールドを取得

#### config.rs - 設定管理
- JSON形式で `~/.config/winh/config.json` に保存
- 設定項目:
  - `api_key`: OpenAI API キー
  - `model`: Whisperモデル名 (デフォルト: "gpt-4o-transcribe")
  - `silence_duration_secs`: 無音検出時間 (デフォルト: 2.0)
  - `silence_threshold`: 無音閾値 (デフォルト: 0.01)
  - `input_device_name`: 入力デバイス名 (Option<String>)
  - `hotkey`: グローバルホットキー (デフォルト: "Ctrl+Shift+H")
  - `clipboard_enabled`: クリップボード自動コピー (デフォルト: true)
  - `auto_input_enabled`: 自動入力 (デフォルト: true)
  - `auto_input_send_enter`: Enter自動送信 (デフォルト: false)
  - `custom_prompt`: Whisper APIに渡すプロンプト

#### main.rs - GUI
- **egui** (eframe) でGUI実装
- **global-hotkey** でグローバルホットキー対応
- 日本語フォント対応 (NotoSansJP)
- 録音状態の可視化:
  - リアルタイム音量レベル表示（バー）
  - 無音経過時間の可視化（ボタン内のプログレスバー）
- バックグラウンドスレッドで音声認識を実行（UIブロッキング回避）
- 結果の自動処理:
  - クリップボードコピー（オプション）
  - アクティブウィンドウへの自動入力（Ctrl+V or 文字入力）

### 実装の工夫

1. **0.5秒の準備期間**: 録音開始ボタンを押してから0.5秒待ってから実際の録音を開始
2. **Grace period**: 録音開始後3秒間は無音検出をしない（発話前の間を許容）
3. **デバイス選択**: "Windows既定" を含む複数の入力デバイスから選択可能
4. **エラーハンドリング**: 各処理でエラーを適切にユーザーに表示

---

## vrchatbox (~/bin/vrchatbox) - VRChat OSC送信

### 概要

VRChatのOSC APIを使ってチャットボックスにテキストを送信するPythonスクリプト

### 実装詳細

**依存ライブラリ:**
- `pythonosc`: OSC (Open Sound Control) プロトコル実装
- `click`: CLIインターフェース

**接続先:**
- IP: `withcache ipwin` コマンドで取得（WindowsのIPアドレス）
- ポート: 9000（VRChatのデフォルトOSCポート）

**OSC メッセージ:**

1. `/chatbox/input` - メッセージ送信
   - 引数: `[message: str, True, notify: bool]`
   - `message`: 表示するテキスト
   - 2番目の引数は常に `True`（即座に送信）
   - `notify`: 通知音を鳴らすかどうか

2. `/chatbox/typing` - タイピング状態制御
   - 引数: `indicator: bool`
   - タイピングインジケーターの表示/非表示

**使用方法:**
```bash
# 標準入力から
echo "Hello VRChat" | vrchatbox

# コマンドライン引数で
vrchatbox Hello VRChat

# オプション
vrchatbox --lazy 3 "Typing..."  # 3秒間タイピング状態を表示してからメッセージ送信
vrchatbox --quiet "Silent"       # 通知音なし
vrchatbox --verbose "Debug"      # デバッグ情報表示
```

**`withcache ipwin` について:**
- ユーザーのカスタムスクリプト（~/bin/にある）
- WindowsのIPアドレスをキャッシュして返す
- WSL環境でWindows側のIPを取得するためのユーティリティと推測

---

## eliza-agent への応用

### 必要な変更点

1. **音声認識部分**: winhのコードをベースにできるが、以下を調整:
   - GUIは必要に応じて変更（AI Agent会話用のUI）
   - OpenAI Whisper APIの処理はほぼそのまま流用可能

2. **AI Agent連携**: **新規実装が必要**
   - xAI APIとのWebSocket接続
   - 会話履歴の管理（max_length_of_conversation_history を考慮）
   - ストリーミングレスポンスの処理

3. **VRChat出力**: vrchatboxをそのまま使用可能
   - subprocess でコマンド実行
   - AI Agentの応答を標準入力経由で渡す
   - `--lazy` オプションで自然なタイピング演出が可能

### 再利用可能なコンポーネント

- ✅ `audio.rs`: ほぼそのまま流用可能
- ✅ `openai.rs`: Whisper API部分は流用可能
- ✅ `config.rs`: 設定管理の基本構造を流用（項目は追加）
- ⚠️ `main.rs`: UI部分は要調整（AI Agent会話UI）
- ✅ `vrchatbox`: 外部コマンドとしてそのまま使用可能

### 新規実装が必要な部分

1. **xAI Agent API クライアント**
   - WebSocket接続
   - 会話履歴管理
   - ストリーミングレスポンス処理

2. **会話状態管理**
   - 録音 → 認識 → AI Agent送信 → 応答受信 → VRChat送信の状態遷移
   - エラー時のリトライやフォールバック

3. **UI**
   - 会話履歴表示
   - Start/Stop ボタン（WebSocket接続制御）
   - 設定UI（API keyの追加など）
