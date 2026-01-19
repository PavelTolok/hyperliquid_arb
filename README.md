
#  Hyperliquid Arbitrage Bot

A **Rust-based cryptocurrency arbitrage bot** that monitors price differences between **Bybit (USDT Perpetuals)** and **Hyperliquid** and sends **real-time arbitrage alerts** via **Twitter (X)**. Stay ahead of the market with automated price tracking and spread detection.

![License](https://img.shields.io/badge/license-MIT-green)
![Python](https://img.shields.io/badge/python-3.8%2B-blue)
![Status](https://img.shields.io/badge/status-active-success)



##  Features
- 🔍 **Real-Time Price Monitoring** – Tracks price differences between Bybit and Hyperliquid
- 🚨 **Arbitrage Alerts** – Sends Twitter alerts when a price spread ≥ 5%
- ⚡ **Fast & Lightweight** – Optimized Python script for quick execution
- 🔧 **Easy Setup** – Simple `.env` configuration for API keys and parameters


##  Arbitrage Logic

The bot scans both exchanges in real time and calculates arbitrage profit potential.

###  Workflow:
1. **Fetch Prices**  
   Retrieve bid/ask data from Bybit and Hyperliquid.
2. **Calculate Spread**  
   ```text
   Spread % = |(Price_High - Price_Low) / Price_Low * 100|

3. **Check Opportunity**

   * If **Price_Hyperliquid > Price_Bybit → Buy on Bybit, Sell on Hyperliquid**
   * If **Price_Bybit > Price_Hyperliquid → Buy on Hyperliquid, Sell on Bybit**
4. **Trigger Alert**
   When spread ≥ **5%**, send Twitter notification with:

   * Trading pair
   * Price difference %
   * Potential arbitrage direction

Includes latency-aware updates and basic fee/slippage filtering.


## 💱 Supported Exchanges

| Exchange    | Type                   |
| ----------- | ---------------------- |
| Bybit       | USDT Perpetual Futures |
| Hyperliquid | Perpetual DEX          |
