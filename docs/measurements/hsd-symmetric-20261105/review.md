# 設計・実装・採否のレビュー

2026-11-05、独立したCodexレビューで設計、試作実装、実測、最終v6を確認した。
指定の`ai-second-opinion`と`rust-guide`はローカルに存在せず、
このレビューを`ai-second-opinion --only codex` advancedの実行と扱わない。

## 実装前と試作

- 左右が同じ通常エントリだけを圧縮する判断は妥当。行列の正方形を条件にしない。
- 6Bの右IDは16bitを保持し、非対称な未知語は圧縮判定に含めない。
- 通常候補だけでなく、ラティスのleft_id参照、未知語からの辞書候補再選択、
  lookup・export・verifyを同じ読み手へ揃えるよう指摘された。対応済み。
- 既存の破損fixtureで6B幅を前提にしないよう指摘された。両モードで検証するよう更新した。
- 矩形行列、右ID32768以上、未知語の非対称ID、同点・辞書切替を追加した。
- writerの4B補助配列を省き、最終出力バッファへ直接レコードを書き込むよう修正した。

## 測定と採否

全216解析標本、2,232構築標本、72新プロセスロード標本とsource archiveを確認した。
推奨辞書の文学入力の初回+5.37%を残し、組の比・順序差・追加測定を分けて評価した。
追加の24プロセスは各8回の平均で、192回を独立標本として数えない。
負荷が高いため一定の速度向上は主張せず、出力一致・継続する退行が見つからないこと・
完成ファイルとPeak Working Setの削減を根拠に採用する判断を支持した。

測定後のrun.pyの条件照合・上書き防止の修正を、実測時のスクリプトと混同しないよう指摘された。
実測時の元ソースZIPとハッシュを残し、現行スクリプトとの差をREADMEに明記した。
符号付き相対差のCVは安定性の指標にしないこと、Windowsのメモリ値をRSSと呼ばないことも反映した。

## 最終v6

records.rs・lattice.rs・contextual_readings.rsは計測した5013と最終版でバイト同一。
reader.rs・writer.rs・mod.rsは説明だけ、container.rsはVERSIONと説明、非公開定数名の変更で、
解析のホットパスに未計測の変更はないことを照合した。

再構築したv5/v6を独立に読み、3辞書ともENTRIES以外の全セクション一致、
4Bから復元した全6Bエントリ一致、validationの実ファイルSHA一致を確認した。
計測v5と再構築v5の差はMETA hasami_versionだけだった。
全12条件のトークン出力digestも元計測の比較ログと一致した。採用の阻害要因はなかった。

`astro-sight review`では公開APIの署名変更とdead symbolはなかった。
`impact`の1件は、containerのprivateな`KNOWN_FLAGS: u32`とfeaturesの別binding
`KNOWN_FLAGS: u8`を結び付けた誤検出だった。親側のread-only Codex CLIも同じ判定。
素性decoderの変更は不要で、[astro-sight #232](https://github.com/owayo/astro-sight/issues/232)へ
公開可能な抜粋・観測範囲・小さい陽性再現が未確定である限界を記録した。

stopフックの再試行に際し、コンテナ側の非公開定数だけを`KNOWN_CONTAINER_FLAGS`へ改名した。
定数の値と判定式は同じで、features.rsは変更していない。
改名後は`astro-sight impact --dir . --git`が終了0となり、HSDのライブラリテストと整形も確認した。
この改名を性能改善やastro-sight本体の修正とは扱わない。
