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
- `cbf rsut`
- `cba`
- 说明：`cb` 空查询默认只显示书签；`refresh/stats` 等动作请使用 `cba`。

默认热键：

- `⌃⌥⌘B`：直接触发主搜索（可在 Alfred 中修改）

结果操作：

- `↩` 打开 URL
- `⌘↩` 复制 URL
- `⌥` 查看目录信息（不执行）

## 4. 可选变量

在 workflow Variables 中可设置：

- `BINARY_PATH`: 指定二进制路径
- `RESULT_LIMIT`: `cb` 默认返回条数（默认 `36`）
- `FUZZY_LIMIT`: `cbf` 默认返回条数（默认 `24`）
- `ALFRED_CHROME_BOOKMARKS_BROWSER`: 指定只搜索某个浏览器（如 `chrome` / `dia` / `arc`）

在 shell 中可设置：

- `ALFRED_CHROME_BOOKMARKS_PATH`: 指定书签文件路径
- `ALFRED_CHROME_BOOKMARKS_BROWSER`: 指定浏览器来源（优先级低于 `ALFRED_CHROME_BOOKMARKS_PATH`）

```bash
export ALFRED_CHROME_BOOKMARKS_PATH="$HOME/Library/Application Support/Arc/Default/Bookmarks"
export ALFRED_CHROME_BOOKMARKS_BROWSER="dia"
```

`ALFRED_CHROME_BOOKMARKS_BROWSER` 为空或设为 `all` 时，会恢复自动扫描全部受支持浏览器。

## 5. 常见问题

- `Binary not found`: 设置 `BINARY_PATH` 或重新打包 workflow。
- 无结果: 先运行 `alfred-chrome-bookmarks refresh` 再试。
- 热键冲突: 在 Alfred Workflow 编辑器中修改 Hotkey Trigger。
