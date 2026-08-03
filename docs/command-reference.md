# Rainy CLI 使用手册

本文档按实际工作流说明命令。参数中的 `<VALUE>` 是必填值，`[VALUE]` 是可选值；
任何层级都可以执行 `rainy <command> --help` 查看用途、参数和可运行示例。

## 全局约定

```bash
rainy [--workspace <PROJECT_DIR>] [--json] [--verbose] [--quiet] \
  [--no-color] [--progress <auto|always|never>] [--trace-id <TRACE_ID>] <COMMAND>
```

- `--workspace` 指向包含 `rainy.yaml` 的项目根目录，省略时使用当前目录。
- 人工操作使用默认输出；Agent、脚本和 CI 必须使用 `--json`。
- `--verbose` 展开成功检查、上游命令和完整路径。
- `--progress auto` 仅为可能耗时的命令在交互终端显示动态进度；`always` 输出逐行进度；`never` 关闭。
- `--no-color`、`NO_COLOR` 或 `TERM=dumb` 禁用颜色；`TERM=dumb` 还禁用动态终端绘制。
- 会修改项目的命令先 preview，再以 `--apply` 执行。不要把 dry-run 当成已完成。
- 同一业务请求使用一个 `--trace-id`，便于关联 `.rainy/audit.log`。

## 从零创建项目

```bash
# 预览
rainy new demo-saas --golden-path spring-nextjs-saas \
  --package com.example.demo --dry-run

# 创建
rainy new demo-saas --golden-path spring-nextjs-saas \
  --package com.example.demo --apply

cd demo-saas
rainy doctor
rainy verify --profile local
```

### 从企业 Git 模板创建

企业 starter 在创建项目之前没有项目级 `rainy.yaml`，因此模板源声明在独立的
`ProjectTemplateCatalog` 中。配置查找顺序为：

1. `--template-config <CATALOG_FILE>`
2. `RAINY_TEMPLATE_CONFIG`
3. `RAINY_HOME/templates.yaml`
4. `~/.rainy/templates.yaml`

```yaml
apiVersion: rainy.dev/v1
kind: ProjectTemplateCatalog
templates:
  enterprise-java-service:
    description: Standard enterprise Java service
    source:
      type: git
      url: ssh://git@git.example.com/platform/project-templates.git
      ref: v1.2.0
    subdirectory: java-service
    repository:
      defaultBranch: main
      remoteUrl: "git@git.example.com:apps/{{ project.name }}.git"
```

验证并创建：

```bash
rainy schema validate --schema project-template-catalog \
  --file ~/.rainy/templates.yaml

# 省略 --apply 时企业模板始终只预览，不访问 Git
rainy new order-service --template enterprise-java-service \
  --package com.company.orders --dry-run

rainy new order-service --template enterprise-java-service \
  --package com.company.orders \
  --git-url git@git.example.com:apps/order-service.git --apply
```

`--git-url` 覆盖配置中的 `repository.remoteUrl`。它只用于生成下一步，不会让 Rainy 自动创建远程仓库
或推送代码。Rainy 克隆并锁定 `source.ref` 的解析 commit，只复制选定 `subdirectory` 的安全普通文件，
拒绝符号链接、路径穿越、目标冲突、超限目录和缺失 `rainy.yaml`/`capability.lock` 的模板。以 `.hbs`
结尾的 UTF-8 文件内容和路径支持 `project.name`、`package.java` 和 `packagePath`，后缀会从目标文件名
移除；其他文件按原始字节复制，避免误解释业务代码中的大括号。

成功输出中的 `next_commands` 包含：

```bash
cd <PROJECT_DIR>
git init -b <DEFAULT_BRANCH>
git remote add origin <PROJECT_GIT_URL>
git add .
git commit -m 'Initial commit'
git push -u origin <DEFAULT_BRANCH>
```

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
rainy evidence generate --format all
```

- `doctor` 检查配置、lock、生成物、默认 secret 和 capability 自检。
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
rainy agent init
rainy agent context
rainy skill sync
```

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

内置 schema 覆盖项目、capability、pack、registry、plan、plugin、Skill profile 和企业 policy。

## 自更新

```bash
rainy self check
rainy self update
rainy self update --version <VERSION>
rainy self skip [VERSION]
```

`self update` 下载对应平台的 Release 安装器，验证 checksum，安装后再次验证二进制版本。
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

命令参数缺失或无效时返回退出码 `2` 和 `CLI_ARGUMENT_INVALID`；未知顶层命令返回
`EXTERNAL_COMMAND_NOT_FOUND`。两者在 `--json` 下将单个错误对象写入 `stderr`，`stdout`
保持为空。用户按 `Ctrl+C` 时退出码为 `130`。

企业私有能力的组织方式见 [enterprise-integration.md](enterprise-integration.md)。
