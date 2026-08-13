# Rainy CLI 使用手册

本文档按实际工作流说明命令。参数中的 `<VALUE>` 是必填值，`[VALUE]` 是可选值；
任何层级都可以执行 `rainy <command> --help` 查看用途、参数和可运行示例。

## 全局约定

```bash
rainy [--workspace <PROJECT_DIR>] [--json] [--verbose] [--quiet] \
  [--no-color] [--progress <auto|always|never>] [--trace-id <TRACE_ID>] <COMMAND>
```

- `--workspace` 显式指定精确项目根；省略时从当前目录向上查找最近的 `rainy.yaml` 或
  `rainy-skills.yaml`，但不会越过最近 Git 根。
- 人工操作使用默认输出；Agent、脚本和 CI 必须使用 `--json`。
- `--verbose` 展开成功检查、上游命令和完整路径。
- `--progress auto` 仅为可能耗时的命令在交互终端显示动态进度；`always` 输出逐行进度；`never` 关闭。
- `--no-color`、`NO_COLOR` 或 `TERM=dumb` 禁用颜色；`TERM=dumb` 还禁用动态终端绘制。
- 会修改项目的命令先 preview，再以 `--apply` 执行。不要把 dry-run 当成已完成。
- `--yes` 是变更命令一致的 `--apply` 别名；`--force` 只允许覆盖已审阅的漂移，不会隐含执行。
- 同一业务请求使用一个 `--trace-id`，便于关联 `.rainy/audit.log`。

## 从零创建项目

```bash
# 人工终端：选择内置 Golden Path、已缓存 Rainy Source 或可发现的企业 Git 模板
rainy new demo-saas

# 预览
rainy new demo-saas --golden-path spring-nextjs-saas \
  --package com.example.demo --dry-run

# 创建
rainy new demo-saas --golden-path spring-nextjs-saas \
  --package com.example.demo --apply

cd demo-saas
rainy doctor --scope auto
rainy doctor --scope project
rainy doctor --scope all --network
rainy verify --profile local
```

### 从企业 Git 模板创建

企业 starter 在创建项目之前没有项目级 `rainy.yaml`，因此模板源声明在独立的
`ProjectTemplateCatalog` 中。配置查找顺序为：

1. `--template-config <CATALOG_FILE>`
2. `RAINY_TEMPLATE_CONFIG`
3. 当前创建目录下的 `project-templates.yaml`
4. `RAINY_HOME/templates.yaml`（默认 `~/.rainy/templates.yaml`）

此外，交互执行 `rainy new <APP_NAME>` 时会聚合所有已同步 Rainy Source 中声明的
`project-template-catalog`，并在模板选项中显示 Source 名称和版本。自动化调用先通过
`rainy source resolve <SOURCE> <CATALOG_CONTENT>` 取得 `resolvedPath`，再显式传
`--template-config <RESOLVED_PATH>/project-templates.yaml`。

当人工终端执行 `rainy new <APP_NAME>` 且未指定 `--golden-path`、`--source` 或 `--template` 时，
Rainy 只展示当前可用的创建方式：内置 Golden Path、具有完整校验缓存的 Rainy Source，以及按上述
规则发现的企业模板 catalog。JSON、CI、管道和重定向输入不会打开选择器；必须显式传入对应参数。

```yaml
apiVersion: rainy.dev/v1
kind: ProjectTemplateCatalog
templates:
  enterprise-backend:
    description: Standard enterprise backend service
    source:
      type: git
      ref: main
      defaultRemote: ssh
      remotes:
        ssh:
          description: SSH key or agent authentication
          url: git@git.example.com:platform/backend-template.git
        http:
          description: Private HTTP through the Git credential helper
          url: http://192.168.0.20/platform/backend-template.git
          allowInsecureHttp: true
    overlay: overlays/enterprise-backend
    textReplacements:
      - path: settings.gradle
        find: "rootProject.name = 'service-template'"
        replace: "rootProject.name = '{{ project.name }}'"
    repository:
      defaultBranch: main
      remoteUrl: "git@git.example.com:apps/{{ project.name }}.git"
```

验证并创建：

```bash
rainy schema validate --schema project-template-catalog \
  --file ~/.rainy/templates.yaml

# 省略 --apply 时企业模板始终只预览，不访问 Git
rainy new order-service --template enterprise-backend

rainy new order-service --template enterprise-backend \
  --template-remote ssh \
  --package com.company.orders \
  --git-url git@git.example.com:apps/order-service.git --apply
```

TTY 中存在多个 `source.remotes` 时，省略 `--template-remote` 会打开下载方式选择器。CI、Agent 和重定向
输入必须显式传该参数，或由 catalog 声明 `defaultRemote`。`allowInsecureHttp` 只允许 catalog 明确配置的
私网/回环 IP；公网 HTTP、内嵌凭据和敏感查询参数始终拒绝。Git 凭据应由 SSH agent 或 credential
helper 提供。

创建成功会写入 `.rainy/project-template.lock`，记录模板 ID、下载方式、URL、请求 ref 和实际 commit：

```bash
# 只读取本地来源锁
rainy template status

# 只读访问 Git remote；变化或断网均不修改项目
rainy template check
rainy template check --json
```

上游 ref 变化返回 warning 和 `updateAvailable: true`；网络不可达返回 warning 和
`updateAvailable: null`。Rainy 不自动合并模板更新，业务项目必须把迁移作为正常代码变更审查。

`--git-url` 覆盖配置中的 `repository.remoteUrl`。它只用于生成下一步，不会让 Rainy 自动创建远程仓库
或推送代码。Rainy 克隆并锁定 `source.ref` 的解析 commit，只复制选定 `subdirectory` 的安全普通文件，
拒绝符号链接、路径穿越、目标冲突、超限目录和缺失 `rainy.yaml`/`capability.lock` 的模板。以 `.hbs`
结尾的 UTF-8 文件内容和路径支持 `project.name`、`package.java` 和 `packagePath`，后缀会从目标文件名
移除；其他文件按原始字节复制，避免误解释业务代码中的大括号。`overlay` 是 catalog 同目录下的安全
相对目录，在上游模板之后合并，用于补充 Rainy 文件或覆盖少量企业适配文件，而无需 fork 上游模板。
`textReplacements` 对上游 UTF-8 文件执行带匹配数量断言的精确替换；上游原文变化时创建会失败，不会
用过期 overlay 静默覆盖整份文件。

成功输出中的 `next_commands` 包含：

```bash
cd <PROJECT_DIR>
git init -b <DEFAULT_BRANCH>
git remote add origin <PROJECT_GIT_URL>
git add .
git commit -m 'Initial commit'
git push -u origin <DEFAULT_BRANCH>
```

### 从自描述企业 Source 创建

推荐的新接入方式要求 Git 仓库或 Archive 根目录包含 `rainy-source.yaml`。Source 配置、版本锁和内容
缓存位于 `RAINY_HOME`，不会把整个企业分发仓库放入当前目录：

```bash
rainy source inspect \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0
rainy source add company \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0 --apply

rainy source list
rainy source check company
rainy new order-service --source company --template service-base \
  --module backend-java,delivery-gitlab \
  --package com.company.orders --apply
```

Source 中可声明多个根模板和多个 `workspace-module`。交互终端省略 `--template`/`--module` 时执行
选择器；CI、JSON 或重定向环境必须显式提供模板，未选择的非必需模块不会安装。生成项目通过
`.rainy/project-source.lock` 固定 Source 版本、revision、digest、模板和模块：

```bash
cd order-service
rainy source check --project
rainy source update --project          # 检查
rainy source update --project --apply  # 只刷新用户缓存
rainy source check --project
```

项目锁还保存不含凭据的来源地址和 ref/channel。新开发者克隆项目后可以直接检查；执行
`source update --project --apply` 会在该用户的 `RAINY_HOME` 恢复验证缓存和 Source 关联。跨机器恢复
适用于 Git/Index/Archive；本地目录 Source 需要在新机器重新关联可访问路径。

缓存更新不会覆盖已生成的项目文件。出现 `project-update-available` 时应对照新模板或模块通过 PR
迁移。解析其他已校验内容：

```bash
rainy source resolve company observability
rainy source resolve company observability --json
```

Git `main` 通过远端 commit 感知变化；发布索引直接报告 channel 中的新 SemVer；直接 Archive 依靠
配置摘要或 `<URL>.sha256`。远端暂时不可达且已有缓存时返回 warning 并继续使用旧缓存。完整清单、
发布索引和企业步骤见 [source-management.md](source-management.md)。

`rainy init app` 是兼容的 preset 初始化入口：

```bash
rainy init app demo-saas --preset spring-nextjs --package com.example.demo --apply
```

## 发现与管理 Capability

```bash
rainy capability list
rainy capability add <CAPABILITY_ID> --provider <PROVIDER> --dry-run
rainy capability explain <CAPABILITY_ID>
rainy capability graph
rainy capability installed
```

推荐把计划保存成文件后再应用，避免审阅内容与执行内容不一致：

```bash
rainy capability add <CAPABILITY_ID> --provider <PROVIDER> \
  --dry-run --output-plan .rainy/plans/<CAPABILITY_ID>.json

rainy apply --plan .rainy/plans/<CAPABILITY_ID>.json --apply
rainy doctor --capability <CAPABILITY_ID>
rainy verify --profile local --capability <CAPABILITY_ID>
```

Relative `--plan` and `--output-plan` paths are resolved from the selected workspace. Use an absolute path only when a plan intentionally belongs outside that workspace.

升级和删除同样默认只预览：

```bash
rainy capability upgrade <CAPABILITY_ID> --dry-run
rainy capability upgrade <CAPABILITY_ID> --apply
rainy capability remove <CAPABILITY_ID> --dry-run
rainy capability remove <CAPABILITY_ID> --apply
```

`--force` 只能在审阅冲突后使用，不能用于绕过 policy。

旧版 `rainy add capability ...` 仍可执行，但已从主帮助隐藏；新脚本统一使用
`rainy capability add ...`，使新增、升级和删除都位于同一资源命令下。

## 管理官方默认内容

```bash
rainy defaults status
rainy defaults install --dry-run
rainy defaults install --apply
rainy defaults update --apply
rainy defaults doctor
```

默认源是 `RainLib/rainy-cli` 的当前 CLI 版本 tag。可通过 `RAINY_DEFAULTS_SOURCE`、
`RAINY_DEFAULTS_REF` 使用企业 Git 镜像；内容固定在 `RAINY_HOME/defaults`，`RAINY_OFFLINE=1`
时只允许使用已验证缓存。

## 管理 Capability Pack

```bash
rainy pack list
rainy pack inspect <PACK_ID>
rainy pack install <LOCAL_DIR|GIT_URL|HTTPS_URL> --dry-run
rainy pack install <LOCAL_DIR|GIT_URL|HTTPS_URL> --apply
rainy pack update --dry-run
rainy pack update --apply
```

命名 Registry 与模块同步：

```bash
rainy registry add <NAME> git+https://git.example.com/team/packs.git --ref <TAG> --apply
rainy registry add <NAME> https://packages.example.com/packs.tar.gz --sha256 <SHA256> --apply
rainy registry sync <NAME> --module <PACK>[,<PACK>...] --dry-run
rainy registry sync <NAME> --module <PACK>[,<PACK>...] --apply
rainy registry sync <NAME> --module <PACK> --install-skills --apply
rainy registry sync <NAME> --module <PACK> --install-skills \
  --target codex,cursor --skill <SKILL_ID>[,<SKILL_ID>...] --apply
rainy registry sync --all-registries --all --apply
rainy registry doctor [NAME]
```

交互终端使用 `--install-skills --apply` 时会先多选
`--target <universal|codex|claude|cursor|github-copilot|gemini|opencode>`，再多选所选 Pack 声明的企业
Skill。CI、Agent 和 JSON 模式不会提示，必须显式传递 `--target`，并用可重复的
`--skill <SKILL_ID>` 精确选择或用 `--all-skills` 安装全部导出项。切换选择会删除未修改的旧受管
Skill；有本地修改时必须先审查，命令会拒绝继续，只有明确指定 `--force` 才会替换。
缓存固定在 `RAINY_HOME/registries`（默认 `~/.rainy/registries`），项目只记录锁信息。

发布前检查 pack：

```bash
rainy schema validate --schema capability-pack --file <PACK_DIR>/pack.yaml
rainy conformance check --path <PACK_DIR>
rainy pack sign <PACK_DIR>
rainy pack verify <PACK_DIR>
```

签名使用 `RAINY_PACK_SIGNING_KEY`，消费端通过
`RAINY_PACK_TRUSTED_PUBLIC_KEY` 强制验证发布者身份。

## 健康、验证和证据

```bash
rainy doctor
rainy verify --profile local
rainy verify --profile ci
rainy evidence generate --format all --apply
```

- `doctor` 支持 `auto|project|skills|runtime|defaults|registries|all`。默认只组合本地发现到的
  配置，网络探测必须显式传 `--network`。
- `local` 用于开发机，可把缺失的外部工具报告为 warning。
- `ci` 是严格门禁，生产流水线应以其退出码为准。
- `evidence` 将交付事实输出到 `rainy.yaml` 中配置的 evidence 目录。

## Plugin

```bash
rainy plugin list
rainy plugin inspect <PLUGIN_ID>
rainy plugin install <PLUGIN_SOURCE> --dry-run
rainy plugin install <PLUGIN_SOURCE> --apply
rainy plugin call <PLUGIN_ID> <ACTION> --input <INPUT_FILE> --dry-run
rainy plugin call <PLUGIN_ID> <ACTION> --input <INPUT_FILE> --apply
```

优先使用 Wasm action plugin 或 HTTPS adapter。原生插件拥有宿主进程权限，只有在代码、
manifest 和权限均完成审阅后才可显式启用 `--allow-native-plugin`。

## Agent 与 Skills

```bash
rainy agent init --apply
rainy agent context
rainy skill sync --apply
```

`rainy agent init --apply` is valid in any directory and writes only `AGENTS.md` until the workspace
is a complete Rainy project (`rainy.yaml` and `capability.lock`). Complete projects also receive
`.enterprise-agent/` context files.

创建当前项目拥有的 Skill 规则和命令包：

```bash
rainy skill create release-review \
  --description "Review enterprise releases" --apply
```

模板位于 `rainy-skills/release-review/`，包含 `SKILL.md`、`references/` 和 `scripts/`。
Rainy 只安装选中的目录并校验摘要，不会在安装阶段执行用户脚本。

交互式安装统一使用：

```bash
rainy skill install
```

缺少 `rainy-skills.yaml` 时会自动初始化。Rainy 依次选择 bundle、Codex/Claude/Cursor
等宿主和 `rainy-skills/` 中的项目 Skill，始终安装 Universal `.agents/skills`，最后单独
确认是否安装。已有 profile 时 bundle 和宿主保持不变，只重新选择项目 Skill。
脚本、Agent 和 CI 不进入交互，必须显式指定：

该流程不要求 `rainy.yaml`。普通仓库只维护 Skill profile、lock、`AGENTS.md` 和宿主目录；
完整 Rainy 工程仍会同步 capability 与企业 Agent 上下文。

```bash
rainy skill install --profile comet --language zh \
  --target codex,claude,cursor --skill release-review --dry-run --json
rainy skill install --profile comet --language zh \
  --target codex,claude,cursor --skill release-review --apply --json
rainy skill status --json
rainy skill doctor --json
```

`--profile`、`--language`、`--target` 和版本参数仅用于缺少 profile 的首次安装。已有
profile 时只传 `--skill`、`--all-custom-skills` 或 `--no-custom-skills`；省略自定义选择会
保留当前选择。`--no-custom-skills` 清空安装选择，但不会删除 `rainy-skills/` 中的源目录。

仅需要 Rainy 执行 Skill 时使用 `--profile rainy`，不依赖 Node.js：

```bash
rainy skill install --profile rainy --target codex --apply
```

生命周期命令：

```bash
rainy skill init [--profile <rainy|comet>] [--target <AGENT_HOST>] [--apply]
rainy skill install [--skill <SKILL_ID> | --all-custom-skills | --no-custom-skills] \
  [--apply] [--force]
rainy skill update [--comet-version <VERSION>] \
  [--skills-version <VERSION>] [--superpowers-version <VERSION>] [--apply]
rainy skill uninstall [--apply] [--force]
```

`--force` 仅用于处理已审阅的受管文件漂移。

## Schema 与 Conformance

```bash
rainy schema list
rainy schema validate --schema <SCHEMA_ID> --file <DOCUMENT_FILE>
rainy conformance check --path <PACK_OR_PLUGIN_DIR>
```

内置 schema 覆盖项目、capability、pack、registry、Source、plan、plugin、Skill profile 和企业 policy。

## 自更新

```bash
rainy self check
rainy self update
rainy self update --apply
rainy self update --version <VERSION> --apply
rainy self skip [VERSION]
rainy self skip [VERSION] --apply
```

`self update` 和 `self skip` 默认只预览并返回可执行的 `applyCommand`；`--apply` 或 `--yes` 才修改
二进制或更新状态。`--force` 只允许重装，不隐含执行。执行 update 时会下载对应平台的 Release
安装器，验证 checksum，安装后再次验证二进制版本。
可通过 `--repo <OWNER/REPO>` 或 `RAINY_UPDATE_REPO` 使用受信任的企业镜像仓库。

## Shell 补全

```bash
rainy completion <bash|elvish|fish|powershell|zsh>
source <(rainy completion zsh)
```

普通模式只向 `stdout` 写补全脚本，不显示进度，也不写项目审计日志。使用 `--json` 时返回
包含 `shell` 和 `script` 的结构化结果。

## 自动化规则

自动化必须满足以下约束：

1. 显式传 `--workspace`、`--json` 和所有影响行为的选项。
2. 先保存并审阅 dry-run plan，再执行该 plan。
3. 不解析人类输出；只解析 JSON、稳定 error code 和进程退出码。
4. apply 后运行 `doctor`、严格 `verify` 和 evidence。
5. policy、checksum、签名或 verify 失败时停止，不自动添加 `--force` 或原生插件信任。

命令参数缺失、拼写错误或无效时返回退出码 `2` 和 `CLI_ARGUMENT_INVALID`，并保留 Clap 的
相似命令建议。只有确实存在的已安装 `rainy-<name>` 插件才会接管顶层快捷命令。操作错误在
`--json` 下将 `rainy.command.v1` 错误对象写入 `stderr`，`stdout` 保持为空；Doctor、Verify、
Schema、Conformance 的检查失败将完整报告写入 `stdout` 并退出 `4`。用户按 `Ctrl+C` 时退出
`130`。

企业私有能力的组织方式见 [enterprise-integration.md](enterprise-integration.md)。
