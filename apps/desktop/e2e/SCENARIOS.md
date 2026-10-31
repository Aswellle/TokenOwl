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

- **reduced-motion**：组件层与 e2e 预览层均已覆盖（`accessibility.spec.ts` 模拟
  `prefers-reduced-motion: reduce`，`bubble.spec.ts` 验证无 tween、立即落位）；
  原生壳的 OS 级开关仍需人工切换辅助设置后复核。
- **多显示器**：当前实机为单屏；Rust 侧几何已按 monitor 原点参数化并有单元测试。
- **macOS 壳行为**（skipTaskbar 的 Accessory 策略、vibrancy）：Windows 为唯一验证平台，
  macOS 标注 EXTERNAL_VALIDATION_REQUIRED。
- **通知系统横幅 / 更新器真实下载**：已有 TOKENOWL_TEST_BANNER / TOKENOWL_UPDATER_DRILL
  钻取钩子，独立于本矩阵执行。
- **长时稳定性**（24h 常驻内存、快照刷新节拍）：属于系统测试范畴，不在交互矩阵内。
- **浏览器 mock 与原生清单的条目差**：原生设置页实测 31 个 manifest profile、能力矩阵
  32 行；浏览器 mock 为 25 个 profile、14 行矩阵。mock 用于 UI 行为覆盖，不宣称与原生
  清单逐条等价；原生清单由 `provider_fixtures.rs` 与实机截图单独验证。

## 8. 2026-10 扩展测试批次（本批新增）

### 8.1 层与规模

| 层 | 工具 | 规模 | 结果 |
|----|------|------|------|
| A 单元/组件 | Vitest + jsdom | 19 文件 / 145 用例 | ✅ 全绿 |
| B e2e（webview 预览） | Playwright + Chromium | 8 文件 / 100 用例 | ✅ 全绿（11.7 min） |
| 联调契约 | Vitest 解析 TS/Rust 源码 | 4 用例 | ✅ 命令名、参数名、事件名双向一致 |
| C 实机（Windows Tauri） | computer-use + 区域采样 | 见 8.3 | ✅ 除注明限制项 |
| Rust | cargo test / clippy / fmt | 307 用例 | ✅ 全绿，clippy `-D warnings` 无告警 |

新增 e2e 文件：`dashboard.spec.ts`、`settings.spec.ts`、`history.spec.ts`、
`notifications.spec.ts`、`accessibility.spec.ts`、`viewport.spec.ts`；
`bubble.spec.ts` 从 7 例扩展到 14 例。`src/lib/mockScenarios.ts` 提供
`?scenario=` 降级状态夹具（offline/slow/stale/error/error-no-snapshots/partial/
three-windows/full/unknown/many/long/syncing/no-snapshots/refresh-fail/delete-fail/
create-fail/lan-fail/prefs-fail/history-fail/history-empty/notifications-fail/
notifications-empty/notifications-read/notifications-mark-fail/matrix-fail/usage-fail/
update-available/update-fail/install-fail/manifest-skipped）。

### 8.2 B 层新增场景

| 域 | 场景 | 用例位置 |
|----|------|----------|
| 概览 | 压力/账户/正常/重置四项汇总、需处理筛选、同步中不计入需处理、错误账户保留旧快照、偏旧警告、部分/失败指标标注、首刷失败提示 | dashboard.spec.ts |
| 供应商详情 | 打开/返回/Escape、用量窗口切换、无快照解释、用量查询失败保持 —、L4 能力 chip | dashboard.spec.ts |
| 行操作 | 全部刷新、刷新失败保留行、移除二次确认/取消/Escape/失败提示 | dashboard.spec.ts |
| 空/离线/加载 | 首跑空态、离线可重试、慢加载骨架、>5 账户 rail 计数、长标签不撑宽 | dashboard.spec.ts |
| 设置 | 三种协议分组、变体菜单/Escape 焦点、网关地址、自定义格式、四类校验、创建成功重置/失败保留、LAN 同意/取消/分类失败、社区包跳过、能力矩阵失败、更新当前/可用/失败/安装失败、API 密钥 password 掩码 | settings.spec.ts |
| 历史 | 30 天汇总、趋势图、热力图 182+ 格、日期/无记录可访问名、方向键、tooltip 分供应商与额度、空态、读取失败可重试 | history.spec.ts |
| 通知 | 列表/未读徽标/严重度、单条已读、全部已读、类型筛选、空态、读取失败、标记失败、偏好开关/失焦提交/回滚/读取失败 | notifications.spec.ts |
| 可访问性 | main 地标、唯一 h1、每视图按钮均有名称、表单控件均有标签、Dock Enter/Space、焦点环、LAN 对话框 Tab 陷阱与焦点返回、reduced-motion | accessibility.spec.ts |
| 视口 | 480/560/800/1440 无横向溢出且仅 content 滚动、长标签、320 视口 bubble、窄窗设置表单 | viewport.spec.ts |
| Bubble | 三真实窗口三环几何/图标 15px、unknown 仅轨道、满环 butt 端点、stale 保留环、offline 独猫头鹰、无窗口解释、键盘激活、拖拽阈值、空态 aria | bubble.spec.ts |

### 8.3 C 层实机数据（Windows，2026-10-31）

| 项 | 实测值 | 结论 |
|----|--------|------|
| 屏幕/DPI | 1920×1080，系统 DPI 144（150%） | 后续数值均为物理像素 |
| 主窗口 | 1162×1093（x=0/76，取决于居中与钳制） | 面板从 bubble 位置展开并右缘钳制 |
| Bubble 窗口 | 84×84 物理 = 56×56 逻辑 | 与 `skipTaskbar`/`WS_EX_TOOLWINDOW` 设计一致 |
| 任务栏 | bubble 显示时仅 "TokenOwl status" 窗口，主窗口隐藏 | 无独立 dock 图标（对照主窗口存在任务栏按钮） |
| 区域 morph | `GetWindowRgn` 采样：382→651→767→870→962→1043→1114→1175→1310→1365→1383→1407→1414→1419→1421→1422 px；结束后 `type=0` 清除 | OS 级椭圆裁剪真实生效，非 CSS 效果 |
| 空态状态 | 0 账户时标题栏 "尚未连接"（修复前为 "正常"） | 已修复 |
| 通知偏好 | 关闭启用 → 依赖字段禁用 + "已保存"；重新启用恢复 | 真实 SQLite 写入/读取 |
| 真实供应商 | `http://127.0.0.1:9910/v1` + 用户提供密钥 + `/models` 额度端点 | 账户创建成功、密钥入系统凭据库、请求到达端点；端点无额度字段 → `InvalidResponse`，UI 显示"首次刷新失败…"且不造数 |
| 凭据 | 创建后 `keyring` 跨进程读取 len=6；删除账户后 `NoEntry` | 修复 keyring 后端后真实持久化/清理 |
| API 密钥字段 | `type=password` 后 AX 树仅显示 `•••••••••` | 修复前 `type=text` 暴露明文 |

### 8.4 本批发现与修复

| # | 严重度 | 发现 | 修复 | 验证 |
|---|--------|------|------|------|
| F1 | 高 | 0 账户时整体状态被算作 healthy，标题栏显示"正常" | 新增 `unconfigured` 状态与"尚未连接"文案/样式 | health 单测、dashboard/bubble e2e、实机截图 |
| F2 | 中 | 空 bubble 的 aria 说"该账户未上报…"，暗示已有账户 | 区分 `noAccount`/`offline`/`noWindow` | BubbleView 单测、bubble e2e、实机 AX 树 |
| F3 | 严重 | `keyring = "3"` 无平台 feature，静默使用内存 mock，凭据永不持久化，所有真实账户刷新 `CredentialError` | 按平台启用 `windows-native`/`apple-native`/`linux-native`，新增跨实例凭据测试 | 实机真实端点账户、keyring 探针、Rust 测试 |
| F4 | 中 | 首次刷新失败且无快照时无任何解释 | `accountErrorEmpty` 通知（行与详情页） | AccountRow 单测、dashboard e2e、实机截图 |
| F5 | 低 | 首次失败同时显示错误通知与"点击刷新"空提示，重复 | error 且无快照时不再渲染空提示 | AccountRow 单测、dashboard e2e、实机截图 |
| F6 | 中 | API 密钥输入 `type=text`，CSS 仅视觉掩码，AX 树暴露明文 | 改为 `type=password` | SetupSubmit 单测、settings e2e、实机 AX 树 |
| F7 | 高 | 快速 expand/collapse 重叠时旧 region 任务可覆盖新形状，隐藏主窗口残留 1431×1432 region | `MorphEpoch` 所有权令牌，仅最新任务可写 region/执行收尾 | MorphEpoch 单测、实机采样结束后 `type=0` |
| F8 | 低（未修） | 删除账户后 `providers` 行残留（按账户 id 键控，无引用） | 待后续加 `delete_provider_if_unused` 或迁移清理 | DB 计数证据 |
| F9 | 测试夹具 | 浏览器 mock 25 profile/14 矩阵行 vs 原生 31/32 | 记录为 mock 边界；原生由 fixtures/实机覆盖 | 原生设置页截图 |
| F10 | 工具限制 | WebView2 AXPress 不派发 DOM click；bubble 小窗口 CUA 截图 `invalid_target` | 改用键盘焦点 + Enter；bubble 截图用 DPI-aware PowerShell | 实机记录 |
| F11 | 测试基建 | Vitest 5s 默认超时在并行负载下随机失败；e2e 固定 1877 落入 Windows 保留端口 | 20s 超时；动态空闲端口并固化到子进程 env | 全量套件 |
| F12 | 中 | 历史/通知/矩阵读取失败分别表现为永久骨架/假空态/"0 个供应商" | 三处显式失败态 + 重试；标记已读失败提示 | 单测、e2e、实机 |

### 8.5 第二批：DeepSeek 真实密钥全流程（2026-10-31 晚）

用户提供真实有效的 DeepSeek API Key，用内置 DeepSeek 供应商（`https://api.deepseek.com/v1`，
余额端点 `/user/balance`）完成全业务逻辑实机验证：

| 步骤 | 实测结果 |
|------|----------|
| 真实 API 探测 | `GET /user/balance` → 200 `{"is_available":false,"balance_infos":[{"currency":"CNY","total_balance":"-0.23",...}]}`；`GET /v1/models` → 200 |
| 创建账户（修复前构建） | 创建成功、密钥入 Windows 凭据管理器；首刷 `InvalidResponse`——通用解析器只认顶层 `remaining/balance`，读不了 `balance_infos` |
| 修复后首刷/手动刷新 | `余额` 指标显示 `剩余 CNY -0.23`，来源 `官方 API`，数据最新 |
| 账户健康 | `额度紧张`（负余额且无 limit 不能按 healthy 处理） |
| 压力统计提示 | `未上报窗口额度`（此前误报"等待首次数据"） |
| 供应商详情 | 余额卡片 + `L3 · 用量/余额` 能力 chip；本地用量如实为空 |
| Bubble | critical 配色 + 供应商标记，**无环**（余额无比率），aria `额度紧张。该账户未上报按窗口计的额度。展开 TokenOwl 面板` |
| 删除 | 两个账户 ref 的凭据均 `NoEntry`，accounts 0 |
| 同构建重加 | 创建 → 密钥入库 → 首刷 → 余额展示 → critical，一次构建内全流程通过 |

修复：F13 内置 DeepSeek 余额信封解析（`parse_balance_response` + 回退通用解析）、
F14 负余额判 critical、F15 内置供应商来源标 `官方 API`（`is_builtin_provider_id`）、
F16 压力提示 `未上报窗口额度`。新增 e2e `?scenario=balance-only`。
截图：`native-deepseek-provider-detail.png`、`native-deepseek-bubble.png`、
`native-deepseek-full-flow.png`（verify-shots/e2e-native/）。
测试后账户、通知与 providers 行全部清理；密钥未写入任何仓库文件。

### 8.6 第三批：窗口额度夹具实机 + OS 级 reduced-motion A/B（2026-10-31 深夜）

**窗口额度夹具**（`e2e/fixture-server.cjs`，127.0.0.1:8791，确定性测试数据，
时间戳相对当前时间生成）：`5h` 窗口 420 000/1 000 000 tokens（消耗 42%）、
`7d` 窗口 8 200 000/10 000 000 tokens（消耗 82%）。

| 步骤 | 结果 |
|------|------|
| 自定义供应商原生创建（LAN 同意流程） | 首刷命中夹具（带 Authorization 头，见 fixture-server.log） |
| 概览 | 双窗口指标：剩余/上限/已用、重置倒计时（`1 小时 59 分`、`1 天 23 小时`）、来源 `自定义端点`、压力 18%、健康 `需要关注` |
| Bubble aria | `需要关注。额度已用：5h 42%，7d 82%`（双环，短窗在外） |
| 圆环像素采样（DPI 感知 CopyFromScreen + 360° 角度扫描） | 外粉环 165°/360°（理论 151.2°+圆头端帽 13.6°=164.8°）；内青环 318°/360°（理论 295.2°+21.1°=316.3°）——画的是**消耗比**，非剩余比 |

同时发现并修复解析器缺口：lenient 解析器丢弃 `starts_at/ends_at`（`QuotaWindow`
类型已声明却无人填充），自定义供应商因此拿不到窗口标签。已补全 + 夹具测试
`openai_windowed_fixture_parses_window_spans`。

**OS 级 reduced-motion A/B**（切换真实的"在 Windows 中显示动画"
`SPI_GETCLIENTAREAANIMATION`，8ms 采样 GetWindowRgn/GetRgnBox）：

| 系统动画 | 折叠 | 展开 |
|----------|------|------|
| 关 | 0 个真实绘制帧（相邻两采样间完成） | 同左 |
| 开 | 20+ 帧连续变化的椭圆（region-animated.csv，t=26283–26811） | 同左 |

由此发现 F18（Rust 区域 morph 不尊重 OS 开关，已修：`client_area_animation_enabled`
把 steps 折为 1）与 F19（红灯标签"回到 Bubble"实际只 hide，已修为与 Bubble 药丸一致的
收缩交接）。F17（Bubble 不同步手动刷新/创建/删除）同批修复并实机双向验证。

工具性残留与偏差记录：PS UIA 对 WebView2 控件返回错误矩形（把红灯定位到了
左上角），坐标点击一律改用 CUA 元素点击或 GetWindowRect 中心；`GetWindowRgn`
对"从未设置区域"与"已清空区域"分别返回 0 与 3（ERROR），采样分析按 1/2 才算
真实绘制帧。

### 8.7 第四批：Bubble 空态/配置态全量 + 展开折叠路径动画（2026-11-01）

**Bubble 状态机全量**（vitest 22 用例 + e2e `bubble.spec.ts` 22 用例）：

| 状态 | 中心标 | 圆环 | 断言要点 |
|------|--------|------|----------|
| 空态（未配置） | 猫头鹰 glyph（`.bubble__mark--solo` 内 svg，无 `.provider-icon`） | 无（`data-ring-count="0"`） | aria `尚未连接。等待添加第一个供应商`；loading 期间整视图不挂载（防幻影空态） |
| 离线 | 猫头鹰 | 无 | aria `本地服务不可用` |
| 配置态双环（默认 demo：5h 16% + 周额度 10%） | 供应商图标 17px | 外 r21/内 r14、stroke 5、双轨道 | aria `额度已用：5h 16%，7d 10%`；`data-ring-consumed` outer=0.160/inner=0.103 |
| 三窗口 | 供应商图标 15px | r21/16/11 | 既有用例 |
| 未知窗口 | 供应商图标 | 双轨道无弧 | `data-ring-consumed="unknown"`，绝不画零弧 |
| 零消耗 / 全消耗 | — | 无弧 / 满弧 butt 端帽 | 零和未知都不画点 |
| 主账户选择 | 图标来自第一个非 error 账户 | 环只来自主账户（**不合并**） | 双账户同名 5h 窗口 → 取主账户 90%，error 账户跳过 |
| 部分窗口 | — | 单环 r21 | partial 状态可信；error 窗口与无窗口余额不参与 |

**展开/折叠路径动画与动效**：

- e2e 页内采样器（16ms 取 `clipPath`）：展开 = 圆形 clip 从气泡直径向外扫掠（≥4 个不同半径、扫过 ≥200px、终态半径覆盖视口对角）；折叠 = 面板 clip 向内收缩回气泡圆（终态 ≤48px）；reduced-motion 下**无 clip**（`clip-path: none`，改走瞬时淡入）。
- vitest morph 事件联调（捕获 `@tauri-apps/api/event` 的 listen）：`window:morph-to-bubble` → `data-morph-phase="settle"` + stabilize 重放 + `.quota-rings__geometry` 回到 scale1/opacity1；`window:morph-from-bubble` → `"exit"` + 环组压缩到 scale0.94/opacity0.5；同一 anchor 二次派发仍重放（settleToken），600ms 后回 idle。
- 联调契约：`bridge-contract` 新增 morph payload 测试——Rust `MorphAnchor{x,y,diameter}`（camelCase）与 TS `MorphAnchorPayload` 字段逐一相等，双向事件名存在性。
- Rust 纯函数补测：`region_radius_at`（端点精确、单调、ease-out 过半、越界钳制）、`morph_step_count`（380ms→23 步、5ms→1 步、OS 开关关闭→1 步）。

新增产品代码仅一处：`QuotaRings` svg 暴露 `data-morph-phase`（测试相位流转的可观察点）。

### 8.8 第五批：额度类型专项 + 官方信源校正（2026-11-01）

**官方信源核实结果**（抓取官方文档/搜索交叉验证）：

| 供应商 | 官方事实 | 本项目校正 |
|--------|----------|------------|
| DeepSeek | 仅 `/user/balance`；`balance_infos[]` 含 `total_balance`/`granted_balance`/`topped_up_balance`；无任何用量 API | 解析器拆出 余额/赠送余额/充值余额 三个指标（0 赠送也是真实数据，保留） |
| OpenRouter | `GET /api/v1/key` → `data.usage/limit/limit_remaining/usage_daily/usage_weekly/usage_monthly`（USD） | 修正双重拼接路径 `/api/v1/api/v1/key` → `/key`；`role` 区分 used/remaining；新增 今日/本周/本月已用 命名指标 |
| SiliconFlow | `GET /v1/user/info` → `data.totalBalance/chargeBalance/balance`（CNY） | 修正 `/v1/v1/user/balance` → `/user/info`；三个命名余额指标 |
| Kimi/Moonshot | `GET /v1/users/me/balance` → `data.available_balance/voucher_balance/cash_balance`（CNY，新 schema——旧字段 `vip_fill_balance` 已不存在） | usage→balance 语义修正 + 官方新字段映射 |
| OpenAI | `/v1/usage`、`/v1/quota` 不存在；真实 Usage/Cost 管理 API 需 admin scope + 必填 query 时间戳（普通路径模型无法表达） | 删除两个必 404 的臆造探针；能力等级 L4→L2（诚实降级），mock 矩阵同步 |
| Qwen/DashScope | 无公开 `/usage` 端点 | 删除臆造探针；L3→L1 |
| Anthropic | 官方限流响应头：`anthropic-ratelimit-{requests,tokens,input-tokens,output-tokens}-{limit,remaining,reset}`（reset 为 RFC3339）；限流页无 unified-5h/7d 头（社区观测存在，保留为可选） | 适配器扩展 4 个前缀 + 每前缀单位；未知跨度不发明窗口类型 |
| 其余（Groq/xAI/Mistral/Cohere/MiniMax/豆包等） | 无公开的 key 余额/用量 API | 维持 models-only，无臆造端点 |
| Token 速度/总调用次数/缓存 token | 无供应商提供 key 级公开 API（OpenAI 的 per-minute buckets 需 admin API + 查询参数，Anthropic 用量报告需 admin key）——诚实不支持，禁止编造 | 记录为能力边界，不预填假端点 |

**Rust 层**：`FieldMapping` 增加 `role`（used/remaining/limit，防"剩余 88.88"被读成"已用 88.88"）与 `name`；`FieldMappings.metrics[]` 支持单响应多命名指标；manifest 校验覆盖 metrics 值/名称/角色边界。

**前端类型自适应**：`QuotaMetric` 窗口芯片 = 类型+跨度（`滚动窗口 · 5h`、`固定窗口 · 7d`、`订阅周期`）；Token 数值 ≥1万 走 万/亿 缩写；百分比读作份额（`82.4%`）；无窗口指标**不显示**重置倒计时与比率条。详情页新增 `QuotaSections` 分组：窗口额度 / 余额与用量（仅两类并存时显示标题）。

**e2e `?scenario=kinds`**：单账户同时携带 6 种额度形态（5h 滚动 Token、7d 固定 Token、订阅积分、无窗口 CNY 余额、命名余额拆分、累计 Token 用量），断言分组标题、窗口芯片、缩写格式（50 万 / 1,823.46 万）、余额无 bar。
