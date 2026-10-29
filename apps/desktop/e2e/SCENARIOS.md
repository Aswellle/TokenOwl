# TokenOwl 交互测试边界与场景矩阵

覆盖三层验证：**A 单元/组件**（vitest，jsdom + mock bridge，确定性）、**B e2e**（Playwright，
mock 预览的 webview 层，真实浏览器渲染）、**C 实机**（原生壳交互，computer-use 驱动真实
窗口 + 截图取证）。任何场景只允许标注三层之一或多层；"未覆盖"必须显式写明原因。

## 1. Bubble 形态

| # | 场景 | 层 | 用例位置 | 状态 |
|---|------|----|----------|------|
| A1 | 双窗口配额 → 外环 r=21 / 内环 r=14 两道弧 + 各自轨道，无通道圆 | A | src/test/BubbleView.test.tsx | ✅ |
| A2 | 剩余 0 或未知 → 仅轨道、无值弧（round line-cap 圆点残影回归） | A | src/test/BubbleView.test.tsx | ✅ |
| A3 | 无供应商 → 独猫头鹰（.bubble__mark--solo），无轨道无弧 | A | src/test/BubbleView.test.tsx | ✅ |
| A4 | 比值 >1 钳制为满环（无越界 dash） | A | 代码审查（Math.min 钳制），实机复核 | ✅ |
| B1 | `?window=bubble` 预览：圆形按钮可聚焦、aria-label 含 TokenOwl + 状态 + 动作提示 | B | e2e/bubble.spec.ts | ✅ |
| B2 | `?window=bubble&empty=1`：独猫头鹰空态，无轨道 | B | e2e/bubble.spec.ts | ✅ |
| C1 | 真机形态：48px 正圆、双环分层有净空、图标居中、亚克力后无伪影 | C | computer-use 截图 | ✅ |
| C2 | 真机空态/离线态呼吸动画不引入滚动条或窗口溢出 | C | computer-use 截图 | ✅ |
| C13 | 真机窗口几何：bubble 窗口内区精确 56×56 逻辑 px。实机验证发现 Windows 最小追踪宽度（SM_CXMINTRACK）把普通窗口钳到 ~135 逻辑宽（透明可点击的死区），且 tao 在每次 show 时用缓存旗标重写 ex-style 抹掉修复——修复为 WS_EX_TOOLWINDOW（setup + 每次可见时由 webview 调 stabilize_bubble_window 重涂），窗口枚举实测内区 56×56、tool=True | C | measure/check-style 脚本 + Rust 代码 | ✅ |

## 2. Bubble 展开路径（panel 从 bubble 位置长出）

| # | 场景 | 层 | 用例位置 | 状态 |
|---|------|----|----------|------|
| B3 | 预览层：点击 bubble → 面板出现（clip reveal 播放完成）、URL 回 `/` | B | e2e/bubble.spec.ts | ✅ |
| C3 | 真机：点 bubble → 面板从 bubble 所在位置扩张（窗口先居中到 bubble 上方，实测 x=505 为居中+右缘钳制值），完成后 bubble 隐藏 | C | computer-use 窗口测量 ×3 | ✅ |
| C4 | 真机：折叠全程中帧已取证（burst-verify2/f066：面板全程不透明、圆形裁剪边界干净；f068：面板收走、任务栏按钮消失、无残留）；展开中帧受合成点击无法置前限制，在浏览器后播放（真实点击时面板由 set_focus 正常置顶，连拍帧 f03 可证） | C | 连拍截图 | ✅ |
| C5 | 真机：展开落定后任务栏 TokenOwl 按钮恢复（主窗口） | C | 连拍帧 f03 | ✅ |

## 3. Bubble 折叠路径（panel 收缩回 bubble）

| # | 场景 | 层 | 用例位置 | 状态 |
|---|------|----|----------|------|
| B4 | 预览层：点面板 Bubble 胶囊 → bubble 预览出现、URL `/?window=bubble` | B | e2e/bubble.spec.ts | ✅ |
| C6 | 真机：点胶囊 → 面板消失、bubble 在原位出现（双向重叠、无空帧——窗口可见性测量 ×4） | C | computer-use | ✅ |
| C7 | 真机：折叠后任务栏无 TokenOwl 按钮（skipTaskbar + WS_EX_TOOLWINDOW 生效）；面板可见时任务栏按钮存在（对照帧） | C | C7-taskbar-bubble-only.png vs 连拍 f01 | ✅ |

## 4. 拖拽 / 停靠 / 持久化

| # | 场景 | 层 | 用例位置 | 状态 |
|---|------|----|----------|------|
| A5 | 拖拽阈值 5px、非主键忽略、拖后点击不展开、移动后持久化 | A | src/test/BubbleView.test.tsx | ✅ |
| C8 | 真机：判别逻辑活体验证——无中间移动事件的按下-抬起被正确判为点击并展开面板（CUA 合成拖拽不含 pointermove，反向证明了阈值判别）；物理拖拽手势依赖 OS 原生 startDragging，CUA 无法合成中间移动事件，标为人工项 | C | computer-use | ⚠️ 部分 |
| C9 | 真机：拖到屏幕边缘 48px 内松手 → 吸附贴边（snap_to_edge 有界逻辑 + 用户历史停靠位 (873,427) 非默认值） | C | 人工（依赖物理拖拽） | ⚠️ 人工项 |
| C10 | 真机：折叠/展开循环 + 跨应用重启，bubble 均回到 (873,427)（位置持久化） | C | computer-use 测量 ×4 | ✅ |

## 5. 全局热键 / 托盘

| # | 场景 | 层 | 用例位置 | 状态 |
|---|------|----|----------|------|
| C11 | Alt+Shift+T：面板 ↔ bubble 双向切换。注册无错误（dev 日志无注册失败输出）；CUA/PowerShell 合成按键均不触发 RegisterHotKey 钩子，物理按键路径留人工复核。处理函数与点击路径共用 show_bubble/open_dashboard，已由 C3/C6 覆盖 | C | computer-use 尝试 + 日志 | ⚠️ 部分 |
| C12 | 托盘左键 → 打开面板；右键菜单 Quit 退出 | C | 人工（托盘点击依赖 shell） | ⚠️ 人工项 |

## 6. 几何纯函数

| # | 场景 | 层 | 用例位置 | 状态 |
|---|------|----|----------|------|
| A6 | cornerRadius：角点/中心/离窗点，覆盖全部窗口尺寸 | A | src/test/morph.test.ts | ✅ |
| A7 | clipCircle：格式正确、负半径钳制为 0 | A | src/test/morph.test.ts | ✅ |
| A8 | Rust centered_origin / clamp_anchor_to_rect：居中、显示器钳制、小屏回退、多显偏移 | A | src-tauri/src/app/mod.rs tests | ✅ |

## 7. 显式边界（本矩阵不覆盖，原因注明）

- **reduced-motion**：需要切换 OS 辅助设置，实机验证会改动用户系统状态；组件层已有
  useReducedMotion 分支与全局 CSS 覆盖，保持人工验证。
- **多显示器**：当前实机为单屏；Rust 侧几何已按 monitor 原点参数化并有单元测试。
- **macOS 壳行为**（skipTaskbar 的 Accessory 策略、vibrancy）：Windows 为唯一验证平台，
  macOS 标注 EXTERNAL_VALIDATION_REQUIRED。
- **通知系统横幅 / 更新器真实下载**：已有 TOKENOWL_TEST_BANNER / TOKENOWL_UPDATER_DRILL
  钻取钩子，独立于本矩阵执行。
- **长时稳定性**（24h 常驻内存、快照刷新节拍）：属于系统测试范畴，不在交互矩阵内。
