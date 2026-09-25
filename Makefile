.PHONY: build release install clean test fmt check help setup-hooks dict-download \
       dict dict-ipadic dict-neologd dict-sudachi dict-repair dict-unidic-cwj dict-unidic-csj dict-clean \
       dict-download-unidic-cwj dict-download-unidic-csj

# Default target
.DEFAULT_GOAL := help

# Variables
BINARY_NAME := hasami
INSTALL_PATH := /usr/local/bin
HASAMI := ./target/release/$(BINARY_NAME)

# Dictionary build variables
DICT_SRC := .dict-src
DICT_OUT := dict

UNIDIC_VERSION := 202512
UNIDIC_CWJ_URL := https://unidic.ninjal.ac.jp/unidic_archive/2512/unidic-cwj-$(UNIDIC_VERSION).zip
UNIDIC_CSJ_URL := https://unidic.ninjal.ac.jp/unidic_archive/2512/unidic-csj-$(UNIDIC_VERSION).zip
UNIDIC_CWJ_DIR := $(DICT_SRC)/unidic-cwj-$(UNIDIC_VERSION)
UNIDIC_CSJ_DIR := $(DICT_SRC)/unidic-csj-$(UNIDIC_VERSION)

## Build Commands

build: ## Build debug version
	cargo build

release: ## Build release version
	cargo build --release

## Installation

install: release ## Build release and install to /usr/local/bin
	cp target/release/$(BINARY_NAME) $(INSTALL_PATH)/

## Development

# hasami-python は pyo3/extension-module のため macOS/Linux ではテストバイナリの
# リンクに失敗する。コンパイル検証は check (clippy --workspace) でカバーする。
test: ## Run tests
	cargo test --workspace --exclude hasami-python

fmt: ## Format code
	cargo fmt --all

# ライブラリとして使う 3 つの構成も CI と同じく確かめる。文分割だけ (feature なし。依存なし)、
# 解析まで (analyzer。依存は memmap2 と bytemuck)、配布辞書の取得まで (download。analyzer に
# HTTP・TLS・SHA-256・zstd の展開を足す)
check: ## Run clippy and check (incl. the library-only feature sets)
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy --lib --no-default-features -- -D warnings
	cargo test --lib --no-default-features
	cargo clippy --lib --no-default-features --features analyzer -- -D warnings
	cargo test --lib --no-default-features --features analyzer
	cargo clippy --lib --no-default-features --features download -- -D warnings
	cargo test --lib --no-default-features --features download
	cargo check --workspace

setup-hooks: ## Enable repository hooks for this clone
	git config core.hooksPath .githooks

clean: ## Clean build artifacts
	cargo clean

## Dictionary Download

# 配布辞書はリポジトリに置かず、リリースに添付する。この版 (Cargo.toml の version) のリリースから
# 3 つとも dict/ に取る (展開後の SHA-256 を目録と照らす)
dict-download: release ## Download the distributed dictionaries of this version's release into dict/
	$(HASAMI) dict download --dir $(DICT_OUT) --all

## Dictionary Build

# 配布辞書 (ipadic / ipadic-neologd / ipadic-neologd-sudachi) は scripts/build-dict.sh が
# 上流の固定版から作る。上流の版・取得・repair の手順はスクリプトにまとめてある
BUILD_DICT := scripts/build-dict.sh --hasami $(HASAMI) --src $(DICT_SRC) --out $(DICT_OUT)

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
	python3 scripts/convert-unidic-csv.py $(UNIDIC_CWJ_DIR) $(DICT_SRC)/unidic-cwj-converted
	@cp $(UNIDIC_CWJ_DIR)/matrix.def $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/char.def   $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/unk.def    $(DICT_SRC)/unidic-cwj-converted/
	$(HASAMI) build --input $(DICT_SRC)/unidic-cwj-converted --output $(DICT_OUT)/unidic-cwj.hsd \
		--meta name=unidic-cwj --meta pos_scheme=unidic --meta sources=unidic-cwj@$(UNIDIC_VERSION)

dict-unidic-csj: release dict-download-unidic-csj ## Build UniDic CSJ (話し言葉) dictionary
	@mkdir -p $(DICT_SRC)/unidic-csj-converted $(DICT_OUT)
	python3 scripts/convert-unidic-csv.py $(UNIDIC_CSJ_DIR) $(DICT_SRC)/unidic-csj-converted
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
	@echo "$(BINARY_NAME) Build Commands"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "Dictionaries in $(DICT_OUT)/ (not in the repository; make dict-download to download, make dict to build):"
	@echo "  ipadic.hsd                  IPAdic single"
	@echo "  ipadic-neologd.hsd          IPAdic + NEologd"
	@echo "  ipadic-neologd-sudachi.hsd  IPAdic + NEologd + SudachiDict (recommended)"
	@echo "  unidic-cwj.hsd              UniDic CWJ (書き言葉, not distributed)"
	@echo "  unidic-csj.hsd              UniDic CSJ (話し言葉, not distributed)"
	@echo ""
	@echo "Release:"
	@echo "  Use GitHub Actions > Release > Run workflow (attaches the binaries and the dictionaries)"
