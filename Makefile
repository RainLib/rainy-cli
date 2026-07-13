SHELL := /bin/sh

CARGO ?= cargo
PYTHON ?= python3
RAINY_BIN ?= target/debug/rainy

PROJECT ?= demo-saas
PACKAGE ?= com.example.demo
CAPABILITY ?= minio-file-storage
PROVIDER ?= minio
PROFILE ?= local
PLAN ?= plans/$(CAPABILITY).json

.DEFAULT_GOAL := help

.PHONY: help
help:
	@printf '%s\n' 'Rainy CLI maintenance targets'
	@printf '%s\n' ''
	@printf '%s\n' 'Build / install:'
	@printf '%s\n' '  make build              Build debug binary'
	@printf '%s\n' '  make release            Build release binary'
	@printf '%s\n' '  make install            Install rainy via cargo install --path'
	@printf '%s\n' '  make install-script     Install from GitHub Release via scripts/install.sh'
	@printf '%s\n' '  make uninstall          Uninstall rainy-cli cargo package'
	@printf '%s\n' ''
	@printf '%s\n' 'Quality gates:'
	@printf '%s\n' '  make fmt                Format Rust code'
	@printf '%s\n' '  make fmt-check          Check Rust formatting'
	@printf '%s\n' '  make test               Run all workspace tests'
	@printf '%s\n' '  make e2e                Run E2E tests'
	@printf '%s\n' '  make clippy             Run clippy with warnings denied'
	@printf '%s\n' '  make check              fmt-check + test + clippy'
	@printf '%s\n' '  make ci                 Full local CI smoke'
	@printf '%s\n' '  make release-check      Local checks before tagging a GitHub Release'
	@printf '%s\n' '  make production-check   Alias for release-check'
	@printf '%s\n' '  make repo-check         Check repository metadata and stale release URLs'
	@printf '%s\n' '  make security-check     Run cargo audit/deny when installed'
	@printf '%s\n' ''
	@printf '%s\n' 'Protocol / integration checks:'
	@printf '%s\n' '  make schema-check       Parse all schema JSON files'
	@printf '%s\n' '  make conformance        Check community packs conformance'
	@printf '%s\n' '  make mcp-check          Python compile-check MCP wrapper'
	@printf '%s\n' '  make installer-check    Syntax-check installer scripts where possible'
	@printf '%s\n' '  make installer-test     Run installer platform/checksum tests'
	@printf '%s\n' '  make release-input-test Validate release tag/version gates'
	@printf '%s\n' '  make smoke              JSON smoke commands'
	@printf '%s\n' ''
	@printf '%s\n' 'Demo project:'
	@printf '%s\n' '  make demo-dry-run       Preview Golden Path project creation'
	@printf '%s\n' '  make demo               Create $(PROJECT)'
	@printf '%s\n' '  make demo-add-plan      Write capability plan inside $(PROJECT)'
	@printf '%s\n' '  make demo-add-dry-run   Preview adding capability to $(PROJECT)'
	@printf '%s\n' '  make demo-add-apply     Apply capability to $(PROJECT)'
	@printf '%s\n' '  make demo-doctor        Run doctor for $(PROJECT)'
	@printf '%s\n' '  make demo-verify        Run verify for $(PROJECT)'
	@printf '%s\n' '  make demo-evidence      Generate evidence for $(PROJECT)'
	@printf '%s\n' '  make clean-demo         Remove $(PROJECT)'
	@printf '%s\n' ''
	@printf '%s\n' 'Overrides: PROJECT=demo-saas PACKAGE=com.example.demo CAPABILITY=minio-file-storage PROVIDER=minio PROFILE=local'

.PHONY: build
build:
	$(CARGO) build

.PHONY: release
release:
	$(CARGO) build --release

.PHONY: install
install:
	$(CARGO) install --path crates/rainy-cli

.PHONY: install-script
install-script:
	sh scripts/install.sh

.PHONY: uninstall
uninstall:
	$(CARGO) uninstall rainy-cli

.PHONY: fmt
fmt:
	$(CARGO) fmt

.PHONY: fmt-check
fmt-check:
	$(CARGO) fmt --check

.PHONY: test
test:
	$(CARGO) test --workspace

.PHONY: e2e
e2e:
	$(CARGO) test --workspace --test e2e

.PHONY: clippy
clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

.PHONY: check
check: fmt-check test clippy

.PHONY: schema-check
schema-check:
	$(PYTHON) -c "import json, pathlib; [json.loads(path.read_text()) for path in sorted(pathlib.Path('schemas').glob('*.schema.json'))]"

.PHONY: conformance
conformance: build
	$(RAINY_BIN) conformance check --path community-packs --json

.PHONY: mcp-check
mcp-check: build
	$(PYTHON) -m py_compile integrations/mcp/rainy_mcp.py
	RAINY_BIN=$(RAINY_BIN) sh scripts/test-mcp.sh

.PHONY: installer-check
installer-check:
	sh -n scripts/install.sh
	sh -n scripts/test-install.sh
	sh -n scripts/check-release-version.sh
	sh -n scripts/test-release.sh
	$(PYTHON) -m py_compile scripts/test-installer-server.py
	@if command -v pwsh >/dev/null 2>&1; then pwsh -NoProfile -Command '$$errors = $$null; [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "scripts/install.ps1"), [ref]$$null, [ref]$$errors); [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "scripts/test-install.ps1"), [ref]$$null, [ref]$$errors); if ($$errors.Count) { $$errors | ForEach-Object { Write-Error $$_ }; exit 1 }'; else printf '%s\n' 'pwsh not found; skipping PowerShell syntax check'; fi

.PHONY: installer-test
installer-test:
	sh scripts/test-install.sh

.PHONY: release-input-test
release-input-test:
	sh scripts/test-release.sh

.PHONY: smoke
smoke: build
	$(RAINY_BIN) capability list --json
	$(RAINY_BIN) new $(PROJECT) --golden-path spring-nextjs-saas --package $(PACKAGE) --dry-run --json
	$(RAINY_BIN) conformance check --path community-packs --json

.PHONY: repo-check
repo-check:
	@! git grep -n 'rainy-dev/rainy' -- Cargo.toml README.md docs scripts crates integrations .github
	@git grep -n 'RainLib/rainy-cli' -- Cargo.toml README.md scripts/install.sh scripts/install.ps1 >/dev/null

.PHONY: security-check
security-check:
	@if command -v cargo-audit >/dev/null 2>&1; then cargo audit; else printf '%s\n' 'cargo-audit not found; security workflow installs and runs it'; fi
	@if command -v cargo-deny >/dev/null 2>&1; then cargo deny check; else printf '%s\n' 'cargo-deny not found; security workflow installs and runs it'; fi

.PHONY: ci
ci: fmt-check test clippy schema-check mcp-check installer-check installer-test release-input-test smoke repo-check

.PHONY: release-check
release-check: ci

.PHONY: production-check
production-check: release-check

.PHONY: demo-dry-run
demo-dry-run: build
	$(RAINY_BIN) new $(PROJECT) --golden-path spring-nextjs-saas --package $(PACKAGE) --dry-run --json

.PHONY: demo
demo: build
	$(RAINY_BIN) new $(PROJECT) --golden-path spring-nextjs-saas --package $(PACKAGE)

.PHONY: demo-add-plan
demo-add-plan: build
	cd $(PROJECT) && ../$(RAINY_BIN) add capability $(CAPABILITY) --provider $(PROVIDER) --output-plan $(PLAN)

.PHONY: demo-add-dry-run
demo-add-dry-run: build
	cd $(PROJECT) && ../$(RAINY_BIN) add capability $(CAPABILITY) --provider $(PROVIDER) --dry-run

.PHONY: demo-add-apply
demo-add-apply: build
	cd $(PROJECT) && ../$(RAINY_BIN) add capability $(CAPABILITY) --provider $(PROVIDER) --apply

.PHONY: demo-doctor
demo-doctor: build
	cd $(PROJECT) && ../$(RAINY_BIN) doctor

.PHONY: demo-verify
demo-verify: build
	cd $(PROJECT) && ../$(RAINY_BIN) verify --profile $(PROFILE)

.PHONY: demo-evidence
demo-evidence: build
	cd $(PROJECT) && ../$(RAINY_BIN) evidence generate

.PHONY: clean-demo
clean-demo:
	rm -rf $(PROJECT)

.PHONY: clean
clean:
	$(CARGO) clean
	rm -rf integrations/mcp/__pycache__
