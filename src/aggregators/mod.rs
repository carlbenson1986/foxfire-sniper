pub mod every_tick_indicators;
mod indicators_data;
pub mod period_indicators;
pub(crate) mod tick_bar;
pub mod tick_indicators_aggregator;
// 1. Technical Indicators:
// Moving Averages (Simple, Exponential, Weighted)
// Oscillators (RSI, Stochastic, MACD)
// Volatility Indicators (ATR, Bollinger Bands)
// Volume-based Indicators (OBV, VWAP)
// Momentum Indicators (ROC, Momentum)
// Chart Patterns:
//
// Candlestick Patterns
// Support/Resistance Levels
// Trend Lines
//
//
// Statistical Computations:
//
// Standard Deviation
// Correlation between assets
// Z-Score
//
//
// Time Series Transformations:
//
// Tick to Time Bar Conversion
// Volume Bar Formation
// Dollar Bar Formation
//
//
// Advanced Analytics:
//
// Fourier Transforms
// Wavelet Analysis
// ARIMA Models
//
//
// Machine Learning Preprocessors:
//
// Feature Scaling
// Principal Component Analysis (PCA)
// Time Series to Supervised Learning Conversion
//
//
// Market Microstructure Metrics (not applicable to Raydium V4, but useful for orderbook markets):
// Order Book Imbalance
// Trade Flow Imbalance
// Liquidity Measures
//
//
// Custom Composite Indicators:
//
// Any frequently used combination of other indicators
