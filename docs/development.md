# 開発

README の「開発」（`make setup` / `make ci` と標準のターゲット）に載せていない手順をまとめる。
ツールの版は `mise.toml` が正で、Makefile はツールを `mise exec --` 経由で呼ぶので、`mise activate` していなくても
同じ版で動く。ターゲットの一覧は引数なしの `make` で出る。

## 準備

```bash
make setup         # mise.toml のツールを入れ、Cargo.lock どおりに依存を取る
make setup-hooks   # clone したら一度入れる（50MB を超えるファイルをコミットしようとすると pre-commit が止める）
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

## 辞書

配布辞書はリポジトリに置かず、リリースの添付ファイルで配る。開発で `dict/` に辞書が要るときは、この版のリリースから
取るか、上流のソースから作る（`dict/*.hsd` は `.gitignore` 済み）。

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
（月は日本時間で切る）。Release は版を上げた Cargo.toml と Cargo.lock をコミットし、タグを切る。その後、5 ターゲットの
バイナリと、タグのソースから作った配布辞書（`dict-build.yml` を呼ぶ）をリリースに添付する。

`dry_run` にチェックを入れて動かすと、次の版を計算して表示するだけで、コミット・タグ・ビルド・リリースはしない。

辞書を変える PR では、Build Dictionaries（`.github/workflows/dict-build.yml`）をブランチで動かす。リリースと同じ手順で
作った辞書を artifact で受け取れる（手順は [dictionaries.md](dictionaries.md) の「辞書のローカルビルド」）。
