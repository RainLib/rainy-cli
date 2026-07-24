# Rainy CLI 当前架构与流程

本文描述当前仓库已经实现的架构、主流程和已知不足。它面向维护者、Agent 和后续实现者；愿景级设计仍以根目录的 `Rainy_CLI_最终形态程序设计与详细开发文档.md` 为参考。

## 项目定位

Rainy CLI 是一个用 Rust 实现的软件能力编排 CLI。它把能力接入过程拆成稳定的命令和协议，让开发者、Agent、Backstage、CI 可以用同一套流程完成：

```text
Plan -> Diff -> Policy -> Apply -> Doctor -> Verify -> Evidence
```

当前开源仓库提供的是 Rainy 的核心实现和一组 community packs。企业内部 starter、审批系统、密钥系统、权限平台等能力不在当前仓库内，需要通过私有 capability pack、plugin 或集成层接入。

## 当前仓库分层

- `crates/rainy-cli`: 当前唯一 Rust crate，包含 CLI 命令树、配置、registry、plan、action、policy、patch、doctor、verify、evidence、plugin、schema、conformance、audit、self-update 等模块。
- `community-packs`: 官方默认分发包中的开源能力模块，覆盖 Spring Boot、Next.js、Docker Compose、PostgreSQL、Redis、MinIO、OIDC/Keycloak、OpenAPI、Dev Container、GitHub Actions、OpenTelemetry、Helm draft。
- `schemas`: Rainy 项目、能力包、计划、变更集、报告、插件、审计、自更新等 JSON Schema。
- `integrations/skills/rainy-cli`: 模型可发现的 Rainy 执行 Skill，定义 bootstrap、plan/apply/verify 安全工作流。
- `integrations/skills/rainy-comet`: OpenSpec、Superpowers、Comet 和 Rainy 之间的职责与审批边界 Skill。
- `integrations/mcp`: MCP stdio wrapper 示例，供 Agent 以 JSON-RPC 方式调用 Rainy CLI。
- `integrations/backstage`: Backstage scaffolder action 和 template 示例。
- `scripts`: GitHub Release 安装脚本，按操作系统和 CPU 架构下载对应 CLI 包。
- `.github/workflows`: CI 和 release workflow。CI 负责格式、测试、clippy、schema、MCP wrapper、Skill、安装脚本和 smoke；release 负责发版前验证、多平台构建，并发布 CLI 与平台无关的 Skill assets。

## 核心模块职责

`main.rs` 是命令分发和全局横切逻辑入口。它解析 CLI 参数，执行自更新提示，按命令路由到各模块，并在成功或失败后写 audit log。

`cli.rs` 定义命令树和参数结构。当前命令包括 `new/init/add/apply/capability/pack/registry/defaults/doctor/verify/evidence/plugin/agent/skill/conformance/schema/self`，未知顶层命令会进入 plugin external forwarding。

`config.rs` 负责 `rainy.yaml` 和 `capability.lock` 的读写。`rainy.yaml` 描述项目、路径、registry source、policy、verify 配置；`capability.lock` 记录已安装能力、provider、版本、artifacts 和 skills。

`defaults.rs` 管理官方内容分发。`rainy-defaults.yaml` 声明 CLI 兼容范围以及 Packs、Skills、模板目录；Git source 按 ref 解析到 commit，下载到 `RAINY_HOME/defaults`，通过全局 lock、跨进程文件锁和原子目录替换保证一致性。Debug 构建可直接读取工作区；release 二进制始终按默认包流程运行，除非显式配置开发源。

`skills.rs` 负责项目级模型 Skill 生命周期。它读取 `rainy-skills.yaml`，从默认分发包安装 Rainy Skills，调用固定版本 Comet 的官方 CLI 安装 OpenSpec/Comet，并通过固定版本 `skills` CLI 安装固定版本 Superpowers，生成 `skills.lock`，执行内容摘要和依赖 doctor，并提供 install/status/update/uninstall。核心能力 profile 不启用时不会要求 Node.js。

`registry.rs` 负责加载 capability pack 和 capability definition。当前支持默认分发包、命名本地源、GitHub/GitLab Git 源、HTTPS index 和带 SHA-256 的 HTTPS 压缩包，并提供模块选择、企业 Skill 安装、pack install/update/sign/verify、capability list/explain/graph。远程企业内容使用 `RAINY_HOME/registries/<name>/<source-hash>` 缓存，项目仅保存配置和 `.rainy/registry.lock`。官方默认内容使用独立的 `RAINY_HOME/defaults` 缓存；只有 JSON Schemas 编译进可执行文件。

`actions.rs` 负责把 capability action 转换成 `ExecutionPlan` 和 `ChangeSet`。它处理依赖检查、provider 选择、模板变量、内置 action 执行、plan file 重放、upgrade/remove，并生成 capability lock 更新。

`patch.rs` 负责文件级 diff 和 apply。它生成 create/modify/delete change，支持 no-op 判断，apply 时逐个写入，失败后按已应用变更回滚。

`policy.rs` 是 apply 前的安全门。它合并系统、用户、工作区和 capability policy，检查敏感路径、allow/deny edit、需要审批的 action，以及危险命令。

`doctor.rs`、`verify.rs`、`evidence.rs` 分别负责项目健康检查、能力验证和证据产出。Evidence 会汇总当前配置、lock、doctor、verify 和变更信息，并输出 Markdown/JSON。

`plugin.rs` 负责外部扩展。当前支持安装插件、列出/查看插件、调用 plugin action、转发 external command、HTTP adapter 和 Wasm action plugin。插件返回的变更仍要经过权限校验和主 CLI policy gate。

`schema.rs` 和 `conformance.rs` 负责 schema list/validate 以及 pack/plugin conformance 检查。Schema 使用标准 Draft 2020-12 validator，并将仓库内相对引用装配为本地 definitions。

`update.rs` 负责版本检查、自更新和跳过版本。它通过 GitHub latest release 获取版本信息，并调用 release 中的安装脚本完成更新。

## 主流程

### 1. 初始化项目

`rainy new` 和 `rainy init app` 由 `init.rs` 生成 Golden Path 项目骨架，包括：

- `rainy.yaml`
- `capability.lock`
- `AGENTS.md`
- backend/frontend 基础目录
- compose、CI、openapi、charts、evidence 等目录或文件

`new/init app` 默认会写入文件，也支持 `--dry-run` 预览。

### 2. 发现能力

能力发现由 `registry.rs` 完成：

```text
rainy capability list
rainy capability explain <id>
rainy capability graph
```

Registry 会先读取项目配置，再加载配置中的 source 和官方默认包，最终按 capability id 提供查询。

### 3. 计划能力变更

能力接入通常从 dry-run 开始：

```text
rainy add capability <id> --dry-run
```

流程是：

```text
load config/lock
load registry
resolve capability
check dependencies
resolve provider
render inputs/templates
run built-in action planners
append capability.lock change
return ExecutionPlan + ChangeSet + rendered diff
```

如果指定 `--output-plan`，CLI 会把 plan 写成 JSON，后续可通过 `rainy apply --plan` 重放。

### 4. 应用能力变更

写文件必须显式 `--apply`：

```text
rainy add capability <id> --apply
rainy apply --plan plans/<id>.json --apply
```

apply 流程是：

```text
plan change
check plan policy
check change policy
apply changes transactionally
write audit log
print human or JSON output
```

如果任意文件写入失败，`patch.rs` 会回滚本次已经写入的文件，避免项目处于半应用状态。

### 5. 验证与证据

`rainy doctor` 检查项目基础文件、lock、默认开发 secret、能力 artifacts 和 capability doctor checks。

`rainy verify --profile local` 运行项目配置和 capability validations，适合开发机使用；对 Maven、Node、Docker 等外部工具缺失可给出 warning。`rainy verify --profile ci` 是严格门禁，未知步骤、registry 加载错误、缺失验证工具链都会失败。Golden Path 生成的 GitHub Actions 会准备 Java/Maven/Node/pnpm、安装前端依赖、安装 Rainy CLI，再运行严格 `ci` 验证。

`rainy evidence generate` 会生成 evidence 文件，把配置、已安装能力、doctor、verify 和变更摘要整理成可进入 PR 或审计链路的材料。

### 6. Pack 和 Plugin 扩展

Pack 是能力定义的主要扩展方式。维护者可以通过 `rainy pack install/update/sign/verify` 管理本地、git 或 HTTP source，并用 `rainy conformance check` 检查协议一致性。HTTP registry 必须为每个文件声明 SHA-256；下载先进入临时缓存，完整校验 Pack 身份和内容后再原子替换，失败时保留旧缓存。`capability.lock` 继续固定实际 Pack source、版本和整体摘要。

Plugin 是命令和 action 扩展方式。外部命令、HTTP adapter、Wasm action plugin 都不能绕过主 CLI；插件 manifest 中的权限会先被校验，返回的变更还要走 policy 和 patch。Wasm 是默认运行时；原生插件必须显式授权且只能在 Rainy 项目内运行，以保证执行记录写入项目审计日志。

企业定义按四层接入：声明式项目变更放私有 Capability Pack，版本和 digest 放私有 Registry，
不可覆盖的安全要求放系统/用户/工作区分层 policy，审批、IAM、部署和 secret 平台通过 Wasm
plugin 或 HTTPS adapter 对接。仓库中的 `examples/enterprise` 可完整运行本地 plan/apply/doctor/verify
流程，`schemas/org-policy.schema.json` 用于校验工作区企业 policy。真实实现、凭据和服务地址仍由企业
私有仓库维护，详见 `docs/enterprise-integration.md`。

### 7. 组合式 Skill 管理

Rainy 将四个系统按职责串联，而不是合并它们的文档：

```text
User / Agent
  -> Comet phase and resume state
  -> OpenSpec intent + Superpowers engineering method
  -> Rainy execution plan + policy + explicit apply
  -> doctor + strict verify + evidence + audit
```

默认组合 profile 通过以下命令启用：

```text
rainy skill init --profile comet --target codex --language zh --dry-run
rainy skill init --profile comet --target codex --language zh --apply
rainy skill status
rainy skill doctor
```

所有安装、更新和卸载默认只生成 dry-run report，必须显式 `--apply`。Comet、`skills` CLI 和 Superpowers 均固定到精确 SemVer；OpenSpec/Comet/Superpowers 实际生成的 Skill 路径、管理方和内容摘要写入 `skills.lock`。任一必需组件缺失都会阻止成功安装并使 doctor 失败。Rainy 自有 Skill 和 Rainy 管理的上游 Skill 发生手工修改时，update/uninstall 会拒绝继续，除非用户审阅后指定 `--force`。

交互终端中的 `skill init` 先选择完整组合或 Rainy-only 套件，始终加入 Universal `.agents/skills`，再多选 Codex、Claude、Cursor、GitHub Copilot、Gemini 和 OpenCode 目标，最后展示安装摘要并要求独立的 yes/no 确认；已有 profile 的 `skill install` 使用同一确认流程。非 TTY、JSON 和 CI 使用确定性的 `comet + codex + universal` 默认值且不会提示，必须显式 `--apply`。Comet 初始化前先安装目标宿主下的 `rainy-cli`、`rainy-comet` 和固定版本 Superpowers，使 Comet 检测到 Superpowers 已存在并避免重复安装。Universal 和 Codex 共享 `.agents/skills`，Claude 使用 `.claude/skills`，Cursor 使用 `.cursor/skills` 并兼容从 `.agents/skills` 发现遵循 Universal 标准的上游 Skills；安装后会按摘要把受管的 `.codex/skills`、`.agent/skills` 兼容副本合并到规范通用目录，保留规则、hooks、workflows 和未知用户文件。上游初始化完成后，Rainy 使用结构化 YAML 合并 `.comet/config.yaml` 并强制 `auto_transition: false`；Comet 阶段变化不能替代 Rainy apply、原生插件、部署、迁移或 secret 写入审批。

`AGENTS.md` 使用 `rainy:context` 管理块更新，保留 Comet block 和用户自定义内容。`rainy skill sync` 对未启用 profile 的旧项目继续保持原有上下文同步行为。

### 8. 发布、安装和自更新

CI 和依赖安全检查在面向 `main` 的 pull request 上运行，普通 `main` push 不会重复触发。安全 workflow 还保留每周定时扫描和手动触发；仓库应通过分支保护禁止绕过 PR required checks 直接写入 `main`。

Release workflow 在 tag 或手动触发时运行：

```text
verify release inputs
build Linux/macOS/Windows binaries
package archives and sha256 files
force standalone defaults-download and embedded-schema smoke, verify all expected assets, and generate SBOM/provenance
publish GitHub Release assets, checksums, and installer scripts
```

安装脚本根据当前系统选择对应 asset：

- Linux x86_64
- Linux arm64
- macOS Intel
- macOS Apple Silicon
- Windows x64

安装器默认把 `~/.rainy/bin` 持久化到检测到的 Unix shell profile，或写入
Windows 用户 PATH；两种方式都保持幂等并允许显式禁用。静态镜像使用
`latest.txt` 加版本目录协议，设置 `RAINY_RELEASE_BASE_URL` 后版本解析和
资产下载都从 OSS/CDN 完成，不访问 GitHub API。

CLI 自更新通过 `rainy self check/update/skip` 管理。默认仓库来自 Cargo package repository，也可以通过 `--repo` 或 `RAINY_UPDATE_REPO` 覆盖。版本检查使用原生 HTTPS、标准 SemVer、超时和失败退避；指定版本更新固定使用对应 tag 的安装器，并在执行前通过该 Release 的 `installers.sha256` 验证安装脚本。

## 模型接入

模型接入分为三层：Skill 提供触发条件、操作顺序和安全规则；MCP 将允许的操作暴露为结构化工具；CLI 继续作为 plan、policy、apply、rollback、verify 和 audit 的唯一执行边界。

`integrations/skills/rainy-cli/SKILL.md` 是正式执行 Skill。每次工作流先运行平台对应的 `ensure-rainy`：已有 CLI 时验证并复用；缺失时下载最新 Release 的安装器与 `installers.sha256`，校验安装器后执行，最后返回可立即使用的绝对路径。校验或安装失败会终止工作流。

`integrations/skills/rainy-comet/SKILL.md` 是组合流程 bridge，规定 OpenSpec 管 WHAT、Superpowers 管 HOW、Comet 管 WHEN/NEXT、Rainy 管 EXECUTE/GUARD。项目可通过 `rainy skill init/install/update/uninstall` 管理宿主目录，也可以从 Release 独立安装两个 Skill 压缩包。

## 数据与协议

- `rainy.yaml`: 项目配置入口，包含路径、registry source、policy、verify。
- `capability.lock`: 已安装能力的事实来源，记录 provider、版本、Pack source/digest、artifacts、skills。
- `rainy-skills.yaml`: 项目模型 Skill 期望状态，记录 profile、scope、语言、宿主目标、固定 Comet 包和审批策略。
- `skills.lock`: Skill 安装事实，记录 Rainy/Comet 版本、宿主路径、实际上游组件和内容摘要。
- `ExecutionPlan`: 可保存和重放的能力变更计划。
- `ChangeSet`: 文件级变更集合，包含 before/after、kind、summary、noop。
- `schemas/*.schema.json`: 对外协议和报告结构的稳定参考。
- `.rainy/org-policy.yaml`: 仓库级企业策略，使用 `org-policy` schema 校验；系统和用户策略使用相同字段。
- `.rainy/audit.log`: 命令成功或失败的本地审计记录。
- JSON 输出：所有主要命令支持 `--json`，供 Agent、MCP、CI 调用。
- 人类输出：结果摘要、下一步、影响范围、可选明细采用固定层级；内部命令和全部路径
  仅由 `--verbose` 展示。错误只输出一次，并包含稳定 code、原因和可执行恢复命令。
- 进度输出：统一四阶段生命周期写入 `stderr`，TTY 使用带心跳和耗时的动态进度条，
  `--progress always` 使用稳定逐行日志；失败进度只报告耗时，具体错误由结构化错误块输出一次。
  `--json` 和 `--quiet` 强制关闭进度。
- Release assets: 多平台 CLI 包、对应 sha256 文件和安装脚本。
- Model Skill assets: 平台无关的 `rainy-cli-skill`、`rainy-comet-skill` 压缩包及对应 sha256。

## 当前限制与后续建设

### P0 / 近期

- 组合 Skill 当前只管理 project scope；全局宿主目录不由 Rainy 自动写入，避免影响其他仓库。
- 上游 Comet 当前没有公开的非交互平台参数。Rainy 以所选目标创建和验证受管 Skill，
  并将 Codex 共享目录产生的兼容副本归一到 `.agents/skills`；未来应推动 Comet 提供
  显式 `--platform` 参数，减少对上游平台检测行为的适配。
- 当前 MCP wrapper 仍是示例级 Python 进程，尚未具备生产 MCP host 的 workspace allowlist、审批交互和独立安装包。

### P1 / 中期

- 当前 Rust 实现仍是单 crate 多模块，和最终设计稿的多 crate 分层有差距。建议在接口稳定后拆分 `core/config/registry/plan/actions/policy/plugin/json-protocol`。
- `verify` 已区分 `local` 和 `ci`：local 适合本地开发，ci 是严格质量门禁。后续可以继续扩展 profile schema，例如显式声明 strict、timeout、required tools。
- MCP 和 Backstage 已补充部署、权限、版本兼容和打包说明，但实现仍是示例级；模型 Skill 已可独立安装，MCP 尚未发布为独立生产包。
- Pack 完整性 manifest 可配合 cosign 发布者签名；组织仍需维护受信公钥轮换和撤销流程。

### P2 / 长期

- GitHub Release 仍是主发布源，OSS/CDN 镜像当前由发布辅助脚本同步。后续可把镜像上传接入受保护的发布环境，并补 crates.io、Homebrew、npm 或包管理器 tap。
- audit log 具备预检、文件锁和落盘同步，但仍是本地文件；企业场景需要集中化审计、指标、trace id 贯通和 SIEM/日志平台集成。
- Capability registry 已具备协议限制、超时、大小上限、逐文件摘要和原子缓存替换；后续仍可增加离线镜像治理、公钥轮换和撤销分发。
- 企业审批系统、密钥系统、权限平台、starter 生态都需要通过私有 pack/plugin 落地，本仓库只提供协议和示例。
