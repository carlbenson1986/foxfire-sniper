-- Your SQL goes here
CREATE TABLE SnipingStrategyInstances
(
    id                          SERIAL PRIMARY KEY,
    user_id                     INTEGER          NOT NULL REFERENCES Users (id) ON DELETE CASCADE,
    sniper_private_key          TEXT             NOT NULL,
    started_at                  TIMESTAMPTZ      NOT NULL DEFAULT NOW(),
    completed_at                TIMESTAMPTZ,
    size_sol                    DOUBLE PRECISION NOT NULL,
    stop_loss_percent_move_down DOUBLE PRECISION NOT NULL,
    take_profit_percent_move_up DOUBLE PRECISION NOT NULL,
    force_exit_horizon_s        BIGINT           NOT NULL,
    max_simultaneous_snipes     BIGINT           NOT NULL,
    min_pool_liquidity_sol      DOUBLE PRECISION NOT NULL,
    skip_pump_fun               BOOLEAN          NOT NULL DEFAULT FALSE,
    skip_mintable               BOOLEAN          NOT NULL DEFAULT FALSE,
    buy_delay_ms                BIGINT           NOT NULL,
    skip_if_price_drops_percent DOUBLE PRECISION NOT NULL
);
