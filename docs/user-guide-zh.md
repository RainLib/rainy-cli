# Rainy CLI 使用手册

本手册面向使用 Rainy 创建和维护项目的开发者。企业平台团队如何制作和发布自己的 Source、Pack、
模板和 Skill，见[企业 Git 能力仓库制作规范](enterprise-git-authoring.md)。

## 1. Rainy 是什么

Rainy 管理项目的创建、Capability、企业内容包、Agent Skill、检查、验证和证据。常用工作流是：

```text
创建项目 -> 预览变更 -> 显式应用 -> Doctor -> Verify -> Evidence
```

默认预览不会写项目文件。所有需要写入的非交互命令必须添加 `--apply`；`--yes` 是相同含义的别名。
交互式选择器在最终确认后会直接执行，不需要再输入 `--apply`。

## 2. 安装与检查

macOS 或 Linux：

```bash
curl -fsSL https://github.com/RainLib/rainy-cli/releases/latest/download/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://github.com/RainLib/rainy-cli/releases/latest/download/install.ps1 | iex
```

安装后重新打开终端，或按安装器提示加载 Shell 配置，然后检查：

```bash
rainy --version
rainy --help
rainy self check
rainy defaults status
```

默认安装路径是 `~/.rainy/bin`。若当前终端提示 `command not found: rainy`，先执行：

```bash
export PATH="$HOME/.rainy/bin:$PATH"
rainy --version
```

随后把该 PATH 设置写入当前 Shell 的启动配置，或重新运行安装器。发布镜像/企业 OSS 环境使用
`RAINY_RELEASE_BASE_URL`，详见 [Release mirrors](release-mirrors.md)。

## 3. 命令约定

```bash
rainy [--workspace <PROJECT_DIR>] [--json] [--verbose] [--quiet] \
  [--no-color] [--progress <auto|always|never>] <COMMAND>
```

- `--workspace <PROJECT_DIR>`：明确指定项目根目录。自动化必须传入。
- `--json`：用于 Agent、脚本和 CI；只解析 JSON，不解析人类输出。
- `--verbose`：显示完整检查项、上游命令和路径。
- `--progress always`：强制输出稳定的逐行进度；CI/JSON 不显示动态进度。
- `--apply`：执行写操作；缺省时只预览。
- `--force`：仅在已审阅本地漂移时使用，不会隐含执行。

帮助始终沿命令层级查看：

```bash
rainy --help
rainy new --help
rainy skill --help
rainy registry sync --help
```

## 4. 创建默认企业后端项目

当前企业包中的默认完整后端模板是 `pkulaw-backend-mvc`：Gradle 多模块、Spring Boot 3.x、DDD 分层。
它默认使用 SSH 拉取上游模板；HTTP 可用，但必须先由 Git credential helper 配置内部 Git 凭据。

### 4.1 一次性注册企业 Source

在要存放新项目的父目录执行：

```bash
mkdir -p ~/workspace/company
cd ~/workspace/company

rainy source inspect \
  git+ssh://git@192.168.0.161/guochanhua/product-develop-group/back-end-group/infrastructure/enterprise-cli-package.git \
  --ref master

rainy source add enterprise \
  git+ssh://git@192.168.0.161/guochanhua/product-develop-group/back-end-group/infrastructure/enterprise-cli-package.git \
  --ref master --apply
```

`master` 适合当前开发验证。企业包发布了并可访问不可变 Tag 后，生产项目和 CI 应改为该 Tag，例如
`--ref v0.1.1`。Tag 必须真实存在于远程仓库。

Source 缓存在 `~/.rainy/sources/`，不会把企业包目录复制到业务项目中。

### 4.2 交互创建

在人工终端直接运行：

```bash
rainy new order-service
```

依次选择：企业 Source 的模板目录、`pkulaw-backend-mvc`、下载方式 `ssh`。最终确认后创建项目。

### 4.3 自动化创建

脚本或 CI 不会打开选择器，必须显式指定模板、模板目录和远程方式：

```bash
CATALOG_ROOT="$(rainy source resolve enterprise enterprise-project-templates --json \
  | jq -r '.data.report.sources[0].contents[0].resolvedPath')"

rainy new order-service \
  --template pkulaw-backend-mvc \
  --template-config "$CATALOG_ROOT/project-templates.yaml" \
  --template-remote ssh \
  --git-url git@192.168.0.161:guochanhua/product-develop-group/back-end-group/order-service.git \
  --apply
```

没有 `jq` 时，先执行以下命令，从 JSON 输出中取得 `resolvedPath`，再把该路径传给
`--template-config`：

```bash
rainy source resolve enterprise enterprise-project-templates --json
```

HTTP 方式仅替换远程参数：

```bash
rainy new order-service \
  --template pkulaw-backend-mvc \
  --template-config <CATALOG_ROOT>/project-templates.yaml \
  --template-remote http --apply
```

HTTP 失败并提示无法读取 Username 时，先配置 Git credential helper 或改用推荐的 SSH 方式。不要把账号、
密码或 Token 写进 URL。

Rainy 会在临时目录拉取模板、记录实际 commit、移除模板仓库的 `.git`，再生成业务目录。它不会创建远程
Git 仓库或自动推送。创建完成后执行：

```bash
cd order-service
git init -b main
git remote add origin git@192.168.0.161:guochanhua/product-develop-group/back-end-group/order-service.git
git add .
git commit -m 'Initial commit'
git push -u origin main
```

### 4.4 后端模板结构与首次验证

默认模板主要模块：

| 模块 | 作用 |
| --- | --- |
| `domain` | 领域模型与端口，不依赖项目其他模块 |
| `application` | 用例和应用服务，仅依赖 `domain` |
| `infrastructure` | 端口实现，仅依赖 `domain` |
| `interfaces` | GraphQL/gRPC 等入站适配器 |
| `starter` | Spring Boot 启动和装配 |
| `proto` | Protobuf/gRPC 代码生成 |
| `feign` | Feign 客户端接口 |

创建后先运行：

```bash
make build
make jar
rainy doctor --scope auto
rainy verify --profile local
rainy template status
rainy template check
```

`template status` 只读取本地来源锁；`template check` 会比较上游分支是否变化，但不会自动修改项目。

## 5. 项目内文件说明

以下文件属于项目契约，应提交到业务 Git：

```text
rainy.yaml                  项目配置、Policy、Registry 配置
capability.lock             已应用 Capability 的锁定状态
rainy-skills.yaml           Skill 期望配置（启用 Skill 后出现）
skills.lock                 Rainy 安装的 Skill 状态（启用 Skill 后出现）
AGENTS.md                   Rainy 管理的 Agent 上下文块
.rainy/project-template.lock 模板来源、请求 ref 和实际 commit
.rainy/registry.lock        已同步企业 Pack 和企业 Skill 锁
```

以下是 Rainy 的运行状态，应保留在 `.rainy/`，不应堆在根目录：

```text
.rainy/plans/                       通过 --output-plan 保存的变更计划
.rainy/skills/upstream-lock.json     Rainy 调用 skills CLI 后生成的上游索引
.rainy/audit.log                     审计记录
.rainy/reports/                      人工保存的 JSON 报告的推荐位置
```

相对 `--plan` 和 `--output-plan` 路径始终相对 `--workspace` 解析：

```bash
rainy capability add minio-file-storage \
  --output-plan .rainy/plans/minio.json
rainy apply --plan .rainy/plans/minio.json --apply
```

Pkulaw 上游模板自身目前包含根目录 `skills-lock.json`。这是模板业务内容，不是 Rainy 运行时生成文件；
是否迁移或移除应由模板仓库维护方决定。

## 6. Capability 工作流

先了解能力，再预览，再执行：

```bash
rainy capability list
rainy capability explain minio-file-storage

rainy capability add minio-file-storage --provider minio \
  --output-plan .rainy/plans/minio.json

rainy apply --plan .rainy/plans/minio.json --apply
rainy doctor --scope project
rainy verify --profile local
```

直接执行但仍先查看计划：

```bash
rainy capability add minio-file-storage --provider minio --apply
```

升级、移除和 CI 检查：

```bash
rainy capability upgrade minio-file-storage
rainy capability upgrade minio-file-storage --apply
rainy capability remove minio-file-storage --apply
rainy verify --profile ci
rainy evidence generate --format all --apply
```

## 7. 安装 Rainy 与企业 Skills

### 7.1 Rainy 自身 Skill

人工终端中，以下命令会在缺少配置时自动初始化，并交互选择工作流、平台和项目自定义 Skill：

```bash
rainy skill install
```

仅安装 Rainy 执行 Skill，不依赖 Node.js：

```bash
rainy skill install --profile rainy --target codex --no-custom-skills --apply
```

完整工作流使用 OpenSpec、Superpowers 和 Comet，需要 Node.js 20+、npm/npx 和 Git：

```bash
rainy skill install --profile comet --language zh \
  --target codex,claude,cursor --no-custom-skills --apply
```

常用维护命令：

```bash
rainy skill status
rainy skill doctor
rainy skill update --apply
rainy skill uninstall --apply
```

### 7.2 项目自定义 Skill

创建项目专属规则或工具说明：

```bash
rainy skill create release-review \
  --description "Review enterprise releases" --apply

# 编辑 rainy-skills/release-review/SKILL.md 后安装
rainy skill install --skill release-review --apply
```

Rainy 只复制 Skill 文件，不会在安装时执行其中的脚本。

### 7.3 企业 Pack 导出的 Skill

在已创建的业务项目根目录注册并同步企业 Pack：

```bash
rainy registry add enterprise \
  git+ssh://git@192.168.0.161/guochanhua/product-develop-group/back-end-group/infrastructure/enterprise-cli-package.git \
  --ref master --apply

rainy registry sync enterprise \
  --module enterprise-governance \
  --install-skills \
  --target codex,claude,cursor \
  --skill enterprise-rainy \
  --apply
```

`enterprise-rainy` 会安装到 `.agents/skills`、`.claude/skills` 和 `.cursor/skills` 的对应目录。

当前 `dependencies-gradle-common` 的内部地址尚未提供 `skills` 工具要求的标准 Skill 端点或可发现的
`SKILL.md`，因此不要在生产流程中选择它。该源完成标准化后，再执行：

```bash
rainy registry sync enterprise \
  --module enterprise-governance \
  --install-skills --target codex \
  --skill dependencies-gradle-common --apply
```

## 8. 更新、检查和恢复

Rainy 不会自动覆盖业务代码。更新操作先检查，再刷新用户缓存或下载内容，项目升级应通过正常 PR 完成。

```bash
# CLI 自身更新
rainy self check
rainy self update
rainy self update --apply

# 官方默认内容
rainy defaults status
rainy defaults update --apply

# 企业 Source：检查与刷新缓存
rainy source check enterprise
rainy source update enterprise --apply

# 当前项目的模板和 Registry
rainy template check
rainy registry doctor enterprise
rainy registry sync enterprise --module enterprise-governance --apply
```

`source update` 只更新 `RAINY_HOME` 中经过校验的缓存；`template check`、Registry 同步也不会自动把新
模板覆盖到已有项目。需要采用上游变更时，创建专门分支、比较锁定 commit 与新版本，再手工迁移并验证。

## 9. CI 与 Agent 用法

CI、Agent 或脚本必须关闭交互，传入完整参数并使用 JSON：

```bash
rainy --workspace "$WORKSPACE" --json capability add minio-file-storage \
  --provider minio --output-plan .rainy/plans/minio.json

rainy --workspace "$WORKSPACE" --json apply \
  --plan .rainy/plans/minio.json --apply

rainy --workspace "$WORKSPACE" --json doctor --scope all
rainy --workspace "$WORKSPACE" --json verify --profile ci
rainy --workspace "$WORKSPACE" --json evidence generate --format all --apply
```

正常和检查不通过报告写到 stdout；参数、配置、网络和完整性错误写到 stderr。退出码：

| 退出码 | 含义 |
| --- | --- |
| `0` | 完成、预览或 warning |
| `1` | 运行或 I/O 错误 |
| `2` | 参数或配置错误 |
| `3` | Policy 或审批拒绝 |
| `4` | Doctor、Verify、Schema 或 Conformance 检查失败 |
| `5` | 网络或认证错误 |
| `6` | 摘要或签名完整性错误 |
| `130` | 用户取消 |

## 10. 常见问题

### `rainy` 找不到

确认 `~/.rainy/bin` 位于 PATH，重新打开终端后执行 `rainy --version`。

### `CONFIG_NOT_FOUND: rainy.yaml not found`

当前目录不是完整 Rainy 项目。创建项目使用 `rainy new <NAME>`；只管理 Skill 时可直接执行
`rainy skill install`；只写 Agent 上下文可执行 `rainy agent init --apply`。

### `DEFAULTS_GIT_FETCH_FAILED`

先执行 `rainy defaults status`。若错误指向一个不存在的发布 Tag，发布方必须先发布对应 Tag，或由平台
团队设置可访问的 `RAINY_DEFAULTS_SOURCE` 和 `RAINY_DEFAULTS_REF`；不要靠跳过校验继续执行。

### HTTP 模板拉取提示无法读取 Username

配置 Git credential helper，或改用 `--template-remote ssh` 并确认 SSH agent/key 可以访问内部 Git。

### 模板或 Skill 有本地修改，命令拒绝继续

先查看差异和 lock，再决定保留、手工迁移或在已审阅后使用 `--force`。不要在 CI 中无条件添加 `--force`。

## 11. 进一步阅读

- [命令参考](command-reference.md)
- [Skill 管理](skills-management.md)
- [Source 内容分发与版本管理](source-management.md)
- [企业 Git 能力仓库制作规范](enterprise-git-authoring.md)
- [CLI 输出规范](cli-output-style.md)
- [架构与流程](architecture-and-flow.md)
