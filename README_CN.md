# HashQuant

> 用 Rust 构建的高性能量化交易框架，支持多交易所、策略回测与实盘部署。

---

## 目录

- [项目简介](#项目简介)
- [功能特性](#功能特性)
- [架构概览](#架构概览)
- [模块说明](#模块说明)
- [快速开始](#快速开始)
- [配置说明](#配置说明)
- [使用指南](#使用指南)
- [风控参数](#风控参数)
- [自定义策略](#自定义策略)
- [项目结构](#项目结构)

---

## 项目简介

HashQuant 是一个用 Rust 编写的模块化量化交易系统，覆盖从数据接入、策略计算、风险管控到实盘执行的完整链路。

**设计理念：**
- **模块解耦**：各 crate 通过 trait 通信，策略层完全不知道底层是哪家交易所
- **零成本抽象**：Rust 的类型系统和所有权机制保障内存安全与高性能
- **可测试优先**：所有模块均可对接 `MockExchange`，无需网络即可运行完整测试

---

## 功能特性

| 功能 | 说明 |
|------|------|
| 多交易所支持 | Binance、OKX、Coinbase、Polymarket |
| 三套运行环境 | Live（生产）/ Testnet（测试网）/ Paper（本地模拟） |
| 多种数据源 | 实时行情轮询、K 线定时拉取、CSV 文件、SQLite 数据库 |
| 内置技术指标 | SMA、EMA、RSI、MACD、布林带 |
| 内置策略 | MA 均线交叉、RSI 均值回归 |
| 风控管理 | 仓位限制、每日止损、净值保护、回撤监控 |
| 回测系统 | 历史 CSV 数据回放，输出详细绩效报告 |
| 完整测试覆盖 | 单元测试 + 集成测试，全部无网络依赖 |

---

## 架构概览

```
数据层                策略层              执行层
┌──────────┐        ┌──────────┐        ┌──────────────┐
│ datafeed │──────▶ │ strategy │──────▶ │     risk     │
│          │        │          │        │  (风控检查)   │
│ CandleFeed         │ MaCross  │        └──────┬───────┘
│ CsvFeed  │        │ RsiStrat │               │
│ ExchFeed │        │ 自定义... │               ▼
└──────────┘        └──────────┘        ┌──────────────┐
                                        │   exchange   │
                                        │  Binance/OKX │
                                        │  Coinbase/   │
                                        │  Polymarket  │
                                        │  MockExchange│
                                        └──────────────┘
                         ↑
                    ┌──────────┐
                    │   core   │
                    │ (类型定义 │
                    │  trait)  │
                    └──────────┘
```

---

## 模块说明

### `crates/core`
核心类型与接口定义。所有其他 crate 依赖此模块，但此模块本身零外部依赖。

- `Exchange` trait：统一的交易所接口
- 共享数据类型：`Ticker`、`Candle`、`Order`、`Trade`、`Balance` 等
- `CoreError`：统一错误类型

### `crates/exchange`
四家交易所的适配器实现，以及本地模拟引擎。

| 客户端 | 生产网 | 测试网/模拟盘 |
|--------|--------|--------------|
| `BinanceClient` | `::new()` | `::testnet()` → testnet.binance.vision |
| `OkxClient` | `::new()` | `::testnet()` → 自动加 `x-simulated-trading: 1` |
| `CoinbaseClient` | `::new()` | `::testnet()` → sandbox 域名 |
| `PolymarketClient` | `::new()` | 无官方测试网，建议用 Mock |
| `MockExchange` | — | 内存撮合引擎，支持限价/市价，自动触发挂单 |

### `crates/datafeed`
数据接入层，提供统一的 `DataFeed` trait。

| 数据源 | 用途 |
|--------|------|
| `CandleFeed` | 定时拉取 K 线，含历史预热，用于实盘 |
| `ExchangeFeed` | Tick 级别实时轮询 |
| `CsvFeed` | 读取 CSV 历史文件，用于回测 |
| `DatabaseFeed` | 读取 SQLite 历史数据 |

### `crates/strategy`
技术指标 + 策略引擎。策略只依赖 `core` trait，不直接调用任何交易所。

**技术指标：**
- `sma(prices, period)` / `ema(prices, period)` — 移动平均
- `rsi(prices, period)` — 相对强弱指数（Wilder 平滑）
- `macd(prices, fast, slow, signal)` — MACD 三线
- `bollinger_bands(prices, period, multiplier)` — 布林带（含 %B 和带宽）

**内置策略：**
- `MaCrossStrategy` — EMA 快慢线金叉死叉
- `RsiStrategy` — RSI 超买超卖均值回归

### `crates/risk`
风控模块，在信号转订单之间插入多重检查。

- `RiskLimits` — 风控参数配置
- `RiskChecker` — 订单前置校验
- `PositionTracker` — 仓位跟踪（均价、已实现/未实现盈亏）
- `DrawdownMonitor` — 回撤、夏普比率、净值曲线

### `crates/backtester`
历史回测系统。

- `Simulator` — 用 `CsvFeed` + `MockExchange` 回放历史数据
- `Metrics` — 计算胜率、盈亏比、最大回撤、夏普、卡尔玛等
- `reporter::print_report()` — 打印格式化回测报告

### `bin/`（独立可执行项目）

| 命令 | 功能 |
|------|------|
| `cargo run --bin fetch_data` | 从 Binance 下载历史 K 线，保存为 CSV |
| `cargo run --bin backtest` | 读 CSV，回测策略，打印绩效报告 |
| `cargo run --bin testnet` | 接测试网，含风控，运行实盘策略 |

---

## 快速开始

### 环境要求

- Rust 1.75+（推荐通过 [rustup](https://rustup.rs) 安装）
- 中国大陆用户需要代理访问 Binance API

### 安装

```bash
git clone https://github.com/yourname/HashQuant.git
cd HashQuant
cargo build
```

### 运行测试

```bash
# 运行所有测试（无网络依赖）
cargo test

# 只测试某个模块
cargo test -p hq-exchange
cargo test -p hq-strategy
cargo test -p hq-datafeed
cargo test -p hq-backtester
```

---

## 配置说明

复制配置模板：

```bash
cp .env.example .env
```

编辑 `.env` 填入你的 Key：

```bash
# Binance 测试网（在 https://testnet.binance.vision 用 GitHub 账号申请）
BINANCE_TESTNET_KEY=你的测试网Key
BINANCE_TESTNET_SECRET=你的测试网Secret

# OKX（生产/模拟盘共用同一组Key）
OKX_API_KEY=
OKX_SECRET=
OKX_PASSPHRASE=

# Coinbase
COINBASE_API_KEY=
COINBASE_SECRET=

# Polymarket
POLY_API_KEY=
POLY_SECRET=

# 代理（中国大陆必须配置）
HTTPS_PROXY=http://127.0.0.1:7890

# 日志级别
RUST_LOG=info
```

---

## 使用指南

### 第一步：下载历史数据

```bash
cd bin

# 默认下载 ETHUSDT 1小时线，3000 根
cargo run --bin fetch_data
```

如需修改品种或数量，编辑 `bin/src/fetch_data.rs` 顶部常量：

```rust
const SYMBOL:   &str = "BTCUSDT";   // 交易对
const INTERVAL: &str = "4h";        // K 线周期
const PAGES:    u32  = 5;           // 下载页数（每页 1000 根）
```

### 第二步：运行回测

```bash
cargo run --bin backtest
```

**回测报告示例：**

```
══════════════════════════════════════════════════════
  📊 回测报告
  策略: MA交叉   交易对: ETHUSDT
  时间: 2024-01-01 → 2024-12-31  跨度: 365.0 天
══════════════════════════════════════════════════════
  💰 资金
  初始资金:              10000.00 USDT
  最终资金:              12341.56 USDT
  总收益率:               +23.42%
  年化收益:               +23.42%
──────────────────────────────────────────────────────
  📈 交易统计
  总交易次数:                   24
  胜率:                      62.5%
  盈亏比:                     2.31
──────────────────────────────────────────────────────
  ⚠️  风险指标
  最大回撤:                   8.20%
  夏普比率:                    1.45
══════════════════════════════════════════════════════
```

### 第三步：测试网实盘

```bash
cargo run --bin testnet
```

程序启动后会：
1. 拉取历史 K 线预热指标（默认 50 根）
2. 等待下一根 K 线收盘
3. 调用策略计算信号
4. 经过风控检查后提交订单
5. 持续运行直到按 `Ctrl+C`

---

## 风控参数

在 `bin/src/testnet.rs` 中修改 `RiskLimits`：

```rust
let limits = RiskLimits {
    max_positions:      3,      // 同时最多持有几个标的
    max_position_pct:   50.0,   // 单标的占净值上限（%）
    max_order_value:    5000.0, // 单笔订单金额上限（USDT）
    min_order_value:    10.0,   // 单笔订单金额下限（USDT）
    max_loss_pct:       3.0,    // 单笔最大亏损（%）
    daily_max_loss_pct: 8.0,    // 每日最大亏损，超过则停止（%）
    min_equity:         100.0,  // 净值低于此值停止交易（USDT）
    max_leverage:       1.0,    // 最大杠杆（现货 = 1.0）
    allow_short:        false,  // 是否允许做空
};
```

---

## 自定义策略

实现 `Strategy` trait 即可接入整个系统：

```rust
use async_trait::async_trait;
use hq_strategy::strategy::{Strategy, Signal};
use hq_core::types::{Candle, Ticker};

pub struct MyStrategy {
    symbol: String,
    symbols_vec: Vec<String>,
    // 你的状态...
}

#[async_trait]
impl Strategy for MyStrategy {
    fn name(&self) -> &str { "MyStrategy" }
    fn symbols(&self) -> &[String] { &self.symbols_vec }

    async fn on_candle(&mut self, candle: &Candle) -> hq_strategy::Result<Vec<Signal>> {
        // 你的信号逻辑...
        // 返回空 vec 表示无信号
        Ok(vec![])
    }

    // 可选：初始化时预热历史数据
    async fn init(&mut self, history: &[Candle]) -> hq_strategy::Result<()> {
        Ok(())
    }
}
```

然后在 `bin/src/testnet.rs` 中替换策略：

```rust
let mut strategy = MyStrategy::new(SYMBOL);
```

---

## 项目结构

```
HashQuant/
├── Cargo.toml                   # workspace 根
├── .env.example                 # 配置模板
├── .gitignore
│
├── crates/                      # 库 crate（workspace 成员）
│   ├── core/                    # 类型定义 & Exchange trait
│   ├── exchange/                # 交易所适配器 + MockExchange
│   │   ├── src/binance/
│   │   ├── src/okx/
│   │   ├── src/coinbase/
│   │   ├── src/polymarket/
│   │   ├── src/mock/            # 内存撮合引擎
│   │   ├── src/config/          # AppConfig (.env 加载)
│   │   └── src/testnet/         # 各交易所 URL 配置
│   ├── datafeed/                # 数据接入层
│   │   ├── src/sources/         # CandleFeed / CsvFeed / ExchangeFeed
│   │   └── src/storage/         # SQLite 落库
│   ├── strategy/                # 策略引擎
│   │   ├── src/indicators/      # SMA / EMA / RSI / MACD / 布林带
│   │   └── src/strategies/      # MA交叉 / RSI均值回归
│   ├── risk/                    # 风控模块
│   │   ├── src/limits.rs        # 风控规则 & 检查器
│   │   ├── src/position.rs      # 仓位跟踪
│   │   ├── src/monitor.rs       # 回撤监控
│   │   └── src/manager.rs       # 风控管理器（主入口）
│   └── backtester/              # 回测系统
│       ├── src/simulator.rs     # 回测引擎
│       ├── src/metrics.rs       # 绩效指标
│       └── src/reporter.rs      # 报告打印
│
└── bin/                         # 可执行入口（独立 workspace）
    ├── Cargo.toml
    └── src/
        ├── fetch_data.rs        # 历史数据下载
        ├── backtest.rs          # 回测入口
        └── testnet.rs           # 测试网实盘（含风控）
```

---

## 注意事项

- `.env` 文件包含私钥，**永远不要提交到 Git**（已加入 `.gitignore`）
- 测试网 Key 与生产网 Key **完全独立**，在 [testnet.binance.vision](https://testnet.binance.vision) 单独申请
- OKX 模拟盘使用与生产网相同的 Key，切换方式为代码中调用 `OkxClient::testnet()`
- 回测结果仅供参考，历史收益不代表未来表现
- 首次运行前请务必先在测试网验证策略逻辑

---

## License

MIT
