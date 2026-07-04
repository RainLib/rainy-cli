# Rainy CLI

Rainy CLI 是一个用 Rust 实现的“软件能力编排”命令行工具。它不是单纯脚手架，也不是把企业 starter 硬编码进 CLI，而是把常见研发能力抽象成 Capability Pack，通过固定流程完成：

```text
Plan -> Diff -> Policy -> Apply -> Doctor -> Verify -> Evidence
```

它服务的目标是：让开发者、Agent、Backstage、CI 都能用同一套确定性命令发现能力、生成变更、执行策略拦截、落地代码、验证结果，并产出可进入 PR 或审计链路的证据。

## 当前工程是什么

当前仓库是 Rainy CLI 的开源核心工程，包含：

- `crates/rainy-cli`: CLI 主程序，当前以单 crate 形式实现核心能力。
- `community-packs`: 开源 Golden Path 能力包。
- `schemas`: Rainy 项目、能力包、计划、变更、报告、插件等 JSON Schema。
- `integrations/mcp`: MCP stdio wrapper 示例，供 Agent 调用 Rainy CLI。
- `integrations/backstage`: Backstage scaffolder action/template 示例。
- `docs`: 外部作者编写 capability pack 和 plugin 的说明。
- `.github/workflows/ci.yml`: 基础 CI 门禁示例。

内置 community packs 覆盖的主流研发闭环包括：

- Spring Boot backend
- Next.js frontend
- Docker Compose local
- PostgreSQL
- Redis
- MinIO file storage
- OIDC / Keycloak
- OpenAPI contract
- Dev Container
- GitHub Actions CI
- OpenTelemetry
- Helm draft

## 适合谁用

- 平台研发：沉淀公司内外通用能力，避免每个项目重复接入。
- 后端/前端研发：从标准 Golden Path 起项目，并按能力增量接入中间件、认证、文件上传、可观测等。
- Agent 平台：通过 `--json`、`--dry-run`、稳定错误码和 MCP wrapper 安全调用 CLI。
- DevOps/CI：运行 `doctor`、`verify`、`evidence`，把能力接入结果变成可检查报告。
- Backstage 门户：用 scaffolder action 调用同一套 CLI 流程。

## 快速开始

查看所有常用维护命令：

```bash
make help
```

本地构建：

```bash
make build
target/debug/rainy --help
```

本地安装到 Cargo bin：

```bash
make install
rainy --help
```

从 GitHub Release 安装预编译包：

```bash
curl -fsSL https://github.com/rainy-dev/rainy/releases/latest/download/install.sh | sh
```

Windows PowerShell：

```powershell
powershell -ExecutionPolicy Bypass -c "iwr https://github.com/rainy-dev/rainy/releases/latest/download/install.ps1 -UseB | iex"
```

安装脚本会根据当前系统下载对应的 release asset：

- Linux x86_64: `rainy-x86_64-unknown-linux-gnu.tar.gz`
- macOS Intel: `rainy-x86_64-apple-darwin.tar.gz`
- macOS Apple Silicon: `rainy-aarch64-apple-darwin.tar.gz`
- Windows x64: `rainy-x86_64-pc-windows-msvc.zip`

默认安装目录是 `~/.rainy/bin`。可以覆盖：

```bash
INSTALL_DIR=/usr/local/bin sh scripts/install.sh
RAINY_REPO=owner/repo RAINY_VERSION=v0.1.0 sh scripts/install.sh
```

从空目录创建 Golden Path 项目：

```bash
rainy new demo-saas --golden-path spring-nextjs-saas --package com.example.demo
cd demo-saas
```

查看可用能力：

```bash
rainy capability list
rainy capability explain minio-file-storage
rainy capability graph
```

先 dry-run 计划变更，再 apply：

```bash
rainy add capability minio-file-storage --provider minio --dry-run
rainy add capability minio-file-storage --provider minio --apply
```

检查和验证项目：

```bash
rainy doctor
rainy verify --profile local
rainy evidence generate
```

Agent 或 CI 使用 JSON 输出：

```bash
rainy capability list --json
rainy add capability minio-file-storage --provider minio --dry-run --json
rainy doctor --json
```

## Makefile 管理命令

仓库提供了 `Makefile` 作为常用管理入口：

```bash
make help          # 查看所有目标
make build         # 构建 debug binary
make release       # 构建 release binary
make install       # cargo install --path crates/rainy-cli
make install-script # 从 GitHub Release 安装预编译包
make uninstall     # cargo uninstall rainy-cli
make fmt           # 格式化 Rust 代码
make fmt-check     # 检查格式
make test          # 运行 workspace tests
make e2e           # 只运行 E2E tests
make clippy        # clippy 严格检查
make check         # fmt-check + test + clippy
make ci            # 本地完整 CI smoke
make schema-check  # 检查 schemas/*.schema.json 可解析
make conformance   # 检查 community-packs conformance
make mcp-check     # 编译检查 MCP Python wrapper
make installer-check # 检查安装脚本语法
make smoke         # JSON smoke commands
```

Demo 项目管理：

```bash
make demo-dry-run      # 预览创建 demo-saas，不写文件
make demo              # 创建 demo-saas
make demo-add-plan     # 在 demo-saas 中生成能力 plan
make demo-add-dry-run  # 预览添加 MinIO 能力
make demo-add-apply    # 真正添加 MinIO 能力
make demo-doctor       # 运行 doctor
make demo-verify       # 运行 verify
make demo-evidence     # 生成 evidence
make clean-demo        # 删除 demo-saas
```

常用变量可以覆盖：

```bash
make demo PROJECT=my-app PACKAGE=com.example.app
make demo-add-apply PROJECT=my-app CAPABILITY=redis PROVIDER=local
make demo-verify PROJECT=my-app PROFILE=ci
```

## 常用命令

项目初始化：

```bash
rainy init app demo-saas --preset spring-nextjs --package com.example.demo
rainy new demo-saas --golden-path spring-nextjs-saas
rainy new demo-saas --golden-path spring-nextjs-saas --dry-run --json
```

能力管理：

```bash
rainy capability list
rainy capability explain minio-file-storage
rainy capability installed
rainy capability graph
rainy capability upgrade minio-file-storage --dry-run
rainy capability remove minio-file-storage --dry-run
```

计划文件工作流：

```bash
rainy add capability minio-file-storage --provider minio --output-plan plans/minio.json
rainy apply --plan plans/minio.json --dry-run
rainy apply --plan plans/minio.json --apply
```

Pack 管理：

```bash
rainy pack list
rainy pack inspect minio-file-storage
rainy pack install ./community-packs/minio-file-storage --dry-run
rainy pack install ./community-packs/minio-file-storage --apply
rainy pack update --dry-run
rainy pack update --apply
rainy pack sign ./community-packs/minio-file-storage
rainy pack verify ./community-packs/minio-file-storage
```

Plugin 管理：

```bash
rainy plugin list
rainy plugin inspect echo
rainy plugin install ./path/to/plugin --dry-run
rainy plugin install ./path/to/plugin --apply
rainy plugin call echo write-example --dry-run
```

Schema / conformance：

```bash
rainy schema list
rainy schema validate --schema capability-pack --file community-packs/minio-file-storage/pack.yaml
rainy conformance check --path community-packs --json
```

Agent 上下文：

```bash
rainy agent init
rainy agent context
rainy skill sync
```

版本检查和更新：

```bash
rainy self check
rainy self check --json
rainy self update
rainy self skip 0.2.0
```

release 构建出来的非 debug CLI 会周期性检查 GitHub latest release，并在发现新版本时提示：

```text
Rainy CLI update available: 0.1.0 -> 0.2.0.
Run `rainy self update` to update, or `rainy self skip 0.2.0` to skip this version.
```

自动检查默认行为：

- debug 构建、CI、`--json`、`--quiet` 不会自动输出更新提示。
- 默认 24 小时检查一次。
- `RAINY_NO_UPDATE_CHECK=1` 或 `RAINY_SKIP_UPDATE_CHECK=1` 可以关闭自动检查。
- `RAINY_UPDATE_CHECK_INTERVAL_HOURS=0` 可以让每次运行都检查。
- `RAINY_UPDATE_REPO=owner/repo` 可以覆盖 GitHub release 仓库。

## 使用模型

Rainy 的核心使用方式是“先计划，再应用”：

1. `rainy add capability ... --dry-run` 生成计划、diff 和策略检查结果。
2. 人或 Agent 审阅计划。
3. `rainy add capability ... --apply` 或 `rainy apply --plan ... --apply` 写入文件。
4. CLI 在 apply 前执行 policy gate。
5. 写入失败时回滚已应用文件，避免部分落地。
6. `rainy doctor` 检查项目健康。
7. `rainy verify` 运行验证步骤。
8. `rainy evidence generate` 生成 Markdown/JSON 证据报告。

说明：

- `add capability`、`apply`、`pack install/update`、`plugin install/call` 默认是 dry-run，需要显式 `--apply` 才会写文件。
- `new` / `init app` 默认会创建项目，但支持 `--dry-run`。
- 所有命令支持全局 `--json`，方便 Agent、MCP、CI 调用。
- 策略会阻止敏感路径、危险命令、需要审批的操作和插件越权写入。

## 当前建设进度

已完成：

- Rust CLI 命令树：`new/init/add/apply/capability/pack/doctor/verify/evidence/plugin/agent/schema/conformance`。
- Golden Path 初始化：生成 Spring Boot + Next.js 基础项目、`rainy.yaml`、`capability.lock`、AGENTS.md、CI、compose、evidence 目录。
- Capability Pack 解析：本地、git cache、HTTP registry source。
- 内置 action：Maven、YAML/JSON/JSONC/TOML merge、模板渲染、文件创建/追加、Docker Compose、package.json、devcontainer、Helm draft 等。
- Plan / Diff / Apply：支持 dry-run、plan file、事务式 apply 回滚、幂等 no-op。
- Capability 依赖和 provider 解析：依赖缺失失败、被依赖能力禁止删除、provider 默认/显式/非法场景有稳定错误。
- Policy Gate：内置敏感路径、项目 policy、org policy、capability policy、审批动作、危险命令、插件写权限。
- Doctor / Verify / Evidence：健康检查、能力验证、证据报告、secret 脱敏、默认开发 secret warning。
- Audit log：成功和失败命令会写 `.rainy/audit.log`。
- Plugin：外部 `rainy-*` 命令、HTTP adapter、Wasm action plugin、manifest 权限、重名 warning、禁止覆盖内置命令。
- Release 安装和自更新：GitHub Actions 多平台 release 构建、`install.sh` / `install.ps1` 安装脚本、`rainy self check/update/skip`。
- MCP 示例：stdio JSON-RPC wrapper 调用 Rainy CLI，默认 dry-run 计划能力接入。
- Backstage 示例：scaffolder actions 和模板示例。
- Schema / conformance：schema list/validate、pack/plugin conformance 检查。
- 测试：当前有 1 个 unit test 和 23 个 E2E tests，覆盖 Golden Path、policy、plugin、schema、conformance、事务回滚等主流程。
- CI 示例：格式、测试、E2E、clippy、JSON smoke、conformance。

部分完成 / 示例级：

- Backstage 集成目前是示例代码，未打包成可直接发布的 Backstage npm 包。
- MCP wrapper 是最小可运行示例，生产环境还需要接入具体 MCP host 配置、权限边界和部署方式。
- `verify` 会在本地工具链缺失时对部分外部命令给 warning，而不是强制安装 Maven、Node、Docker 等。
- pack signing 是 sha256 签名/校验初版，不是完整供应链签名体系。
- schema validator 是内置轻量实现，覆盖当前 schema 使用的规则，不等同完整 JSON Schema 规范实现。

未包含在当前开源仓库：

- 企业私有 starter / enterprise packs。
- 企业审批系统、权限平台、密钥系统的真实集成。
- 发布到 crates.io、Homebrew、npm 的流水线。
- 完整 Backstage 插件发布包。

## 开发验证

本地提交前建议运行：

```bash
make check
```

额外 smoke：

```bash
make smoke
make schema-check
make mcp-check
make installer-check
```

## 扩展文档

- Capability Pack authoring: [docs/capability-pack-authoring.md](docs/capability-pack-authoring.md)
- Plugin protocol: [docs/plugin-protocol.md](docs/plugin-protocol.md)
- MCP wrapper: [integrations/mcp](integrations/mcp)
- Backstage example: [integrations/backstage](integrations/backstage)
- Full design document: [Rainy_CLI_最终形态程序设计与详细开发文档.md](Rainy_CLI_最终形态程序设计与详细开发文档.md)
