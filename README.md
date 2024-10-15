Solana Sniping Bot
=======================

## Overview

The bot is a scalable customizable event-driven trading platform, designed to work with a Raydium Solana DEX pair, but
can be easily extended to work with any markets on DEXes or CEXes.
Blazing 💥 fast written in Rust 🦀
The bot uses the Solana Farm Client and Solana Farm SDK.
The bot uses Geyser based gRPC interface for Solana and Solana RPC node connection, executes in parallel on multiple RPC
and Bloxroute Trading API.

## Usage

- `ssh` to the remote machine (user anicho or any).
- `cd /opt/bot/foxfire-sniper`
- Verify configuration `vim ./config.toml`
- Run the bot with `./target/release/solana-bot`
- To stop the bot, press `Ctrl+C` or `kill -9 <pid>`

## Requirements

Highly depend on the desired accuracy, volume, and speed of the trading bot, but the following are the minimum
requirements:

- Cloud machine with fast internet connection and low latency to Solana RPC.
- Solana RPC node connection (http/https, websocket is not used).
- Yellowstone Dragon's Mouth - a Geyser based gRPC interface for Solana (note incoming feed is 36Mbps minimum - can't be lower than this).
- Rust toolchain (https://rustup.rs/).

## Installation
- Install [ postgresql and timescaledb ](https://docs.timescale.com/self-hosted/latest/install/installation-linux/)on top of postgresql
- Install rust `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Install redis `sudo apt-get install redis-server`
- Install diesel cli `curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash`
`cargo install diesel_cli --no-default-features --features postgres`
- Run diesel migrations to populate the database schema `diesel setup`, then can be repeated with `diesel migration redo`
- Clone the repo `git clone git@github.com:crypt0grapher/foxfire-sniper.git`
- `cd foxfire-sniper`
- `cargo build --release`

## Configuration

Copy [`config.example.toml`](./config.example.toml) to `config.toml` and fill in the required values.
Run the bot with `cargo run --release` and the bot will start trading on the specified market.

## Engine

System's [Engine<Signal, Action>](./src/engine.rs) is a high-performance, multithreaded orchestrator
of [Collectors, Strategies, and Executors](./src/types/engine.rs) with synchronized operation through a robust
messaging.

All three are traits. This makes implementations pluggable, e.g. we use same collector for a number of various
strategies, same strategy for a number of various executors, etc.

1. `Collector<S>: Send + Sync` : streams **Events**.
2. `Strategy<S, A>: Send + Sync` : consumes **Events** and generates **Actions**.
3. `Executor<A>: Send + Sync` : executes **Actions** and generates **Events**.

Such a design allows using trading on any network and using any strategy, including centralizes exchanges, number of
strategies, and MEV -
literally anything. Streams and queues are abstracted away and can be replaced with any other implementation (e.g.
Kafka, RabbitMQ, etc).

## Events

Currently the bot uses the following events:

- Price and reserve change (not necessary for the volume bot, but core of everything going forward).
- Swap execution receipt (used for the strategy to retry and confirm swap).
- Tick - time moment (used for the volume strategy to decide when to trade).
- Buy/Sell (unconditional buy/sell signals, sent by strategy to its agents only, can be used for retries and add
  buys/sells in a loopback).

## Actions

Swap on the target Raydium pair by a given wallet is the only action used in the current implementation.


### Current implementations

1. `Collector` streams `SolanaEvent` which is `PoolPrice`, `SwapExecuted`, or `Tick`,
2. `Strategy` consumes `SolanaEvent` and generates `Vec<SolanaSwapAction>`.
3. `Executor`: sends a list of `SolanaSwapAction` to the Solana network via all connected RPC nodes and Bloxroute
   Trading API and generates `SolanaEvent`.
