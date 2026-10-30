# 開発

README の「開発」（`make setup` / `make ci` と標準のターゲット）に載せていない手順をまとめる。
ツールの版は `mise.toml` が正で、Makefile はツールを `mise exec --` 経由で呼ぶので、`mise activate` していなくても
同じ版で動く。ターゲットの一覧は引数なしの `make` で出る。

## 準備

```bash
make setup         # mise.toml のツールを入れ、Cargo.lock どおりに依存を取る
make setup-hooks   # 50MB 超のファイルと、非公開辞書入力の公開側へのコミットを止める
```

mise を使わずに PATH にあるツールで動かすなら `SYSTEM_TOOLS=1` を付ける（例: `make install SYSTEM_TOOLS=1`。版はそろわない）。

mise で入れられないものは OS のものを使う。`make dict` 系（`scripts/build-dict.sh`）と UniDic の取得には
git・curl・xz・unzip が、リリースの辞書の圧縮（CI）には zstd が要る。

## 検査とテスト

`make ci` は CI の quality ジョブと同じ検査（整形・clippy・テスト）で、`make check`（`make fmt-check` と `make lint`）と
`make test` からなる。

`make test` と `make lint` は、ライブラリとして使う 3 つの構成（feature なし・`analyzer`・`download`）も確かめる。
hasami-python は pyo3 の extension-module のため、macOS・Linux ではテストバイナリをリンクできない。そこで `make test` からは
外し、コンパイルは `make lint`（`clippy --workspace`）で確かめる。make のターゲットが無い操作は、コマンドの前に
`mise exec --` を付ける。

```bash
# Python バインディングを含むワークスペース全体のビルド（make build はルートのクレートだけ）
mise exec -- cargo build --workspace

# 配布辞書を使う #[ignore] のテスト（先に make dict-download で dict/ に辞書を取る。
# -- --ignored だけにすると、ネットワークを使うテストまで走る）
mise exec -- cargo test --locked --workspace --exclude hasami-python -- --ignored distributed
```

辞書を全件なめる経路（`info --verify`・`export`）の性能を変えたときは、変更前後のバイナリを
`scripts/bench-dictionary-tooling.py` に渡して交互に測る。出力（検証の報告・CSV）が変更前後で
一致することもスクリプトが毎回確かめる。

```bash
mise exec -- python scripts/bench-dictionary-tooling.py   --before <変更前のhasami.exe> --after <変更後のhasami.exe>   --dict dict/ipadic.hsd --dict dict/ipadic-neologd-sudachi.hsd   --rounds 7 --output <出力先>/tooling.jsonl
```

## CLI の実装

`src/main.rs` がコマンドを振り分け、`build`・`merge` の引数と実行は `src/cli/build.rs`、
`repair` は `src/cli/repair.rs`、配布辞書を扱う `dict` は `src/cli/dict.rs` に置く。
`build`・`merge`・`repair` 共通の CSV 入力・メタデータ・辞書の書き出しは `src/cli/write.rs`、
`tokenize` の入出力・並列処理・書式化は `src/cli/tokenize.rs` が受け持つ。
`src/cli/` はバイナリ専用で、ライブラリの公開 API には含めない。

標準入力の処理は `process_input` が `Read` と `Write` を受け、実際の標準入力・標準出力への接続は
`run` が行う。入力の分割位置や割り込み、壊れた UTF-8、読み書きと flush の失敗は、
読み書き先を差し替える単体テストで確かめる。読み込む前に出力を flush すること、壊れた行の手前にある
完結した行だけを出すこと、並列でも入力順に出すことを保つ。

## 辞書

辞書の衝突や未知語をコーパスで調べるには `scripts/audit_morphology.py` を使う。
同一入力を変更前後の辞書で解析し、未知語に加えて読み・品詞・語の境界の変化を確認する。
未知語率は正答率ではないので、候補の文脈と修正の副作用を確認する。
入力資料のファイル名・書名・本文・文脈付き解析結果は公開記録やコミットに含めない。
共有する記録には集計値と修正の根拠を残し、説明やテストの例文は独立に作成する。

配布辞書はリポジトリに置かず、リリースの添付ファイルで配る。開発で `dict/` に辞書が要るときは、この版のリリースから
取るか、上流のソースから作る（`dict/*.hsd` は `.gitignore` 済み）。
独自 CSV は非公開リポジトリで管理し、公開側では `dict/` 全体を追跡しない。
辞書を構築する場合だけ、[dictionaries.md](dictionaries.md) の手順で非公開側の main の HEAD を取得する。
通常のビルドと CI は非公開 CSV なしで実行できる。

```bash
make dict-download        # この版（Cargo.toml の version）のリリースの 3 辞書を dict/ に取る
make dict                 # 上流のソースから作る
```

作り方と辞書ごとのターゲット（`make dict-ipadic` など）は [dictionaries.md](dictionaries.md) の「辞書のローカルビルド」、
配布辞書をその場で直す `make dict-repair` は [dictionary-repair.md](dictionary-repair.md) にある。

## CI

CI（`.github/workflows/ci.yml`）の Quality ジョブは、Linux と macOS で `make setup` と `make ci` だけを実行する。
Windows では make を使わない。代わりに Build ジョブの Windows の行で `cargo test` を直接回す。Build ジョブは、
リリースと同じ 5 つのターゲット（Linux の x86_64・ARM64、macOS の Apple Silicon・Intel、Windows の x86_64）を
それぞれの OS のランナーでビルドする。

Windows のローカルで `make ci` を実行するときは、Git for Windows のシェルを明示する。
辞書ソースの Python テストも Bash を使うので、Git for Windows の `bin` を PATH の先頭に置く。
System32 の `bash.exe`（WSL の起動用）では、この Windows 向けのテストを実行できない。

```powershell
make SHELL=C:/PROGRA~1/Git/bin/sh.exe ci
$env:PATH = "C:\Program Files\Git\bin;" + $env:PATH
mise exec -- python -m unittest discover -s scripts -p 'test_dictionary_*.py'
```

どのジョブも `.github/actions/setup-mise`（jdx/mise-action）で mise.toml の版のツールを入れる。mise 自身は、公開から
14 日たった最新の版になる。`mise.toml` の版を変えたら `mise.lock` を作り直す（CI は lock の URL と SHA-256 で取る。
トークンが無いと GitHub API の制限で記録が黙って欠ける）。

```bash
MISE_GITHUB_TOKEN=$(gh auth token) mise lock --platform linux-x64,linux-arm64,macos-arm64,macos-x64,windows-x64
```

`mise.lock` は書式 1 のまま持つ。CI の mise が書式 2 を読めるのは 2026.9.7 からなので、CI の mise がそれ以上になるまで
`mise lock --upgrade` はしない。lock は CI の mise と同じ版の mise で作る。

## リリース

GitHub Actions の Release（`.github/workflows/release.yml`）を手で動かす（Actions > Release > Run workflow）。
版は `yy.m.counter`（例: `26.9.104`）で、同じ月のうちは counter を 1 つずつ上げ、月が変わると 100 から数え直す
（月は日本時間で切る）。Release はタグを切る前に、上流版に変更がなくても独自辞書を含めて辞書と例外表を構築・検証し、
例外表の差分があればコミットする。その後、版を上げた Cargo.toml と Cargo.lock をコミットし、タグを切る。5 ターゲットの
バイナリと、タグのソースから作った配布辞書（`dict-build.yml` を呼ぶ）をリリースに添付する。

`dry_run` にチェックを入れて動かすと、次の版を計算して表示するだけで、コミット・タグ・ビルド・リリースはしない。

### HSD 公開後の Kotonoha 連携

Release の `notify-kotonoha` ジョブは、HSD の公開と添付の検証が成功した後に
`owayo/kotonoha` の `release.yml` を起動する。公開した hasami のタグを固定して渡し、
Kotonoha 側で変更の判定・13 KNM のビルド・HSD との結合検証・Release への添付まで行う。
`dry_run`、辞書だけの試作ビルド、公開・検証に失敗した実行では通知しない。定期チェックは使わない。

専用 GitHub App に **Actions: Read and write** だけを許可し、`owayo/kotonoha` のみにインストールする。
hasami の Actions variable **`KOTONOHA_APP_CLIENT_ID`** に Client ID、
Actions secret **`KOTONOHA_APP_PRIVATE_KEY`** に秘密鍵を登録する。
通知ジョブは Kotonoha の Actions だけに絞った短命の installation token を生成し、ジョブ終了時に失効させる。
Kotonoha の受信側を先に既定ブランチへ反映する。
リポジトリが同じ所有者でも、このリポジトリの `GITHUB_TOKEN` は Kotonoha の Actions を起動できない。

未設定・App の削除やインストール解除・通知失敗は `notify-kotonoha` を失敗にする。HSD は公開済みなので、
設定を直して失敗ジョブだけを再実行する。Kotonoha はキューで処理し、同じ HSD の重複通知や古い通知では再公開しない。
通知の成功は Kotonoha の実行開始要求を受け付けたことを示す。ビルド・公開の結果は Kotonoha 側の Actions で確認する。

辞書を変える PR では、Build Dictionaries（`.github/workflows/dict-build.yml`）をブランチで動かす。リリースと同じ手順で
作った辞書を artifact で受け取れる（手順は [dictionaries.md](dictionaries.md) の「辞書のローカルビルド」）。
