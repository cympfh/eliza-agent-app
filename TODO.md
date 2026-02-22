@CLAUDE TODO List

## [x] ボタンの色に録音ステータスを反映 [2026-02-19 完了]
Recording してるとき、 ~/git/winh/ は silence_progress というのを計算してボタンの色に反映させてる。
これを完全に模倣して。

## [x] Stop Monitoring のバグ修正 [2026-02-19 完了]
Agent とのやり取りの途中、Stop Monitoring を手動で押しても、Agent からレスポンスが帰ってきたタイミングでまた Start Monitoring に切り替わってしまう。
状態管理にバグがありそうだ。

## [x] テキストでも直接送信できるようにする [2026-02-20 完了]

1行のテキストフォームを用意、Ctrl+Enter で送信できるようにする。
Shift+Enter で改行できるようにする。
ただのEnterは何もしないことに注意！

Monitoring の状態に関わらず、テキストを送信できるようにする。

## [x] /memory API は空なら投げない [2026-02-20 完了]

messages が空のときは /memory API を呼び出さないようにする。
→ eliza.rs の save_memory() に `if self.conversation_history.is_empty() { return Ok(()); }` が既に実装済みで対応完了。

## [x] Conversation の "You" の文字が白すぎて見えない [2026-02-20 15:58 完了]
`LIGHT_BLUE` → `from_rgb(30, 80, 180)` の濃い紺青に変更。

## [x] Sleep コマンドを追加したい [2026-02-22 完了]

NOTE: eliza-agent-app は ../eliza-agent-server/ と連携して動いている。
変更は eliza-agent-app にしてもよいし、 eliza-agent-server にしてもよい。
両方にしても良い。
もしもeliza-agent-server に変更を加えたいなら ../eliza-agent-server/TODO.md に TODO を書いてください。

ユーザーが明示的に「おやすみ」と言った、または明らかに寝息であったりして寝ていると判断したら、勝手に stop_monitoring() を呼び出す。

### 実装内容

- `eliza.rs`: `ChatResponse` に `sleep: bool` フィールドを追加（`#[serde(default)]` で省略可）
- `eliza.rs`: `send_message()` の戻り値を `Result<(String, bool), ElizaError>` に変更（bool は sleep フラグ）
- `main.rs`: `ProcessingMessage::ElizaComplete` に sleep フラグを追加: `ElizaComplete(String, bool)`
- `main.rs`: `ElizaAgentApp` に `pending_sleep: bool` フィールドを追加
- `main.rs`: `ElizaComplete` 受信時に sleep=true なら `pending_sleep=true` をセット
- `main.rs`: `Complete` 受信時に `pending_sleep` が true なら `stop_monitoring()` を呼び、ステータスに「おやすみなさい」を表示

sleep の判定は eliza-agent-server 側が担う。server 側の TODO.md に実装タスクを追加済み。
server が `sleep: bool` を返さない場合は `#[serde(default)]` により `false` として扱われるため、
server 側が未対応でも動作に問題はない。
