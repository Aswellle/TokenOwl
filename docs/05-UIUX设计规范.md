# TokenOwl UI/UX 与视觉交互规范

## 1. 设计关键词

**Calm / Compact / Precise / Native / Fluid / Professional**

TokenOwl 不应像开发者监控后台，也不应像赛博朋克系统面板。

它应该是一个安静、漂亮、专业、长期放在桌面上的“桌面工具”。

---

## 2. 视觉原则

### 2.1 内容优先

用户第一眼应该看到：

```text
还剩多少
什么时候刷新
现在是否正常
```

而不是：

```text
HTTP 200
OAuth
Request ID
Adapter version
```

技术细节只在详情层展示。

### 2.2 颜色

颜色同时承担：

1. 指标类型；
2. 状态严重程度。

推荐 quota window 视觉编码：

```text
5h      Indigo / Violet
7d      Cyan / Blue
Monthly Amber / Gold
Bonus   Purple
Balance Green
Request Rose
```

状态不能只靠颜色：必须同时配合 icon、文字、形状。

例如：

```text
● Healthy
△ Attention
! Critical
— Stale
```

---

## 3. Floating Bubble

### 结构

```text
       ╭─────╮
      │  Owl  │
      │   72% │
       ╰─────╯
```

推荐：

- 外环：综合剩余额度 / 健康环；
- 中间：Logo；
- hover：轻微 scale 1.03；
- press：scale 0.96；
- sync：不旋转整颗球，只在环形 progress 上做轻微 motion；
- error：小角标，不震动。

### 拖动

- 鼠标按住拖动 120ms 后进入 drag；
- 点击和拖动必须区分；
- 松开后靠近屏幕边缘自动吸附；
- 支持多显示器；
- 记录 monitor id + normalized position；
- 显示器断开后自动迁移到主屏。

---

## 4. Expand / Collapse

目标：让用户感觉“圆球展开成一个面板”，而不是突然弹出窗口。

### 动画阶段

```text
0ms       Bubble
0–80ms    Container expands
80–160ms  Card surface appears
120–220ms Content fades/slides
220ms     Stable
```

### 禁止

- 爆炸式 spring；
- 大尺度 overshoot；
- 每个 Card 单独 stagger 200ms；
- 高光扫过；
- 玻璃拟态导致文字对比度下降。

---

## 5. Dashboard Layout

推荐：

```text
┌───────────────────────────────────────────────────────┐
│ TokenOwl        7 accounts     Updated 12s ago   ⚙   │
├───────────────────────────────────────────────────────┤
│                                                       │
│  Critical / Attention                                 │
│  ┌─────────────────────────────────────────────────┐  │
│  │ Anthropic                    5h · 12% remaining │  │
│  │ ████████░░░░░░░░░░░░         Reset 1h 04m      │  │
│  └─────────────────────────────────────────────────┘  │
│                                                       │
│  Providers                                            │
│  ...                                                  │
└───────────────────────────────────────────────────────┘
```

---

## 6. Provider Card

卡片必须包含：

- logo；
- provider/account label；
- status；
- top quota；
- reset countdown；
- last updated；
- expand affordance。

详细模式：

```text
Qwen — Coding Plan

5h quota                         71% left
███████████████████░░░░          Reset 02:31:20

7d quota                         38% left
██████████░░░░░░░░░░░░          Reset Oct 5

Bonus credits                    820 / 1000
████████████████░░░░

Source: Browser session
Updated: 2m ago
```

---

## 7. Responsive

Dashboard 最小宽度建议 520px。

在窄窗口中：

```text
正常：2-column summary
窄屏：1-column
超窄：Card 内部 metadata 换行
```

禁止：

- 卡片内容横向溢出；
- 固定 1440px dashboard；
- 过多横向滚动。

---

## 8. Typography

推荐：

```text
System font
Inter / SF Pro / Segoe UI fallback
```

层级：

```text
Display    20–24px
Heading    15–18px
Body       13–14px
Meta       11–12px
Numeric    tabular-nums
```

倒计时、余额等数据使用 tabular numerals。

---

## 9. Progress Bar

不要只显示一条普通 HTML progress。

推荐：

```text
label                      72% left
██████████████████░░░░░░░
reset in 2h 31m
```

quota bar 与 reset progress 应视觉上区分。

例如：

```text
Quota remaining   ██████████████░░
Time remaining    ────────────────○
```

---

## 10. Loading

首次加载：

- skeleton；
- 不使用全屏 spinner。

刷新：

- provider card 顶部显示小型 sync indicator；
- 旧数据继续可用。

---

## 11. Empty State

没有 Provider：

```text
Your AI limits, all in one place.

Connect your first provider.

[ Add Provider ]
```

没有可查询 quota：

```text
Connected

This provider does not expose account quota through its
supported API yet.

[ Configure custom usage endpoint ]
```

不要把这种情况显示为“0”。

---

## 12. Settings UX

分组：

```text
General
Providers
Notifications
Appearance
Privacy
Advanced
About
```

Provider Settings：

```text
Connected        ●

Authentication
  API Key
  OAuth
  Browser Session

Data Sources
  Official API      ✓
  Browser           ○
  Local CLI         ○

Refresh
  Every 5 minutes

[ Test Connection ]
[ Disable ]
```

---

## 13. Accessiblity

最低要求：

- 键盘可操作；
- focus ring；
- Reduced Motion；
- 颜色不能是唯一状态编码；
- WCAG AA 对主要文字；
- Tooltip 不承载唯一信息；
- screen reader 能描述 quota / reset 状态。

---

## 14. Native Feeling

Windows：

- 与系统 DPI 一致；
- 支持系统主题；
- taskbar/tray；
- 原生通知。

macOS：

- menu bar/tray；
- Keychain；
- system notification；
- notch / multiple display 场景。

---
