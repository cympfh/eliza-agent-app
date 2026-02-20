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
