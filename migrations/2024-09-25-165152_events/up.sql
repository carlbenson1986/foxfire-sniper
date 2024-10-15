-- Create table to store Raydium swap events with strategy and pool data
-- Up migration
CREATE TABLE bot_events
(
    timestamp  TIMESTAMPTZ NOT NULL PRIMARY KEY,
    event_type TEXT        NOT NULL,
    event_data JSONB       NOT NULL
);

-- Create a hypertable
SELECT create_hypertable('bot_events', 'timestamp');

-- Up migration
CREATE TABLE solana_actions
(
    uuid           TEXT PRIMARY KEY,
    sniper         TEXT        NOT NULL,
    fee_payer      TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL,
    action_payload JSONB       NOT NULL,
    status         TEXT,
    tx_hash        TEXT,
    balance_before JSONB,
    balance_after  JSONB,
    fee            BIGINT,
    sent_at        TIMESTAMPTZ,
    confirmed_at   TIMESTAMPTZ
);
