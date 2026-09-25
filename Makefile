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

# Default target
.DEFAULT_GOAL := help

# Variables
BINARY_NAME := hasami
INSTALL_PATH ?= /usr/local/bin
# Cargo.lock をコミットしているので、依存の解決結果を CI とそろえる (lockfile の更新が要るなら失敗させる)
CARGO_FLAGS ?= --locked
HASAMI := ./target/release/$(BINARY_NAME)

# Dictionary build variables
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
$(error mise not found. Install it from https://mise.jdx.dev, or add SYSTEM_TOOLS=1 to use the tools on PATH)
endif
endif
RUN := $(if $(MISE),$(MISE) exec --,)
endif

.PHONY: help setup setup-hooks build release run test lint fmt fmt-check check ci install uninstall clean \
       dict-download dict dict-ipadic dict-neologd dict-sudachi dict-repair dict-unidic-cwj dict-unidic-csj \
       dict-download-unidic-cwj dict-download-unidic-csj dict-clean

## Setup

setup: ## Install the toolchain (mise.toml) and fetch the dependencies (Cargo.lock)
	@if [ -n "$(MISE)" ]; then "$(MISE)" install; fi
	$(RUN) cargo fetch $(CARGO_FLAGS)

setup-hooks: ## Enable repository hooks for this clone
	git config core.hooksPath .githooks

## Build Commands

build: ## Build debug version
	$(RUN) cargo build $(CARGO_FLAGS)

release: ## Build release version
	$(RUN) cargo build --release $(CARGO_FLAGS)

run: ## Run the CLI (arguments in ARGS="...")
	$(RUN) cargo run $(CARGO_FLAGS) -- $(ARGS)

## Checks

# ライブラリとして使う 3 つの構成も確かめる。文分割だけ (feature なし。依存なし)、
# 解析まで (analyzer。依存は memmap2 と bytemuck)、配布辞書の取得まで (download。analyzer に
# HTTP・TLS・SHA-256・zstd の展開を足す)
#
# hasami-python は pyo3/extension-module のため macOS/Linux ではテストバイナリの
# リンクに失敗する。コンパイルは lint (clippy --workspace) で確かめる。
test: ## Run tests (the workspace and the library-only feature sets)
	$(RUN) cargo test $(CARGO_FLAGS) --workspace --exclude hasami-python
	$(RUN) cargo test $(CARGO_FLAGS) --lib --no-default-features
	$(RUN) cargo test $(CARGO_FLAGS) --lib --no-default-features --features analyzer
	$(RUN) cargo test $(CARGO_FLAGS) --lib --no-default-features --features download

lint: ## Run clippy with warnings denied (the workspace and the library-only feature sets)
	$(RUN) cargo clippy $(CARGO_FLAGS) --workspace --all-targets -- -D warnings
	$(RUN) cargo clippy $(CARGO_FLAGS) --lib --no-default-features -- -D warnings
	$(RUN) cargo clippy $(CARGO_FLAGS) --lib --no-default-features --features analyzer -- -D warnings
	$(RUN) cargo clippy $(CARGO_FLAGS) --lib --no-default-features --features download -- -D warnings

fmt: ## Format code
	$(RUN) cargo fmt --all

fmt-check: ## Check formatting (does not rewrite)
	$(RUN) cargo fmt --all -- --check

check: fmt-check lint ## Check formatting and run clippy (no tests)

ci: check test ## Run the same checks as the CI quality job (check + test)

## Installation

# 上書きコピーではなく一時ファイル + rename で置き換える。macOS はコード署名の
# 検証結果を inode 単位でキャッシュするため、実行中や直前に実行したバイナリへ cp で
# 上書きすると、新しいバイナリが起動直後に SIGKILL される (exit 137)。
# 一時ファイルは rename が inode の差し替えになるよう、同じディレクトリに置く。
install: release ## Build release and install to INSTALL_PATH (default /usr/local/bin)
	@mkdir -p "$(INSTALL_PATH)"
	cp "target/release/$(BINARY_NAME)" "$(INSTALL_PATH)/$(BINARY_NAME).new"
	mv -f "$(INSTALL_PATH)/$(BINARY_NAME).new" "$(INSTALL_PATH)/$(BINARY_NAME)"

uninstall: ## Remove the installed binary from INSTALL_PATH
	rm -f "$(INSTALL_PATH)/$(BINARY_NAME)"

clean: ## Clean build artifacts
	$(RUN) cargo clean

## Dictionary Download

# 配布辞書はリポジトリに置かず、リリースに添付する。この版 (Cargo.toml の version) のリリースから
# 3 つとも dict/ に取る (展開後の SHA-256 を目録と照らす)
dict-download: release ## Download the distributed dictionaries of this version's release into dict/
	$(HASAMI) dict download --dir $(DICT_OUT) --all

## Dictionary Build

# 配布辞書 (ipadic / ipadic-neologd / ipadic-neologd-sudachi) は scripts/build-dict.sh が
# 上流の固定版から作る。上流の版・取得・repair の手順はスクリプトにまとめてある。
# スクリプトが呼ぶ python3 も mise.toml の版にするため、$(RUN) で包む
BUILD_DICT := $(RUN) scripts/build-dict.sh --hasami $(HASAMI) --src $(DICT_SRC) --out $(DICT_OUT)

dict: release ## Build the distributed dictionaries (IPAdic, +NEologd, +SudachiDict)
	$(BUILD_DICT)

dict-ipadic: release ## Build IPAdic dictionary
	$(BUILD_DICT) ipadic

dict-neologd: release ## Build IPAdic + NEologd dictionary
	$(BUILD_DICT) neologd

dict-sudachi: release ## Build IPAdic + NEologd + SudachiDict dictionary (recommended)
	$(BUILD_DICT) sudachi

# 配布辞書をその場で直す。scripts/build-dict.sh の repair 一式から dict/user の追加だけを除いたもの
# (配布辞書には追加済みなので、足し直すと重複する)。文や句・数と単位の組の名詞の削除と降格の参照には
# IPAdic 単体の配布辞書を使う
dict-repair: release ## Repair a dictionary in place (DICT=path/to/dict.hsd)
	@test -n "$(DICT)" || { echo "usage: make dict-repair DICT=dict/xxx.hsd"; exit 1; }
	$(HASAMI) repair --dict $(DICT) \
		--drop-invalid-context-ids \
		--drop-ortho-variants \
		--drop-numeral-misreadings \
		$(foreach f,$(wildcard $(DICT_OUT)/user-remove/*.csv),--remove $(f)) \
		--drop-sentence-like-nouns $(DICT_OUT)/ipadic.hsd \
		--drop-quantity-nouns $(DICT_OUT)/ipadic.hsd \
		--demote-common-proper-nouns $(DICT_OUT)/ipadic.hsd

dict-unidic-cwj: release dict-download-unidic-cwj ## Build UniDic CWJ (書き言葉) dictionary
	@mkdir -p $(DICT_SRC)/unidic-cwj-converted $(DICT_OUT)
	$(RUN) python3 scripts/convert-unidic-csv.py $(UNIDIC_CWJ_DIR) $(DICT_SRC)/unidic-cwj-converted
	@cp $(UNIDIC_CWJ_DIR)/matrix.def $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/char.def   $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/unk.def    $(DICT_SRC)/unidic-cwj-converted/
	$(HASAMI) build --input $(DICT_SRC)/unidic-cwj-converted --output $(DICT_OUT)/unidic-cwj.hsd \
		--meta name=unidic-cwj --meta pos_scheme=unidic --meta sources=unidic-cwj@$(UNIDIC_VERSION)

dict-unidic-csj: release dict-download-unidic-csj ## Build UniDic CSJ (話し言葉) dictionary
	@mkdir -p $(DICT_SRC)/unidic-csj-converted $(DICT_OUT)
	$(RUN) python3 scripts/convert-unidic-csv.py $(UNIDIC_CSJ_DIR) $(DICT_SRC)/unidic-csj-converted
	@cp $(UNIDIC_CSJ_DIR)/matrix.def $(DICT_SRC)/unidic-csj-converted/
	@cp $(UNIDIC_CSJ_DIR)/char.def   $(DICT_SRC)/unidic-csj-converted/
	@cp $(UNIDIC_CSJ_DIR)/unk.def    $(DICT_SRC)/unidic-csj-converted/
	$(HASAMI) build --input $(DICT_SRC)/unidic-csj-converted --output $(DICT_OUT)/unidic-csj.hsd \
		--meta name=unidic-csj --meta pos_scheme=unidic --meta sources=unidic-csj@$(UNIDIC_VERSION)

dict-download-unidic-cwj:
	@if [ ! -d "$(UNIDIC_CWJ_DIR)" ]; then \
		echo "Downloading UniDic CWJ $(UNIDIC_VERSION) (書き言葉)..."; \
		mkdir -p $(DICT_SRC); \
		curl -fL -o $(DICT_SRC)/unidic-cwj.zip '$(UNIDIC_CWJ_URL)'; \
		unzip -q $(DICT_SRC)/unidic-cwj.zip -d $(DICT_SRC); \
	else \
		echo "UniDic CWJ already downloaded: $(UNIDIC_CWJ_DIR)"; \
	fi

dict-download-unidic-csj:
	@if [ ! -d "$(UNIDIC_CSJ_DIR)" ]; then \
		echo "Downloading UniDic CSJ $(UNIDIC_VERSION) (話し言葉)..."; \
		mkdir -p $(DICT_SRC); \
		curl -fL -o $(DICT_SRC)/unidic-csj.zip '$(UNIDIC_CSJ_URL)'; \
		unzip -q $(DICT_SRC)/unidic-csj.zip -d $(DICT_SRC); \
	else \
		echo "UniDic CSJ already downloaded: $(UNIDIC_CSJ_DIR)"; \
	fi

dict-clean: ## Remove downloaded dictionary sources
	rm -rf $(DICT_SRC)

## Help

help: ## Show this help message
	@echo "$(BINARY_NAME) development tasks"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "Dictionaries in $(DICT_OUT)/ (not in the repository; make dict-download to download, make dict to build):"
	@echo "  ipadic.hsd                  IPAdic single"
	@echo "  ipadic-neologd.hsd          IPAdic + NEologd"
	@echo "  ipadic-neologd-sudachi.hsd  IPAdic + NEologd + SudachiDict (recommended)"
	@echo "  unidic-cwj.hsd              UniDic CWJ (書き言葉, not distributed)"
	@echo "  unidic-csj.hsd              UniDic CSJ (話し言葉, not distributed)"
	@echo ""
	@echo "Tool versions are pinned in mise.toml. Run make setup first"
	@echo "(SYSTEM_TOOLS=1 uses the tools on PATH instead of mise)."
	@echo ""
	@echo "Release:"
	@echo "  Use GitHub Actions > Release > Run workflow (attaches the binaries and the dictionaries)"
