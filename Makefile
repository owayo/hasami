.PHONY: build release install clean test fmt check help setup-hooks \
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

check: ## Run clippy and check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo check --workspace

setup-hooks: ## Enable repository hooks and Git LFS for this clone
	git config core.hooksPath .githooks
	git lfs install --local

clean: ## Clean build artifacts
	cargo clean

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

dict-repair: release ## Repair a dictionary in place (DICT=path/to/dict.hsd)
	@test -n "$(DICT)" || { echo "usage: make dict-repair DICT=dict/xxx.hsd"; exit 1; }
	$(HASAMI) repair --dict $(DICT) \
		--drop-invalid-context-ids \
		--drop-ortho-variants \
		--drop-numeral-misreadings \
		$(foreach f,$(wildcard $(DICT_OUT)/user-remove/*.csv),--remove $(f))

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
	@echo "Dictionary files are output to $(DICT_OUT)/:"
	@echo "  ipadic.hsd                  IPAdic single"
	@echo "  ipadic-neologd.hsd          IPAdic + NEologd"
	@echo "  ipadic-neologd-sudachi.hsd  IPAdic + NEologd + SudachiDict (recommended)"
	@echo "  unidic-cwj.hsd              UniDic CWJ (書き言葉, not distributed)"
	@echo "  unidic-csj.hsd              UniDic CSJ (話し言葉, not distributed)"
	@echo ""
	@echo "Release:"
	@echo "  Use GitHub Actions > Release > Run workflow"
