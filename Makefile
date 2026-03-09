.PHONY: build release install clean test fmt check help \
       dict dict-ipadic dict-neologd dict-unidic-cwj dict-unidic-csj dict-clean \
       dict-download-ipadic dict-download-neologd dict-download-unidic-cwj dict-download-unidic-csj

# Default target
.DEFAULT_GOAL := help

# Variables
BINARY_NAME := hasami
INSTALL_PATH := /usr/local/bin
HASAMI := ./target/release/$(BINARY_NAME)

# Dictionary build variables
DICT_SRC := .dict-src
DICT_OUT := dict

IPADIC_REPO := https://github.com/taku910/mecab.git
IPADIC_DIR := $(DICT_SRC)/mecab/mecab-ipadic

NEOLOGD_REPO := https://github.com/neologd/mecab-ipadic-neologd.git
NEOLOGD_DIR := $(DICT_SRC)/mecab-ipadic-neologd
NEOLOGD_EXCLUDE := \
	neologd-adjective-exp-dict-seed.20151126.csv \
	neologd-date-time-infreq-dict-seed.20190415.csv \
	neologd-quantity-infreq-dict-seed.20190415.csv

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

test: ## Run tests
	cargo test

fmt: ## Format code
	cargo fmt

check: ## Run clippy and check
	cargo clippy -- -D warnings
	cargo check

clean: ## Clean build artifacts
	cargo clean

## Dictionary Build

dict: dict-ipadic dict-neologd dict-unidic-cwj dict-unidic-csj ## Build all dictionaries

dict-ipadic: release dict-download-ipadic ## Build IPAdic dictionary
	@mkdir -p $(DICT_OUT)
	$(HASAMI) build --input $(IPADIC_DIR) --output $(DICT_OUT)/ipadic.hsd

dict-neologd: dict-ipadic dict-download-neologd ## Build IPAdic + NEologd dictionary
	@mkdir -p $(DICT_SRC)/neologd-seed
	@if command -v xz >/dev/null 2>&1; then \
		xz -dkf $(NEOLOGD_DIR)/seed/*.csv.xz 2>/dev/null || true; \
	else \
		python3 -c "import lzma,glob,os; [open(f[:-3],'wb').write(lzma.open(f).read()) for f in glob.glob('$(NEOLOGD_DIR)/seed/*.csv.xz') if not os.path.exists(f[:-3])]"; \
	fi
	@for f in $(NEOLOGD_DIR)/seed/*.csv; do \
		base=$$(basename "$$f"); \
		skip=false; \
		for ex in $(NEOLOGD_EXCLUDE); do \
			if [ "$$base" = "$$ex" ]; then skip=true; break; fi; \
		done; \
		if [ "$$skip" = "false" ]; then cp "$$f" $(DICT_SRC)/neologd-seed/; fi; \
	done
	$(HASAMI) merge \
		--dict $(DICT_OUT)/ipadic.hsd \
		--input $(DICT_SRC)/neologd-seed \
		--output $(DICT_OUT)/ipadic-neologd.hsd

dict-unidic-cwj: release dict-download-unidic-cwj ## Build UniDic CWJ (書き言葉) dictionary
	@mkdir -p $(DICT_SRC)/unidic-cwj-converted $(DICT_OUT)
	python3 scripts/convert-unidic-csv.py $(UNIDIC_CWJ_DIR) $(DICT_SRC)/unidic-cwj-converted
	@cp $(UNIDIC_CWJ_DIR)/matrix.def $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/char.def   $(DICT_SRC)/unidic-cwj-converted/
	@cp $(UNIDIC_CWJ_DIR)/unk.def    $(DICT_SRC)/unidic-cwj-converted/
	$(HASAMI) build --input $(DICT_SRC)/unidic-cwj-converted --output $(DICT_OUT)/unidic-cwj.hsd

dict-unidic-csj: release dict-download-unidic-csj ## Build UniDic CSJ (話し言葉) dictionary
	@mkdir -p $(DICT_SRC)/unidic-csj-converted $(DICT_OUT)
	python3 scripts/convert-unidic-csv.py $(UNIDIC_CSJ_DIR) $(DICT_SRC)/unidic-csj-converted
	@cp $(UNIDIC_CSJ_DIR)/matrix.def $(DICT_SRC)/unidic-csj-converted/
	@cp $(UNIDIC_CSJ_DIR)/char.def   $(DICT_SRC)/unidic-csj-converted/
	@cp $(UNIDIC_CSJ_DIR)/unk.def    $(DICT_SRC)/unidic-csj-converted/
	$(HASAMI) build --input $(DICT_SRC)/unidic-csj-converted --output $(DICT_OUT)/unidic-csj.hsd

dict-download-ipadic:
	@if [ ! -d "$(IPADIC_DIR)" ]; then \
		echo "Downloading IPAdic from taku910/mecab..."; \
		mkdir -p $(DICT_SRC); \
		git clone --depth 1 --filter=blob:none --sparse $(IPADIC_REPO) $(DICT_SRC)/mecab; \
		cd $(DICT_SRC)/mecab && git sparse-checkout set mecab-ipadic; \
	else \
		echo "IPAdic already downloaded: $(IPADIC_DIR)"; \
	fi

dict-download-neologd:
	@if [ ! -d "$(NEOLOGD_DIR)" ]; then \
		echo "Downloading NEologd seed..."; \
		git clone --depth 1 --filter=blob:none --sparse $(NEOLOGD_REPO) $(NEOLOGD_DIR); \
		cd $(NEOLOGD_DIR) && git sparse-checkout set seed; \
	else \
		echo "NEologd already downloaded: $(NEOLOGD_DIR)"; \
	fi

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
	@echo "  ipadic.hsd          IPAdic single"
	@echo "  ipadic-neologd.hsd  IPAdic + NEologd (recommended)"
	@echo "  unidic-cwj.hsd      UniDic CWJ (書き言葉)"
	@echo "  unidic-csj.hsd      UniDic CSJ (話し言葉)"
	@echo ""
	@echo "Release:"
	@echo "  Use GitHub Actions > Release > Run workflow"
