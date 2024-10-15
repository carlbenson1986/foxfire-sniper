CREATE TABLE Users
(
    id                 SERIAL PRIMARY KEY,
    chat_id            BIGINT UNIQUE NOT NULL,
    tg_name            TEXT UNIQUE   NOT NULL,
    wallet_address     TEXT UNIQUE   NOT NULL,
    wallet_private_key TEXT UNIQUE   NOT NULL,
    created            TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    last_login         TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    first_name         TEXT,
    last_name          TEXT,
    is_active          BOOLEAN       NOT NULL DEFAULT TRUE,
    is_superuser       BOOLEAN       NOT NULL DEFAULT FALSE
);

INSERT INTO public.users (id, chat_id, tg_name, wallet_address, wallet_private_key, first_name, last_name, is_active, is_superuser) VALUES (0, 0, 'admin', '0x', '0x', 'admin', null, true, true);

CREATE TABLE DepositsWithdrawals
(
    id            SERIAL PRIMARY KEY,
    user_id       INTEGER REFERENCES Users (id) ON DELETE CASCADE,
    time          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_deposit    BOOLEAN     NOT NULL DEFAULT TRUE,
    amount_sol    DOUBLE PRECISION,
    is_success    BOOLEAN     NOT NULL DEFAULT FALSE,
    signature     TEXT        NOT NULL,
    signature_fee TEXT,
    fee_taken_sol FLOAT,
    description   TEXT
);


CREATE TABLE VolumeStrategyInstances
(
    id                          SERIAL PRIMARY KEY,
    user_id                     INTEGER          NOT NULL REFERENCES Users (id) ON DELETE CASCADE,
    target_pool                 TEXT             NOT NULL,
    started_at                  TIMESTAMPTZ      NOT NULL DEFAULT NOW(),
    completed_at                TIMESTAMPTZ,
    tranche_size_sol            DOUBLE PRECISION NOT NULL,
    tranche_frequency_hbs       BIGINT           NOT NULL,
    tranche_length_hbs          BIGINT           NOT NULL,
    agents_buying_in_tranche    INTEGER          NOT NULL,
    agents_selling_in_tranche   INTEGER          NOT NULL,
    agents_keep_tokens_lamports BIGINT           NOT NULL
);

CREATE TABLE Traders
(
    id                   SERIAL PRIMARY KEY,
    strategy_instance_id INTEGER REFERENCES VolumeStrategyInstances (id),
    wallet               TEXT        NOT NULL,
    private_key          TEXT        NOT NULL,
    created              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_active            BOOLEAN     NOT NULL DEFAULT TRUE
);