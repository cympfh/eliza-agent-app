@CLAUDE

これはあなたにお願いしたい事項の管理シートです。
完了したら, 見出しの後ろに [☑ YYYY-MM-DD] を追加してください。

1. [☑ 2026-02-19] is_listening という状態管理を無くす。
従って phrase に関する設定や match 処理も削除する。
ボタンによる start_monitoring, stop_monitoring だけを残す。

2. [☑ 2026-02-19] オプションに "VRChat のミュート状態を使う" を追加する。
これは、VRChat のミュート状態を監視して、ミュート状態なら start_monitoring する、
ミュート解除なら stop_monitoring する機能。
ボタンをクリックしなくて済むようになるオプション。

調査結果:
OSC 9001 ポートを Listen し、/avatar/parameters/MuteSelf (bool) を受信する。
MuteSelf=true → ミュート中 → start_monitoring
MuteSelf=false → ミュート解除 → stop_monitoring

3. 「適切な音量閾値を自動で設定する」機能を追加する。

「2秒間の無音」と「2秒以上喋ってもらう」を録音することで、
適切な音量閾値を自動で設定する機能。
無音の最大値と、喋ってもらう平均をそれぞれ閾値にする。

4. hotkey の設定は使ったことない。削除。

5. Refactoring: fire all warnings (NOTE: You can compile this by GNU make (make build-windows))
