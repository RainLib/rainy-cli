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
- `examples/enterprise`: 可本地验证的企业 pack、project policy 和接入样例。
- `examples/enterprise-source`: 带根清单、项目模板和可组合模块的最小企业 Source。
- `schemas`: Rainy 项目、能力包、计划、变更、报告、插件等 JSON Schema。
- `integrations/skills/rainy-cli`: Rainy CLI 执行、安全审批和跨平台 bootstrap Skill。
- `integrations/skills/rainy-comet`: OpenSpec + Superpowers + Comet 与 Rainy 的职责交接 Skill。
- `integrations/mcp`: MCP stdio wrapper 示例，供 Agent 调用 Rainy CLI。
- `integrations/backstage`: Backstage scaffolder action/template 示例。
- `docs`: 外部作者编写 capability pack 和 plugin 的说明。
- `.github/workflows/ci.yml`: 基础 CI 门禁示例。

企业平台团队从零建设 GitHub/GitLab 能力仓库，请从
[企业 Git 能力仓库制作规范](docs/enterprise-git-authoring.md) 开始；其中给出了仓库目录、Pack、
Capability、模板、Skill、Plugin、Policy、CI、版本发布、项目消费、更新和回滚的完整约定。
需要把模板、模块、Pack、Skill 和 Plugin 作为一个可版本化交付包统一分发时，使用
[Rainy Source 企业内容分发与版本管理](docs/source-management.md)。

官方默认包中的 community packs 覆盖以下主流研发闭环：

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
irm https://github.com/RainLib/rainy-cli/releases/latest/download/install.ps1 | iex
```

安装脚本会根据当前系统下载对应的 release asset：

- Linux x86_64: `rainy-x86_64-unknown-linux-gnu.tar.gz`
- Linux arm64: `rainy-aarch64-unknown-linux-gnu.tar.gz`
- macOS Intel: `rainy-x86_64-apple-darwin.tar.gz`
- macOS Apple Silicon: `rainy-aarch64-apple-darwin.tar.gz`
- Windows x64: `rainy-x86_64-pc-windows-msvc.zip`

默认安装目录是 `~/.rainy/bin`。Unix 安装器会根据 `$SHELL` 幂等写入
`.zshrc`、`.bashrc`、`.profile` 或 Fish 配置；新终端可以直接执行
`rainy`。由于 `curl | sh` 是子进程，当前 Unix 终端需要按安装器输出执行
一次 `source`，或者重新打开终端。Windows 安装器默认同时更新用户 PATH 和
当前 PowerShell 进程 PATH。

可以覆盖安装目录或禁止修改 PATH：

```bash
INSTALL_DIR=/usr/local/bin sh scripts/install.sh
RAINY_REPO=owner/repo RAINY_VERSION=v0.5.2 sh scripts/install.sh
RAINY_NO_MODIFY_PATH=1 sh scripts/install.sh
```

Windows 安装脚本也支持同样的参数：

```powershell
.\scripts\install.ps1 -Repo owner/repo -Version v0.5.2 -InstallDir "$HOME\.rainy\bin"
.\scripts\install.ps1 -NoModifyPath
```

安装器必须下载并验证对应 `.sha256` 文件，校验文件缺失或摘要不匹配时会停止。替换失败会恢复原有二进制，成功后会自动验证 `rainy --version`。
预编译 CLI 仅内嵌与当前协议强绑定的 JSON Schemas。官方 community packs、Rainy Skills 和
Golden Path 模板由默认分发包管理，首次需要时从同版本 Git tag 下载到
`~/.rainy/defaults/rainy-official/<SOURCE_HASH>`，不会写入当前工程。下载成功后可离线复用。

```bash
rainy defaults status
rainy defaults install --apply
rainy defaults doctor
rainy defaults update --apply
```

企业镜像可设置 `RAINY_DEFAULTS_SOURCE` 和 `RAINY_DEFAULTS_REF`；`RAINY_OFFLINE=1` 禁止网络
回源，缓存不存在时会返回可操作的错误。

GitHub 访问不稳定时可以使用 OSS/CDN 静态镜像。镜像配置、目录协议和
`ossutil` 发布步骤见 [Release mirrors](docs/release-mirrors.md)。

从空目录创建 Golden Path 项目：

```bash
rainy new demo-saas --golden-path spring-nextjs-saas --package com.example.demo --apply
cd demo-saas
```

推荐的企业 Source 流程会先校验根目录 `rainy-source.yaml`，再把不可变内容存入用户级缓存：

```bash
rainy source inspect \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0
rainy source add company \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0 --apply
rainy new order-service --source company --template service-base \
  --module backend-java,delivery-gitlab \
  --package com.company.orders --apply
cd order-service
rainy source check --project
```

Source 也支持带 SHA-256 的 ZIP/TAR 和 `RainySourceIndex` 发布通道。配置和缓存位于
`RAINY_HOME`，不会把整个分发仓库下载到当前目录；生成项目只包含选中的模板和模块，并记录
`.rainy/project-source.lock`。完整版本、更新、回滚和内容接续命令见
[Source 管理文档](docs/source-management.md)。

旧版 `ProjectTemplateCatalog` 企业 Git 模板仍兼容：

```bash
rainy schema validate --schema project-template-catalog \
  --file ./project-templates.yaml
rainy new order-service --template enterprise-java-service \
  --template-config ./project-templates.yaml \
  --package com.company.orders --dry-run
rainy new order-service --template enterprise-java-service \
  --template-config ./project-templates.yaml \
  --package com.company.orders \
  --git-url git@git.example.com:apps/order-service.git --apply
```

Rainy 会将固定 Git ref 克隆到临时目录，渲染模板并校验 `rainy.yaml` 与 `capability.lock`，但不会把
模板仓库的 `.git` 带入新工程。创建完成后会输出 `git init`、`git remote add origin`、首次提交与推送
命令。以 `.hbs` 结尾的文件内容和路径支持变量渲染；其他文件原样复制。模板源也可以统一声明在
`~/.rainy/templates.yaml`，或由 `RAINY_TEMPLATE_CONFIG` 指定。

查看可用能力：

```bash
rainy capability list
rainy capability explain minio-file-storage
rainy capability graph
```

先 dry-run 计划变更，再 apply：

```bash
rainy capability add minio-file-storage --provider minio --dry-run
rainy capability add minio-file-storage --provider minio --apply
```

检查和验证项目：

```bash
rainy doctor
rainy verify --profile local
rainy verify --profile ci
rainy evidence generate --apply
```

为项目启用组合式模型工作流（需要 Node.js 20+、npm/npx 和 Git）：

```bash
rainy skill --help
rainy skill install --help
rainy skill install # 缺少配置时自动初始化，并交互选择套件、平台和项目 Skill
rainy skill status
rainy skill doctor
```

`rainy skill install` 是日常统一入口。没有 `rainy-skills.yaml` 时会自动执行初始化；
在真实终端中会依次选择 Skill 套件、目标平台和 `rainy-skills/` 中的项目自定义 Skill。
已有配置时保留套件和平台，只重新选择项目 Skill。无交互的脚本、Agent 和 CI 默认使用
`--profile comet --target codex --language zh`，并始终加入 Universal `.agents/skills`；
使用 `--skill <SKILL_ID>`、`--all-custom-skills` 或 `--no-custom-skills` 可以明确自定义选择。
交互终端在选择后
显示安装摘要并询问 `[Y/n]`；明确确认后立即安装，选择 `n` 则只预览。
非交互调用仍必须使用 `--apply` 才会执行，`--yes` 是含义相同的兼容别名。
预览默认只显示状态、启用的 Skills、下一步和影响位置；`--verbose` 才显示内部上游命令
及全部路径。

Skill 管理可以直接用于普通 Git 项目，不要求预先存在 `rainy.yaml` 或 `capability.lock`。
这种模式只创建 `rainy-skills.yaml`、`skills.lock`、`AGENTS.md` 和选定宿主的 Skill 目录；
已有完整 Rainy 工程时才同步 `.enterprise-agent/` 能力上下文。

默认 `comet` profile 由 OpenSpec 管理需求与验收标准、Superpowers 管理工程方法、Comet 管理阶段和恢复状态，Rainy 继续负责可执行计划、policy、显式 `--apply`、verify、evidence 和 audit。Rainy 会统一安装并锁定三套上游 Skills：Comet、OpenSpec 和 Superpowers；任一项缺失都会使安装或 doctor 失败，不需要再手工执行 `npx skills add`。

用户可以把自己的规则、参考资料和可选命令放在项目的 Skill 库中，无需修改或重新打包
Rainy CLI：

```bash
rainy skill create release-review --description "Review enterprise releases" --apply
# 编辑 rainy-skills/release-review/SKILL.md、references/ 和 scripts/
rainy skill install --skill release-review --apply
```

Rainy 安装时只复制选中的 Skill，不执行其中的脚本，并把选择和内容摘要写入
`rainy-skills.yaml` 与 `skills.lock`。需要跨仓库共享的企业 Skill 应通过 Registry Pack
发布和安装，项目 `rainy-skills/` 只负责当前仓库拥有的规则。

核心 CLI 仍不强制依赖 Node 工具；只需要 Rainy Skill 时可使用：

```bash
rainy skill install --profile rainy --target codex --apply
```

Universal 和 Codex 的规范项目目录是 `.agents/skills`；Claude 使用
`.claude/skills`，Cursor 使用 `.cursor/skills`。Universal 始终启用，因此所有受支持
平台至少可以从通用目录发现 Rainy Skills。Rainy 安装或更新时会把可确认的旧
`.codex/skills` 和 OpenSpec 兼容 `.agent/skills` 托管副本合并到该目录；摘要不同
时停止并要求人工审阅，不会删除 `.codex/rules`、hooks、`.agent/workflows` 或其他
用户文件。

Agent 或 CI 使用 JSON 输出：

```bash
rainy capability list --json
rainy capability add minio-file-storage --provider minio --dry-run --json
rainy doctor --json
```

所有成功结果使用 `rainy.command.v1` 包装：

```json
{"protocolVersion":"rainy.command.v1","type":"capabilities","status":"ok","data":{"capabilities":[]}}
```

操作错误写入 `stderr`；Doctor、Verify、Schema 和 Conformance 的检查失败报告仍完整写入
`stdout`，并退出 `4`。固定退出码为：`0` 完成/预览/警告，`1` 运行或 I/O，`2` 参数或配置，
`3` 策略/审批，`4` 检查失败，`5` 网络/认证，`6` 摘要/签名完整性，`130` 取消。

## 命令进度

Rainy 在交互式终端中根据真实任务事件显示 spinner、当前阶段和耗时，不再伪造固定四阶段完成度。
Skill、verify、doctor 等多步骤命令会持续更新当前任务；列表、说明和补全等快速读取命令默认保持安静。
进度写入 `stderr`，最终结果写入 `stdout`，因此可以安全地重定向业务结果。

```bash
rainy skill install                      # 终端交互选择并显示动态进度
rainy verify --profile ci --progress always # CI/日志中强制逐行显示进度
rainy doctor --progress never            # 关闭进度
```

进度模式也可以通过 `RAINY_PROGRESS=auto|always|never` 配置。`--json` 和 `--quiet`
始终关闭进度，保证 JSON 协议及静默调用不会混入额外内容；`--no-color` 只关闭颜色，真实 TTY
仍原地刷新。Rainy 同时遵循 `NO_COLOR`；`TERM=dumb` 或重定向输出使用稳定逐行事件。按
`Ctrl+C` 会先恢复终端并请求取消，清理整个子进程组，最终退出 `130`。

## Shell 补全

`rainy completion <SHELL>` 从当前版本的真实命令树生成补全脚本，普通输出只包含脚本正文，
不会混入进度或审计记录：

```bash
# Zsh：当前会话
source <(rainy completion zsh)

# Fish：持久安装
rainy completion fish > ~/.config/fish/completions/rainy.fish

# Bash：持久安装
rainy completion bash > ~/.local/share/bash-completion/completions/rainy
```

## Makefile 管理命令

仓库提供了 `Makefile` 作为常用管理入口：

```bash
make help          # 查看所有目标
make build         # 构建 debug binary
make release       # 构建 release binary
make install       # cargo install --path crates/rainy-cli
make install-local # 替换 ~/.rainy/bin/rainy，并快照当前工作区 Defaults
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
make production-check # release-check + 强制 cargo audit/deny
make security-check # 强制执行 cargo audit 和 cargo deny
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

本地验证当前工作区版本并替换已安装命令：

```bash
make install-local
rainy --version

# 覆盖安装目录（例如 CI 沙箱或测试目录）
make install-local INSTALL_DIR="$HOME/.local/bin"
```

`install-local` 使用 `target/release/rainy`，先写入同目录临时文件，再原子替换
`$INSTALL_DIR/rainy`。随后验证当前工作区的 Packs、Skills 和模板，并原子快照到
`$RAINY_HOME/defaults`，所以本地测试不依赖尚未发布的版本 Tag。它不会修改 shell 的 `PATH`；
首次安装仍使用发布安装器来写入 shell 配置。

常用变量可以覆盖：

```bash
make demo PROJECT=my-app PACKAGE=com.example.app
make demo-add-apply PROJECT=my-app CAPABILITY=redis PROVIDER=local
make demo-verify PROJECT=my-app PROFILE=ci
```

## 常用命令

项目初始化：

```bash
rainy init app demo-saas --preset spring-nextjs --package com.example.demo --apply # 兼容入口
rainy new demo-saas --golden-path spring-nextjs-saas --apply
rainy new demo-saas --golden-path spring-nextjs-saas --dry-run --json
rainy new order-service --template enterprise-java-service \
  --template-config ./project-templates.yaml --dry-run
```

能力管理：

```bash
rainy capability list
rainy capability add minio-file-storage --provider minio --dry-run
rainy capability explain minio-file-storage
rainy capability installed
rainy capability graph
rainy capability upgrade minio-file-storage --dry-run
rainy capability remove minio-file-storage --dry-run
```

计划文件工作流：

```bash
rainy capability add minio-file-storage --provider minio --output-plan plans/minio.json
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
rainy pack sign ./community-packs/minio-file-storage --apply
rainy pack verify ./community-packs/minio-file-storage
```

企业 Registry（远程内容缓存到 `~/.rainy/registries`，不会下载到当前工程）：

```bash
rainy registry add company git+https://gitlab.example.com/platform/rainy-packs.git --ref v1.0.0 --apply
rainy registry sync company --module service-baseline,observability --apply
# 交互选择平台和 Pack 实际导出的 Skill
rainy registry sync company --module company-engineering --install-skills --apply
# CI / Agent 中显式选择，完全无交互
rainy registry sync company --module company-engineering --install-skills --target codex,cursor --skill company-service --apply
rainy registry list --verbose
rainy registry doctor company
```

可同时关联多个 Registry，也可使用带 SHA-256 的 `.tar.gz`、`.tgz`、`.zip` URL 或 HTTPS index。
项目提交 `rainy.yaml` 和 `.rainy/registry.lock`；通过 `RAINY_HOME` 可修改系统缓存根目录。企业仓库的
完整制作流程见 [企业 Git 能力仓库制作规范](docs/enterprise-git-authoring.md)，接入边界与架构说明见
[企业能力接入](docs/enterprise-integration.md)。

交互终端省略 `--target` 和 `--skill` 时会先多选 Agent 平台，再多选 `pack.yaml` 中
`exports.skills` 声明的 Skill。自动化使用可重复的 `--skill <SKILL_ID>`；确实需要全部安装时使用
`--all-skills`。`.rainy/registry.lock` 只记录已选项，后续 `rainy pack update --apply` 不会静默安装
Registry 新增的 Skill。

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
rainy agent init --apply
rainy agent context
rainy skill sync --apply
```

`rainy agent init` can be used in an ordinary repository or empty directory. It updates only the
managed block in `AGENTS.md`; when both `rainy.yaml` and `capability.lock` exist, it also refreshes
the project-specific `.enterprise-agent/` context files.

Skill profile 管理：

```bash
rainy skill create release-review --description "Review enterprise releases" --apply
rainy skill install # 自动初始化并进入交互选择
rainy skill install --profile comet --target codex --language zh \
  --skill release-review --dry-run
rainy skill install --profile comet --target codex --language zh \
  --skill release-review --apply
rainy skill install --no-custom-skills --apply # 清空已安装选择，保留 rainy-skills/ 源目录
rainy skill install --yes # --apply 的兼容别名
rainy skill status
rainy skill doctor
rainy skill update --dry-run
rainy skill update --comet-version 0.4.0-beta.6 --skills-version 1.5.20 --superpowers-version 5.1.0 --apply
rainy skill uninstall --dry-run
rainy skill uninstall --apply
```

目前项目 scope 支持始终启用的 `universal`，以及可选的 `codex`、`claude`、`cursor`、`github-copilot`、`gemini`、`opencode`。交互终端可多选平台和项目 Skill，脚本使用重复或逗号分隔的 `--target`、`--skill`。三个上游包均使用精确版本，`skills.lock` 记录 Rainy、上游和项目自定义 Skill 的内容摘要；检测到任何受管副本被手工修改时会拒绝覆盖或卸载，审阅后才能使用 `--force`。源目录 `rainy-skills/` 始终保留并由项目维护。全局宿主安装暂不由 Rainy 管理。每个子命令都提供独立说明和可执行示例，例如 `rainy skill install --help`。命令输出规范见 [`docs/cli-output-style.md`](docs/cli-output-style.md)。

版本检查和更新：

```bash
rainy self check
rainy self check --json
rainy self check --repo owner/repo
rainy self update                                      # 预览
rainy self update --apply                              # 安装最新版
rainy self update --repo owner/repo --version v0.5.2 --apply
rainy self skip 0.5.2                                  # 预览
rainy self skip --repo owner/repo 0.5.2 --apply
```

release 构建出来的非 debug CLI 会周期性检查 GitHub latest release，并在发现新版本时提示：

```text
Rainy CLI update available: 0.1.1 -> 0.2.0.
Run `rainy self update --apply` to update, or `rainy self skip 0.5.2 --apply` to skip this version.
```

自动检查默认行为：

- debug 构建、CI、`--json`、`--quiet` 不会自动输出更新提示。
- 默认 24 小时检查一次。
- 网络检查使用短超时；失败后指数退避，避免阻塞后续命令。
- `RAINY_NO_UPDATE_CHECK=1` 或 `RAINY_SKIP_UPDATE_CHECK=1` 可以关闭自动检查。
- `RAINY_UPDATE_CHECK_INTERVAL_HOURS=0` 可以让每次运行都检查。
- `RAINY_UPDATE_REPO=owner/repo` 可以覆盖 GitHub release 仓库。

## 发布流程

普通分支 push、`main` push、Pull request 和定时任务都不会自动触发 Action。CI 与 Security
workflow 只保留 `workflow_dispatch` 人工诊断入口；只有推送 `vX.Y.Z` tag 才会自动执行完整
安全门禁、五目标构建与 GitHub Release 发布。开发者应在打 tag 前本地执行
`make production-check`，本机必须已安装固定主版本的 `cargo-audit` 与 `cargo-deny`。

GitHub Release 由 `.github/workflows/release.yml` 负责。发版前建议本地先跑：

```bash
make release-check
```

创建并推送版本标签后会触发 release workflow：

```bash
git tag -a v0.5.2 -m "Rainy CLI v0.5.2"
git push origin v0.5.2
```

release workflow 会先执行格式、测试、clippy、audit/deny、schema、MCP wrapper、PTY 和安装脚本检查，然后分别构建并上传：

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

也可以由 Rainy 以项目 scope 安装两个 Skill 和上游组合：`rainy skill install --profile comet --target codex --apply`。该命令在缺少配置时自动生成可提交的 `rainy-skills.yaml` 和 `skills.lock`，调用固定版本 Comet 的官方初始化入口，并强制 `.comet/config.yaml` 中 `auto_transition: false`。Comet 阶段前进不等于批准 Rainy `--apply`。

可以独立验证 bootstrap：

```bash
sh integrations/skills/rainy-cli/scripts/ensure-rainy.sh
```

Windows PowerShell：

```powershell
& integrations/skills/rainy-cli/scripts/ensure-rainy.ps1
```

Rainy 的核心使用方式是“先计划，再应用”：

1. `rainy capability add ... --dry-run` 生成计划、diff 和策略检查结果。
2. 人或 Agent 审阅计划。
3. `rainy capability add ... --apply` 或 `rainy apply --plan ... --apply` 写入文件。
4. CLI 在 apply 前执行 policy gate。
5. 写入失败时回滚已应用文件，避免部分落地。
6. `rainy doctor` 检查项目健康。
7. `rainy verify` 运行验证步骤。
8. `rainy evidence generate --apply` 生成 Markdown/JSON 证据报告。

说明：

- `add capability`、`apply`、`pack install/update`、`plugin install/call` 默认是 dry-run，需要显式 `--apply` 才会写文件。
- `new` / `init app` 和其他变更命令默认只预览；必须传 `--apply` 或同义的 `--yes` 才会写入。
- 所有命令支持全局 `--json`，方便 Agent、MCP、CI 调用。
- `verify --profile local` 适合开发机，缺少本地工具链时可给 warning；`verify --profile ci` 是严格门禁，未知步骤或缺失验证工具会失败。
- 策略会阻止敏感路径、危险命令、需要审批的操作和插件越权写入。
- Wasm 是默认插件运行时，受 1 MiB 输入、5 MiB 输出、64 MiB 内存、1 亿 fuel 和 30 秒限制；原生 `rainy-*` 插件必须在 Rainy 项目内显式授权，默认 300 秒超时并写入审计日志。

## 当前建设进度

已完成：

- Rust CLI 命令树：`new/source/init/add/apply/capability/pack/registry/defaults/doctor/verify/evidence/plugin/agent/skill/schema/conformance/self/completion`。
- Golden Path 初始化：生成 Spring Boot + Next.js 基础项目、`rainy.yaml`、`capability.lock`、AGENTS.md、CI、compose、evidence 目录。
- Capability Pack 解析：本地、Git、HTTPS index、校验后的 HTTPS archive 和全局缓存。
- Rainy Source：自描述根清单、Git/Archive/Index/本地来源、SemVer/commit/digest 感知、不可变用户缓存、项目来源锁、模板与多模块组合。
- 内置 action：Maven、YAML/JSON/JSONC/TOML merge、模板渲染、文件创建/追加、Docker Compose、package.json、devcontainer、Helm draft 等。
- Plan / Diff / Apply：支持 dry-run、plan file、事务式 apply 回滚、幂等 no-op。
- Capability 依赖和 provider 解析：依赖缺失失败、被依赖能力禁止删除、provider 默认/显式/非法场景有稳定错误。
- Policy Gate：内置敏感路径、项目 policy、org policy、capability policy、审批动作、危险命令、插件写权限。
- Doctor / Verify / Evidence：健康检查、能力验证、证据报告、secret 脱敏、默认开发 secret warning。
- 运行时可靠性：统一 `RunContext`、工作区向上发现、真实进度事件、Ctrl+C 子进程组清理、外部命令超时和输出上限。
- 跨平台 Verify：优先使用 `run.program/run.args`，不经过 `sh -c`；旧 `command` 仅兼容无 shell 运算符的简单命令。
- Audit log：修改命令执行前检查审计可写性，成功和失败命令通过文件锁写入 `.rainy/audit.log`。
- Plugin：Wasm action plugin 默认可用；原生 `rainy-*` 需要显式信任；HTTP adapter 受权限、HTTPS/loopback 和响应大小限制。
- Release 安装和自更新：五平台构建与 smoke、强制 checksum、回滚安装、SBOM、provenance、原生 HTTPS 版本检查和 `self check/update/skip`。
- 模型 Skill：Rainy CLI bootstrap Skill、Rainy-Comet bridge Skill、项目级 profile/lock、六类宿主目标、Comet 固定版本安装、内容摘要漂移检查、doctor/update/uninstall。
- MCP 示例：stdio JSON-RPC wrapper 调用 Rainy CLI，默认 dry-run 计划能力接入。
- Backstage 示例：scaffolder actions 和模板示例。
- Schema / conformance：标准 Draft 2020-12 validator、schema list/validate、pack/plugin conformance 检查。
- 企业扩展样例：私有 pack、本地 registry、分层 org policy schema、企业接入边界和 CI 门禁说明。
- CLI 交互规范：命令级 help 示例、统一 Summary/Next step/Checks/Details 层级、独立 JSON 协议和进度显示。
- 测试：包含 unit、E2E 和真实 PTY tests，覆盖 Golden Path、policy、plugin、schema、conformance、事务回滚、自更新、窄终端、多轮选择、取消和子进程清理。
- CI / release 门禁：三系统测试、MSRV、audit/deny、CodeQL、格式、E2E、clippy、schema、安装器测试、JSON smoke、conformance 和五平台 release 构建。

部分完成 / 示例级：

- Backstage 集成目前是示例代码，未打包成可直接发布的 Backstage npm 包。
- MCP wrapper 是最小可运行示例，生产环境还需要接入具体 MCP host 配置、权限边界和部署方式。
- `verify --profile local` 会在本地工具链缺失时对部分外部命令给 warning；生产门禁应使用严格的 `verify --profile ci`。
- Pack 默认生成完整性 manifest；配置 `RAINY_PACK_SIGNING_KEY` / `RAINY_PACK_TRUSTED_PUBLIC_KEY` 后使用 cosign 验证发布者身份。

未包含在当前开源仓库：

- 企业真实 starter / 私有 packs（仓库只提供可运行的结构样例）。
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
- Command reference: [docs/command-reference.md](docs/command-reference.md)
- Enterprise integration: [docs/enterprise-integration.md](docs/enterprise-integration.md)
- Enterprise Git registry authoring: [docs/enterprise-git-authoring.md](docs/enterprise-git-authoring.md)
- Enterprise Source distribution and versioning: [docs/source-management.md](docs/source-management.md)
- CLI output style: [docs/cli-output-style.md](docs/cli-output-style.md)
- Capability Pack authoring: [docs/capability-pack-authoring.md](docs/capability-pack-authoring.md)
- Plugin protocol: [docs/plugin-protocol.md](docs/plugin-protocol.md)
- MCP wrapper: [integrations/mcp](integrations/mcp)
- Model Skill: [integrations/skills/rainy-cli](integrations/skills/rainy-cli)
- Composed workflow Skill: [integrations/skills/rainy-comet](integrations/skills/rainy-comet)
- Skill profile management: [docs/skills-management.md](docs/skills-management.md)
- Release mirrors and OSS: [docs/release-mirrors.md](docs/release-mirrors.md)
- 0.4 to 0.5 migration: [docs/migration-0.4-to-0.5.md](docs/migration-0.4-to-0.5.md)
- Backstage example: [integrations/backstage](integrations/backstage)
- Full design document: [Rainy_CLI_最终形态程序设计与详细开发文档.md](Rainy_CLI_最终形态程序设计与详细开发文档.md)
