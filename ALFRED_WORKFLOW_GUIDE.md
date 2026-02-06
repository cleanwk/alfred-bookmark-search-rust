# Alfred Workflow 配置指南

## 1. 生成可安装包

```bash
./scripts/bootstrap_workflow.sh -- --version 0.1.0
```

产物：`dist/AlfredChromeBookmarks.alfredworkflow`

## 2. 安装

```bash
open dist/AlfredChromeBookmarks.alfredworkflow
```

## 3. 验证

在 Alfred 中执行：

- `cb rust`
- `cb folder:work/project rust`

## 4. 可选变量

在 workflow Variables 中可设置：

- `BINARY_PATH`: 指定二进制路径

在 shell 中可设置：

- `ALFRED_CHROME_BOOKMARKS_PATH`: 指定书签文件路径

```bash
export ALFRED_CHROME_BOOKMARKS_PATH="$HOME/Library/Application Support/Arc/Default/Bookmarks"
```

## 5. 常见问题

- `Binary not found`: 设置 `BINARY_PATH` 或重新打包 workflow。
- 无结果: 先运行 `alfred-chrome-bookmarks refresh` 再试。
