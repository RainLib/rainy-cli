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
- `integrations/skills/rainy-cli`: Rainy CLI 执行、安全审批和跨平台 bootstrap Skill。
- `integrations/skills/rainy-comet`: OpenSpec + Superpowers + Comet 与 Rainy 的职责交接 Skill。
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
- 模型用户：安装 Rainy Skill 后，可以让模型发现 Rainy 工作流；本地缺少 `rainy` 时 Skill 会校验并安装官方 Release 后继续。
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
curl -fsSL https://github.com/RainLib/rainy-cli/releases/latest/download/install.sh | sh
```

Windows PowerShell：

```powershell
powershell -ExecutionPolicy Bypass -c "iwr https://github.com/RainLib/rainy-cli/releases/latest/download/install.ps1 -UseB | iex"
```

安装脚本会根据当前系统下载对应的 release asset：

- Linux x86_64: `rainy-x86_64-unknown-linux-gnu.tar.gz`
- Linux arm64: `rainy-aarch64-unknown-linux-gnu.tar.gz`
- macOS Intel: `rainy-x86_64-apple-darwin.tar.gz`
- macOS Apple Silicon: `rainy-aarch64-apple-darwin.tar.gz`
- Windows x64: `rainy-x86_64-pc-windows-msvc.zip`

默认安装目录是 `~/.rainy/bin`。可以覆盖：

```bash
INSTALL_DIR=/usr/local/bin sh scripts/install.sh
RAINY_REPO=owner/repo RAINY_VERSION=v0.1.2 sh scripts/install.sh
```

Windows 安装脚本也支持同样的参数：

```powershell
.\scripts\install.ps1 -Repo owner/repo -Version v0.1.2 -InstallDir "$HOME\.rainy\bin" -AddToPath
```

安装器必须下载并验证对应 `.sha256` 文件，校验文件缺失或摘要不匹配时会停止。替换失败会恢复原有二进制，成功后会自动验证 `rainy --version`。
预编译 CLI 已内嵌 community packs 和 JSON schemas，安装后不依赖源码仓库；首次使用时会把只读运行资源提取到系统临时缓存。

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
rainy verify --profile ci
rainy evidence generate
```

为项目启用组合式模型工作流（需要 Node.js 20+、npm/npx 和 Git）：

```bash
rainy skill init --profile comet --target codex --language zh --dry-run
rainy skill init --profile comet --target codex --language zh --apply
rainy skill status
rainy skill doctor
```

默认 `comet` profile 由 OpenSpec 管理需求与验收标准、Superpowers 管理工程方法、Comet 管理阶段和恢复状态，Rainy 继续负责可执行计划、policy、显式 `--apply`、verify、evidence 和 audit。核心 CLI 不强制依赖这些 Node 工具；只需要 Rainy Skill 时可使用：

```bash
rainy skill init --profile rainy --target codex --apply
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
make release-check # 发 GitHub Release 前的本地检查
make production-check # 生产可用性本地检查，等同 release-check
make schema-check  # 检查 schemas/*.schema.json 可解析
make conformance   # 检查 community-packs conformance
make mcp-check     # 编译检查 MCP Python wrapper
make skill-check   # 检查模型 Skill 和跨平台 CLI 引导
make installer-check # 检查安装脚本语法
make installer-test # 检查安装器平台识别和 checksum 失败路径
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

Skill profile 管理：

```bash
rainy skill init --profile comet --target codex --language zh --dry-run
rainy skill init --profile comet --target codex --language zh --apply
rainy skill install --dry-run
rainy skill install --apply
rainy skill status
rainy skill doctor
rainy skill update --dry-run
rainy skill update --comet-version 0.4.0-beta.6 --apply
rainy skill uninstall --dry-run
rainy skill uninstall --apply
```

目前项目 scope 支持 `codex`、`claude`、`cursor`、`github-copilot`、`gemini`、`opencode`。Comet 包使用精确版本，`skills.lock` 记录 Rainy 及上游 Skill 内容摘要；检测到已锁定 Rainy Skill 被手工修改时会拒绝覆盖，审阅后才能使用 `--force`。全局宿主安装暂不由 Rainy 管理。

版本检查和更新：

```bash
rainy self check
rainy self check --json
rainy self check --repo owner/repo
rainy self update
rainy self update --repo owner/repo --version v0.1.2
rainy self skip 0.2.0
rainy self skip --repo owner/repo 0.2.0
```

release 构建出来的非 debug CLI 会周期性检查 GitHub latest release，并在发现新版本时提示：

```text
Rainy CLI update available: 0.1.1 -> 0.2.0.
Run `rainy self update` to update, or `rainy self skip 0.2.0` to skip this version.
```

自动检查默认行为：

- debug 构建、CI、`--json`、`--quiet` 不会自动输出更新提示。
- 默认 24 小时检查一次。
- 网络检查使用短超时；失败后指数退避，避免阻塞后续命令。
- `RAINY_NO_UPDATE_CHECK=1` 或 `RAINY_SKIP_UPDATE_CHECK=1` 可以关闭自动检查。
- `RAINY_UPDATE_CHECK_INTERVAL_HOURS=0` 可以让每次运行都检查。
- `RAINY_UPDATE_REPO=owner/repo` 可以覆盖 GitHub release 仓库。

## 发布流程

面向 `main` 的 pull request 会运行 CI，依赖相关变更还会运行安全检查；合并后的普通 `main` push 不重复运行。安全检查另有每周定时扫描和手动触发。仓库应启用 `main` 分支保护并将 PR 检查设为 required checks。

GitHub Release 由 `.github/workflows/release.yml` 负责。发版前建议本地先跑：

```bash
make release-check
```

创建并推送版本标签后会触发 release workflow：

```bash
git tag -a v0.1.2 -m "Rainy CLI v0.1.2"
git push origin v0.1.2
```

release workflow 会先执行格式、测试、clippy、schema、MCP wrapper 和安装脚本检查，然后分别构建并上传：

- `rainy-x86_64-unknown-linux-gnu.tar.gz`
- `rainy-aarch64-unknown-linux-gnu.tar.gz`
- `rainy-x86_64-apple-darwin.tar.gz`
- `rainy-aarch64-apple-darwin.tar.gz`
- `rainy-x86_64-pc-windows-msvc.zip`
- 对应的 `.sha256` 文件
- `install.sh`
- `install.ps1`
- `rainy-cli-skill.tar.gz` / `rainy-cli-skill.zip`
- `rainy-comet-skill.tar.gz` / `rainy-comet-skill.zip`
- Skill 包对应的 `.sha256` 文件
- SPDX JSON SBOM 和 GitHub build provenance

用户安装或更新时，脚本会按当前操作系统和 CPU 架构拉取对应 release asset。fork 或私有发布仓库可以通过 `RAINY_REPO=owner/repo`、`RAINY_UPDATE_REPO=owner/repo` 或 `--repo owner/repo` 指定。

## 使用模型

正式模型 Skill 包括 [`integrations/skills/rainy-cli`](integrations/skills/rainy-cli) 和 [`integrations/skills/rainy-comet`](integrations/skills/rainy-comet)。前者负责 CLI bootstrap 和安全执行；后者只负责 OpenSpec、Superpowers、Comet 与 Rainy 的流程交接，不复制上游 Skill 内容。

将 `rainy-cli` 安装到支持 Agent Skills 的模型宿主后，模型会先执行强制 bootstrap：

- 优先使用 `RAINY_BIN`、当前 `PATH` 或 `$HOME/.rainy/bin` 中已有的 Rainy。
- 如果本地没有 `rainy`，从 `RainLib/rainy-cli` 最新 GitHub Release 下载安装器和 `installers.sha256`。
- 校验安装器摘要后才执行安装，并再次运行 `rainy --version`。
- 返回安装后二进制的绝对路径，因此当前模型会话不需要重启 shell 就能继续。
- 安装或校验失败时停止后续工程操作，不会绕过 Rainy 的 policy gate。

也可以由 Rainy 以项目 scope 安装两个 Skill 和上游组合：`rainy skill init --profile comet --target codex --apply`。该命令生成可提交的 `rainy-skills.yaml` 和 `skills.lock`，调用固定版本 Comet 的官方初始化入口，并强制 `.comet/config.yaml` 中 `auto_transition: false`。Comet 阶段前进不等于批准 Rainy `--apply`。

可以独立验证 bootstrap：

```bash
sh integrations/skills/rainy-cli/scripts/ensure-rainy.sh
```

Windows PowerShell：

```powershell
& integrations/skills/rainy-cli/scripts/ensure-rainy.ps1
```

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
- `verify --profile local` 适合开发机，缺少本地工具链时可给 warning；`verify --profile ci` 是严格门禁，未知步骤或缺失验证工具会失败。
- 策略会阻止敏感路径、危险命令、需要审批的操作和插件越权写入。
- Wasm 是默认插件运行时；原生 `rainy-*` 插件必须在 Rainy 项目内通过 `--allow-native-plugin`、`RAINY_ALLOW_NATIVE_PLUGIN=1` 或 `policy.allowNativePlugins: true` 显式授权，并写入审计日志。

## 当前建设进度

已完成：

- Rust CLI 命令树：`new/init/add/apply/capability/pack/doctor/verify/evidence/plugin/agent/skill/schema/conformance`。
- Golden Path 初始化：生成 Spring Boot + Next.js 基础项目、`rainy.yaml`、`capability.lock`、AGENTS.md、CI、compose、evidence 目录。
- Capability Pack 解析：本地、git cache、HTTP registry source。
- 内置 action：Maven、YAML/JSON/JSONC/TOML merge、模板渲染、文件创建/追加、Docker Compose、package.json、devcontainer、Helm draft 等。
- Plan / Diff / Apply：支持 dry-run、plan file、事务式 apply 回滚、幂等 no-op。
- Capability 依赖和 provider 解析：依赖缺失失败、被依赖能力禁止删除、provider 默认/显式/非法场景有稳定错误。
- Policy Gate：内置敏感路径、项目 policy、org policy、capability policy、审批动作、危险命令、插件写权限。
- Doctor / Verify / Evidence：健康检查、能力验证、证据报告、secret 脱敏、默认开发 secret warning。
- Audit log：修改命令执行前检查审计可写性，成功和失败命令通过文件锁写入 `.rainy/audit.log`。
- Plugin：Wasm action plugin 默认可用；原生 `rainy-*` 需要显式信任；HTTP adapter 受权限、HTTPS/loopback 和响应大小限制。
- Release 安装和自更新：五平台构建与 smoke、强制 checksum、回滚安装、SBOM、provenance、原生 HTTPS 版本检查和 `self check/update/skip`。
- 模型 Skill：Rainy CLI bootstrap Skill、Rainy-Comet bridge Skill、项目级 profile/lock、六类宿主目标、Comet 固定版本安装、内容摘要漂移检查、doctor/update/uninstall。
- MCP 示例：stdio JSON-RPC wrapper 调用 Rainy CLI，默认 dry-run 计划能力接入。
- Backstage 示例：scaffolder actions 和模板示例。
- Schema / conformance：标准 Draft 2020-12 validator、schema list/validate、pack/plugin conformance 检查。
- 测试：包含 unit 和 E2E tests，覆盖 Golden Path、policy、plugin、schema、conformance、事务回滚、自更新状态等主流程。
- CI / release 门禁：三系统测试、MSRV、audit/deny、CodeQL、格式、E2E、clippy、schema、安装器测试、JSON smoke、conformance 和五平台 release 构建。

部分完成 / 示例级：

- Backstage 集成目前是示例代码，未打包成可直接发布的 Backstage npm 包。
- MCP wrapper 是最小可运行示例，生产环境还需要接入具体 MCP host 配置、权限边界和部署方式。
- `verify --profile local` 会在本地工具链缺失时对部分外部命令给 warning；生产门禁应使用严格的 `verify --profile ci`。
- Pack 默认生成完整性 manifest；配置 `RAINY_PACK_SIGNING_KEY` / `RAINY_PACK_TRUSTED_PUBLIC_KEY` 后使用 cosign 验证发布者身份。

未包含在当前开源仓库：

- 企业私有 starter / enterprise packs。
- 企业审批系统、权限平台、密钥系统的真实集成。
- 发布到 crates.io、Homebrew、npm 的流水线。
- 完整 Backstage 插件发布包。

## 开发验证

本地提交前建议运行：

```bash
make production-check
```

额外 smoke：

```bash
make smoke
make schema-check
make mcp-check
make installer-check
make installer-test
```

## 扩展文档

- Current architecture and flow: [docs/architecture-and-flow.md](docs/architecture-and-flow.md)
- Capability Pack authoring: [docs/capability-pack-authoring.md](docs/capability-pack-authoring.md)
- Plugin protocol: [docs/plugin-protocol.md](docs/plugin-protocol.md)
- MCP wrapper: [integrations/mcp](integrations/mcp)
- Model Skill: [integrations/skills/rainy-cli](integrations/skills/rainy-cli)
- Composed workflow Skill: [integrations/skills/rainy-comet](integrations/skills/rainy-comet)
- Skill profile management: [docs/skills-management.md](docs/skills-management.md)
- Backstage example: [integrations/backstage](integrations/backstage)
- Full design document: [Rainy_CLI_最终形态程序设计与详细开发文档.md](Rainy_CLI_最终形态程序设计与详细开发文档.md)
