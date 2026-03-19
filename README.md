# HashQuant

> A high-performance quantitative trading framework built in Rust, supporting multiple exchanges, strategy backtesting, and live deployment.

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Architecture](#architecture)
- [Modules](#modules)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Usage Guide](#usage-guide)
- [Risk Management](#risk-management)
- [Custom Strategies](#custom-strategies)
- [Project Structure](#project-structure)

---

## Overview

HashQuant is a modular quantitative trading system written in Rust, covering the complete pipeline from data ingestion, strategy computation, risk control, to live order execution.

**Design Principles:**
- **Decoupled modules** — Crates communicate via traits; the strategy layer has zero knowledge of which exchange is running underneath
- **Zero-cost abstractions** — Rust's type system and ownership model guarantee memory safety and high performance
- **Test-first** — Every module can be wired to `MockExchange`; the entire test suite runs without network access

---

## Features

| Feature | Description |
|---------|-------------|
| Multi-exchange | Binance, OKX, Coinbase, Polymarket |
| Three environments | Live / Testnet / Paper (local mock) |
| Multiple data sources | Live polling, scheduled K-line fetch, CSV files, SQLite |
| Built-in indicators | SMA, EMA, RSI, MACD, Bollinger Bands |
| Built-in strategies | MA Crossover, RSI Mean Reversion |
| Risk management | Position limits, daily stop-loss, equity floor, drawdown monitor |
| Backtesting | CSV replay with detailed performance report |
| Full test coverage | Unit + integration tests, all network-free |

---

## Architecture

```
Data Layer            Strategy Layer        Execution Layer
┌──────────┐         ┌──────────┐          ┌──────────────┐
│ datafeed │────────▶│ strategy │─────────▶│     risk     │
│          │         │          │          │ (risk checks) │
│ CandleFeed         │ MaCross  │          └──────┬───────┘
│ CsvFeed  │         │ RsiStrat │                 │
│ ExchFeed │         │ custom...│                 ▼
└──────────┘         └──────────┘          ┌──────────────┐
                                           │   exchange   │
                                           │  Binance/OKX │
                                           │  Coinbase/   │
                                           │  Polymarket  │
                                           │  MockExchange│
                                           └──────────────┘
                          ↑
                     ┌──────────┐
                     │   core   │
                     │  (types  │
                     │  traits) │
                     └──────────┘
```

---

## Modules

### `crates/core`
Core types and interface definitions. All other crates depend on this one, which itself has zero external dependencies.

- `Exchange` trait — unified exchange interface
- Shared types: `Ticker`, `Candle`, `Order`, `Trade`, `Balance`, etc.
- `CoreError` — unified error type

### `crates/exchange`
Adapter implementations for four exchanges plus a local simulation engine.

| Client | Live | Testnet / Simulated |
|--------|------|---------------------|
| `BinanceClient` | `::new()` | `::testnet()` → testnet.binance.vision |
| `OkxClient` | `::new()` | `::testnet()` → auto adds `x-simulated-trading: 1` |
| `CoinbaseClient` | `::new()` | `::testnet()` → sandbox domain |
| `PolymarketClient` | `::new()` | No official testnet — use Mock |
| `MockExchange` | — | In-memory matching engine; supports limit/market orders and automatic fill on price trigger |

### `crates/datafeed`
Data ingestion layer providing a unified `DataFeed` trait.

| Source | Use Case |
|--------|----------|
| `CandleFeed` | Scheduled K-line polling with warmup history, for live trading |
| `ExchangeFeed` | Tick-level real-time polling |
| `CsvFeed` | Read historical CSV files for backtesting |
| `DatabaseFeed` | Read historical data from SQLite |

### `crates/strategy`
Technical indicators and strategy engine. Strategies depend only on `core` traits — they never call an exchange directly.

**Indicators:**
- `sma(prices, period)` / `ema(prices, period)` — moving averages
- `rsi(prices, period)` — Relative Strength Index (Wilder smoothing)
- `macd(prices, fast, slow, signal)` — MACD three-line
- `bollinger_bands(prices, period, multiplier)` — Bollinger Bands with %B and bandwidth

**Built-in Strategies:**
- `MaCrossStrategy` — EMA golden cross / death cross
- `RsiStrategy` — RSI overbought/oversold mean reversion

### `crates/risk`
Risk management module, injected between signal generation and order submission.

- `RiskLimits` — configurable risk parameters
- `RiskChecker` — pre-order validation
- `PositionTracker` — position tracking (average price, realized/unrealized P&L)
- `DrawdownMonitor` — drawdown, Sharpe ratio, equity curve

### `crates/backtester`
Historical backtesting system.

- `Simulator` — replays `CsvFeed` through a `MockExchange`
- `Metrics` — win rate, profit factor, max drawdown, Sharpe, Calmar, etc.
- `reporter::print_report()` — prints a formatted performance report

### `bin/` (standalone executable)

| Command | Function |
|---------|----------|
| `cargo run --bin fetch_data` | Download historical K-lines from Binance, save as CSV |
| `cargo run --bin backtest` | Read CSV, backtest strategy, print report |
| `cargo run --bin testnet` | Connect to testnet with risk management, run live strategy |

---

## Quick Start

### Requirements

- Rust 1.75+ (install via [rustup](https://rustup.rs))
- Users in mainland China need a proxy to access Binance API

### Build

```bash
git clone https://github.com/yourname/HashQuant.git
cd HashQuant
cargo build
```

### Run Tests

```bash
# Run all tests (no network required)
cargo test

# Test a specific module
cargo test -p hq-exchange
cargo test -p hq-strategy
cargo test -p hq-datafeed
cargo test -p hq-backtester
```

---

## Configuration

Copy the template:

```bash
cp .env.example .env
```

Edit `.env` and fill in your keys:

```bash
# Binance Testnet (apply at https://testnet.binance.vision with GitHub login)
BINANCE_TESTNET_KEY=your_testnet_key
BINANCE_TESTNET_SECRET=your_testnet_secret

# OKX (live and simulated trading share the same key)
OKX_API_KEY=
OKX_SECRET=
OKX_PASSPHRASE=

# Coinbase
COINBASE_API_KEY=
COINBASE_SECRET=

# Polymarket
POLY_API_KEY=
POLY_SECRET=

# Proxy (required in mainland China)
HTTPS_PROXY=http://127.0.0.1:7890

# Log level
RUST_LOG=info
```

---

## Usage Guide

### Step 1 — Download Historical Data

```bash
cd bin

# Default: ETHUSDT 1h candles, 3000 bars
cargo run --bin fetch_data
```

To change symbol or range, edit the constants at the top of `bin/src/fetch_data.rs`:

```rust
const SYMBOL:   &str = "BTCUSDT";   // trading pair
const INTERVAL: &str = "4h";        // candle interval
const PAGES:    u32  = 5;           // pages to fetch (1000 bars each)
```

### Step 2 — Run a Backtest

```bash
cargo run --bin backtest
```

**Sample report output:**

```
══════════════════════════════════════════════════════
  📊 Backtest Report
  Strategy: MA-Cross   Symbol: ETHUSDT
  Period: 2024-01-01 → 2024-12-31  Duration: 365.0 days
══════════════════════════════════════════════════════
  💰 Capital
  Initial Equity:        10000.00 USDT
  Final Equity:          12341.56 USDT
  Total Return:            +23.42%
  Annualized Return:       +23.42%
──────────────────────────────────────────────────────
  📈 Trade Statistics
  Total Trades:                 24
  Win Rate:                  62.5%
  Profit Factor:              2.31
──────────────────────────────────────────────────────
  ⚠️  Risk Metrics
  Max Drawdown:               8.20%
  Sharpe Ratio:               1.45
══════════════════════════════════════════════════════
```

### Step 3 — Testnet Live Trading

```bash
cargo run --bin testnet
```

On startup the program will:
1. Pull historical K-lines to warm up indicators (default 50 bars)
2. Wait for the next candle to close
3. Run the strategy and compute signals
4. Pass signals through risk checks
5. Submit approved orders
6. Continue until `Ctrl+C`

---

## Risk Management

Edit `RiskLimits` in `bin/src/testnet.rs`:

```rust
let limits = RiskLimits {
    max_positions:      3,      // max open symbols simultaneously
    max_position_pct:   50.0,   // max single-position size as % of equity
    max_order_value:    5000.0, // max single order value (USDT)
    min_order_value:    10.0,   // min single order value (USDT)
    max_loss_pct:       3.0,    // max loss per trade (%)
    daily_max_loss_pct: 8.0,    // halt trading if daily loss exceeds this (%)
    min_equity:         100.0,  // halt trading if equity drops below this (USDT)
    max_leverage:       1.0,    // max leverage (1.0 = spot, no leverage)
    allow_short:        false,  // allow short selling
};
```

**Risk check chain — every signal goes through:**

```
Signal generated
      │
      ▼
Equity ≥ min_equity?         → EquityTooLow if not
      │
      ▼
Open positions < max_positions?  → MaxPositionsExceeded if not
      │
      ▼
Position size ≤ max_position_pct?  → PositionSizeExceeded if not
      │
      ▼
Daily loss < daily_max_loss_pct?  → DailyLossExceeded if not
      │
      ▼
✅ Approved → place_order()
```

---

## Custom Strategies

Implement the `Strategy` trait to plug into the full system:

```rust
use async_trait::async_trait;
use hq_strategy::strategy::{Strategy, Signal};
use hq_core::types::Candle;

pub struct MyStrategy {
    symbol:      String,
    symbols_vec: Vec<String>,
    closes:      Vec<f64>,
    // your state fields...
}

#[async_trait]
impl Strategy for MyStrategy {
    fn name(&self) -> &str { "MyStrategy" }
    fn symbols(&self) -> &[String] { &self.symbols_vec }

    async fn on_candle(&mut self, candle: &Candle) -> hq_strategy::Result<Vec<Signal>> {
        self.closes.push(candle.close);

        // Your signal logic here.
        // Return empty vec for no signal.
        // Return Signal::buy / Signal::sell to generate orders.

        Ok(vec![])
    }

    // Optional: pre-load history for indicator warmup
    async fn init(&mut self, history: &[Candle]) -> hq_strategy::Result<()> {
        self.closes = history.iter().map(|c| c.close).collect();
        Ok(())
    }
}
```

Then swap it in `bin/src/testnet.rs`:

```rust
let mut strategy = MyStrategy::new(SYMBOL);
```

The same strategy object works identically in backtesting:

```rust
// bin/src/backtest.rs
let mut strategy = MyStrategy::new(SYMBOL);
let result = sim.run(&mut feed, &mut strategy, SYMBOL).await;
```

---

## Project Structure

```
HashQuant/
├── Cargo.toml                    # workspace root
├── .env.example                  # configuration template
├── .gitignore
│
├── crates/                       # library crates (workspace members)
│   ├── core/                     # types & Exchange trait
│   ├── exchange/                 # exchange adapters + MockExchange
│   │   ├── src/binance/
│   │   ├── src/okx/
│   │   ├── src/coinbase/
│   │   ├── src/polymarket/
│   │   ├── src/mock/             # in-memory matching engine
│   │   ├── src/config/           # AppConfig (.env loader)
│   │   └── src/testnet/          # per-exchange URL configs
│   ├── datafeed/                 # data ingestion layer
│   │   ├── src/sources/          # CandleFeed / CsvFeed / ExchangeFeed
│   │   └── src/storage/          # SQLite persistence
│   ├── strategy/                 # strategy engine
│   │   ├── src/indicators/       # SMA / EMA / RSI / MACD / Bollinger
│   │   └── src/strategies/       # MA-Cross / RSI-MeanReversion
│   ├── risk/                     # risk management
│   │   ├── src/limits.rs         # risk rules & checker
│   │   ├── src/position.rs       # position tracker
│   │   ├── src/monitor.rs        # drawdown monitor
│   │   └── src/manager.rs        # RiskManager (main entry point)
│   └── backtester/               # backtesting system
│       ├── src/simulator.rs      # backtest engine
│       ├── src/metrics.rs        # performance metrics
│       └── src/reporter.rs       # report printer
│
└── bin/                          # executables (standalone workspace)
    ├── Cargo.toml
    └── src/
        ├── fetch_data.rs         # historical data downloader
        ├── backtest.rs           # backtest entry point
        └── testnet.rs            # testnet live trading with risk management
```

---

## Important Notes

- The `.env` file contains private keys — **never commit it to Git** (already in `.gitignore`)
- Binance Testnet keys are **completely separate** from live keys — apply at [testnet.binance.vision](https://testnet.binance.vision)
- OKX simulated trading uses the same key as live — switch by calling `OkxClient::testnet()`
- Backtest results are for reference only; past performance does not guarantee future results
- Always validate strategy logic on testnet before deploying to live

---

## License

MIT
