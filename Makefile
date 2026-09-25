# hasami の開発用タスク。引数なしの `make` でターゲット一覧を表示する。
#
# ツールの版は mise.toml が正。mise があればコマンドを `mise exec --` 経由で呼ぶので、
# シェルで mise を activate していなくても (IDE や GUI から make を呼んでも) mise.toml の版で動く。
# mise を使わず PATH 上のツールで動かすなら SYSTEM_TOOLS=1 を付ける (その場合、版の再現性は保証しない)。
#
# CI の quality ジョブは make setup と make ci だけを呼ぶ。検査の中身はここの ci が定義する。
#
# macOS 標準の GNU Make 3.81 で動く書き方に限っている
# (.ONESHELL / .SHELLFLAGS / $(file ...) / != は使わない)。

.DEFAULT_GOAL := help

BINARY_NAME := hasami
INSTALL_PATH ?= /usr/local/bin
# Cargo.lock をコミットしているので、依存の解決結果を CI とそろえる (lockfile の更新が要るなら失敗させる)
CARGO_FLAGS ?= --locked
HASAMI := ./target/release/$(BINARY_NAME)

# 辞書のソースの置き場所と、辞書の書き出し先
DICT_SRC := .dict-src
DICT_OUT := dict

UNIDIC_VERSION := 202512
UNIDIC_CWJ_URL := https://unidic.ninjal.ac.jp/unidic_archive/2512/unidic-cwj-$(UNIDIC_VERSION).zip
UNIDIC_CSJ_URL := https://unidic.ninjal.ac.jp/unidic_archive/2512/unidic-csj-$(UNIDIC_VERSION).zip
UNIDIC_CWJ_DIR := $(DICT_SRC)/unidic-cwj-$(UNIDIC_VERSION)
UNIDIC_CSJ_DIR := $(DICT_SRC)/unidic-csj-$(UNIDIC_VERSION)

# ---- ツールチェーン -----------------------------------------------------------
# mise は PATH、よくある導入先の順に探す。GUI から起動した make はシェルの PATH を
# 引き継がないことがあるため。make MISE=/path/to/mise で明示もできる。
# mise が無い環境の振る舞いを試すときは MISE_CANDIDATES= で探す先を空にする。
MISE_CANDIDATES ?= $(HOME)/.local/bin/mise /opt/homebrew/bin/mise /usr/local/bin/mise
ifeq ($(SYSTEM_TOOLS),1)
RUN :=
else
ifndef MISE
MISE := $(firstword $(shell command -v mise 2>/dev/null) $(wildcard $(MISE_CANDIDATES)))
endif
ifeq ($(MISE),)
ifneq ($(filter-out help,$(or $(MAKECMDGOALS),help)),)
$(error mise が見つかりません。https://mise.jdx.dev で導入するか、PATH 上のツールで実行するなら SYSTEM_TOOLS=1 を付けてください)
endif
endif
RUN := $(if $(MISE),$(MISE) exec --,)
endif

.PHONY: help setup setup-hooks build release run test lint fmt fmt-check check ci install uninstall clean \
       dict-download dict dict-ipadic dict-neologd dict-sudachi dict-repair dict-unidic-cwj dict-unidic-csj \
       dict-download-unidic-cwj dict-download-unidic-csj dict-clean

## セットアップ

setup: ## ツールチェーン (mise) と依存を取得する
	@if [ -n "$(MISE)" ]; then "$(MISE)" install; fi
	$(RUN) cargo fetch $(CARGO_FLAGS)

# pre-commit は 50MB を超えるファイルのコミットを止める (配布辞書をコミットしないため)
setup-hooks: ## この clone でリポジトリのフック (.githooks) を有効にする
	git config core.hooksPath .githooks

## ビルド

build: ## デバッグ版をビルドする
	$(RUN) cargo build $(CARGO_FLAGS)

release: ## リリース版をビルドする
	$(RUN) cargo build --release $(CARGO_FLAGS)

run: ## デバッグ版を実行する (引数は ARGS="...")
	$(RUN) cargo run $(CARGO_FLAGS) -- $(ARGS)

## 検査

# ライブラリとして使う 3 つの構成も確かめる。文分割だけ (feature なし。依存なし)、
# 解析まで (analyzer。依存は memmap2 と bytemuck)、配布辞書の取得まで (download。analyzer に
# HTTP・TLS・SHA-256・zstd の展開を足す)
#
# hasami-python は pyo3/extension-module のため macOS/Linux ではテストバイナリの
# リンクに失敗する。コンパイルは lint (clippy --workspace) で確かめる。
test: ## テストを実行する (ワークスペースと、ライブラリとして使う 3 つの feature の構成)
	$(RUN) cargo test $(CARGO_FLAGS) --workspace --exclude hasami-python
	$(RUN) cargo test $(CARGO_FLAGS) --lib --no-default-features
	$(RUN) cargo test $(CARGO_FLAGS) --lib --no-default-features --features analyzer
	$(RUN) cargo test $(CARGO_FLAGS) --lib --no-default-features --features download

lint: ## clippy を警告ゼロで通す (ワークスペースと、ライブラリとして使う 3 つの feature の構成)
	$(RUN) cargo clippy $(CARGO_FLAGS) --workspace --all-targets -- -D warnings
	$(RUN) cargo clippy $(CARGO_FLAGS) --lib --no-default-features -- -D warnings
	$(RUN) cargo clippy $(CARGO_FLAGS) --lib --no-default-features --features analyzer -- -D warnings
	$(RUN) cargo clippy $(CARGO_FLAGS) --lib --no-default-features --features download -- -D warnings

fmt: ## コードを整形する (書き換える)
	$(RUN) cargo fmt --all

fmt-check: ## 整形済みかを確かめる (書き換えない)
	$(RUN) cargo fmt --all -- --check

check: fmt-check lint ## 整形と静的検査 (書き換えない)

ci: check test ## CI と同じ検査 (書き換えない)

## インストール

# 上書きコピーではなく一時ファイル + rename で置き換える。macOS はコード署名の
# 検証結果を inode 単位でキャッシュするため、実行中や直前に実行したバイナリへ cp で
# 上書きすると、新しいバイナリが起動直後に SIGKILL される (exit 137)。
# 一時ファイルは rename が inode の差し替えになるよう、同じディレクトリに置く。
install: release ## リリース版を INSTALL_PATH (既定 /usr/local/bin) に入れる
	@mkdir -p "$(INSTALL_PATH)"
	cp "target/release/$(BINARY_NAME)" "$(INSTALL_PATH)/$(BINARY_NAME).new"
	mv -f "$(INSTALL_PATH)/$(BINARY_NAME).new" "$(INSTALL_PATH)/$(BINARY_NAME)"

uninstall: ## INSTALL_PATH から取り除く
	rm -f "$(INSTALL_PATH)/$(BINARY_NAME)"

clean: ## ビルド成果物を消す
	$(RUN) cargo clean

## 辞書の取得

# 配布辞書はリポジトリに置かず、リリースに添付する。この版 (Cargo.toml の version) のリリースから
# 3 つとも dict/ に取る (展開後の SHA-256 を目録と照らす)
dict-download: release ## この版のリリースから配布辞書 3 つを dict/ に取る
	$(HASAMI) dict download --dir $(DICT_OUT) --all

## 辞書のビルド

# 配布辞書 (ipadic / ipadic-neologd / ipadic-neologd-sudachi) は scripts/build-dict.sh が
# 上流の固定版から作る。上流の版・取得・repair の手順はスクリプトにまとめてある。
# スクリプトが呼ぶ python3 も mise.toml の版にするため、$(RUN) で包む
BUILD_DICT := $(RUN) scripts/build-dict.sh --hasami $(HASAMI) --src $(DICT_SRC) --out $(DICT_OUT)

dict: release ## 配布辞書 3 つ (IPAdic、+NEologd、+SudachiDict) を上流のソースから作る
	$(BUILD_DICT)

dict-ipadic: release ## IPAdic の辞書を作る
	$(BUILD_DICT) ipadic

dict-neologd: release ## IPAdic + NEologd の辞書を作る
	$(BUILD_DICT) neologd

dict-sudachi: release ## IPAdic + NEologd + SudachiDict の辞書を作る (推奨)
	$(BUILD_DICT) sudachi

# 配布辞書をその場で直す。scripts/build-dict.sh の repair 一式から dict/user の追加だけを除いたもの
# (配布辞書には追加済みなので、足し直すと重複する)。文や句・数と単位の組の名詞の削除と降格の参照には
# IPAdic 単体の配布辞書を使う
dict-repair: release ## 辞書をその場で修復する (DICT=path/to/dict.hsd)
	@test -n "$(DICT)" || { echo "usage: make dict-repair DICT=dict/xxx.hsd"; exit 1; }
	$(HASAMI) repair --dict $(DICT) \
		--drop-invalid-context-ids \
		--drop-ortho-variants \
		--drop-numeral-misreadings \
		$(foreach f,$(wildcard $(DICT_OUT)/user-remove/*.csv),--remove $(f)) \
		--drop-sentence-like-nouns $(DICT_OUT)/ipadic.hsd \
		--drop-quantity-nouns $(DICT_OUT)/ipadic.hsd \
		--demote-common-proper-nouns $(DICT_OUT)/ipadic.hsd

dict-unidic-cwj: release dict-download-unidic-cwj ## UniDic CWJ (書き言葉) の辞書を作る (配布しない)
	@mkdir -p $(DICT_SRC)/unidic-cwj-converted $(DICT_OUT)
	$(RUN) python3 scripts/convert-unidic-csv.py $(UNIDIC_CWJ_DIR) $(DICT_SRC)/unidic-cwj-converted
	@cp $(UNIDIC_CWJ_DIR)/matrix.def $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/char.def   $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/unk.def    $(DICT_SRC)/unidic-cwj-converted/
	$(HASAMI) build --input $(DICT_SRC)/unidic-cwj-converted --output $(DICT_OUT)/unidic-cwj.hsd \
		--meta name=unidic-cwj --meta pos_scheme=unidic --meta sources=unidic-cwj@$(UNIDIC_VERSION)

dict-unidic-csj: release dict-download-unidic-csj ## UniDic CSJ (話し言葉) の辞書を作る (配布しない)
	@mkdir -p $(DICT_SRC)/unidic-csj-converted $(DICT_OUT)
	$(RUN) python3 scripts/convert-unidic-csv.py $(UNIDIC_CSJ_DIR) $(DICT_SRC)/unidic-csj-converted
	@cp $(UNIDIC_CSJ_DIR)/matrix.def $(DICT_SRC)/unidic-csj-converted/
	@cp $(UNIDIC_CSJ_DIR)/char.def   $(DICT_SRC)/unidic-csj-converted/
	@cp $(UNIDIC_CSJ_DIR)/unk.def    $(DICT_SRC)/unidic-csj-converted/
	$(HASAMI) build --input $(DICT_SRC)/unidic-csj-converted --output $(DICT_OUT)/unidic-csj.hsd \
		--meta name=unidic-csj --meta pos_scheme=unidic --meta sources=unidic-csj@$(UNIDIC_VERSION)

dict-download-unidic-cwj: ## UniDic CWJ (書き言葉) のソースを .dict-src/ に取る
	@if [ ! -d "$(UNIDIC_CWJ_DIR)" ]; then \
		echo "Downloading UniDic CWJ $(UNIDIC_VERSION) (書き言葉)..."; \
		mkdir -p $(DICT_SRC); \
		curl -fL -o $(DICT_SRC)/unidic-cwj.zip '$(UNIDIC_CWJ_URL)'; \
		unzip -q $(DICT_SRC)/unidic-cwj.zip -d $(DICT_SRC); \
	else \
		echo "UniDic CWJ already downloaded: $(UNIDIC_CWJ_DIR)"; \
	fi

dict-download-unidic-csj: ## UniDic CSJ (話し言葉) のソースを .dict-src/ に取る
	@if [ ! -d "$(UNIDIC_CSJ_DIR)" ]; then \
		echo "Downloading UniDic CSJ $(UNIDIC_VERSION) (話し言葉)..."; \
		mkdir -p $(DICT_SRC); \
		curl -fL -o $(DICT_SRC)/unidic-csj.zip '$(UNIDIC_CSJ_URL)'; \
		unzip -q $(DICT_SRC)/unidic-csj.zip -d $(DICT_SRC); \
	else \
		echo "UniDic CSJ already downloaded: $(UNIDIC_CSJ_DIR)"; \
	fi

dict-clean: ## 取得した辞書のソース (.dict-src/) を消す
	rm -rf $(DICT_SRC)

## ヘルプ

help: ## このヘルプを表示する
	@echo "$(BINARY_NAME) の開発用タスク"
	@echo ""
	@echo "使い方: make <target>"
	@echo ""
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-26s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "$(DICT_OUT)/ の辞書 (リポジトリには置かない。make dict-download で取るか、make dict で作る):"
	@echo "  ipadic.hsd                  IPAdic 単体"
	@echo "  ipadic-neologd.hsd          IPAdic + NEologd"
	@echo "  ipadic-neologd-sudachi.hsd  IPAdic + NEologd + SudachiDict (推奨)"
	@echo "  unidic-cwj.hsd              UniDic CWJ (書き言葉。配布しない)"
	@echo "  unidic-csj.hsd              UniDic CSJ (話し言葉。配布しない)"
	@echo ""
	@echo "ツールの版は mise.toml を参照。初回は make setup"
	@echo "(SYSTEM_TOOLS=1 を付けると mise ではなく PATH 上のツールを使う)"
	@echo ""
	@echo "リリース: GitHub Actions > Release > Run workflow (バイナリと配布辞書を添付する)"
