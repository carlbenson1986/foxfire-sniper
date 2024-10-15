// @generated automatically by Diesel CLI.

diesel::table! {
    bot_events (timestamp) {
        timestamp -> Timestamptz,
        event_type -> Text,
        event_data -> Jsonb,
    }
}

diesel::table! {
    depositswithdrawals (id) {
        id -> Int4,
        user_id -> Nullable<Int4>,
        time -> Timestamptz,
        is_deposit -> Bool,
        amount_sol -> Nullable<Float8>,
        is_success -> Bool,
        signature -> Text,
        signature_fee -> Nullable<Text>,
        fee_taken_sol -> Nullable<Float8>,
        description -> Nullable<Text>,
    }
}

diesel::table! {
    prices (id) {
        id -> Int4,
        pool -> Varchar,
        price -> Float8,
        created_at -> Timestamp,
        base_reserve -> Nullable<Float8>,
        quote_reserve -> Nullable<Float8>,
    }
}

diesel::table! {
    snipingstrategyinstances (id) {
        id -> Int4,
        user_id -> Int4,
        sniper_private_key -> Text,
        started_at -> Timestamptz,
        completed_at -> Nullable<Timestamptz>,
        size_sol -> Float8,
        stop_loss_percent_move_down -> Float8,
        take_profit_percent_move_up -> Float8,
        force_exit_horizon_s -> Int8,
        max_simultaneous_snipes -> Int8,
        min_pool_liquidity_sol -> Float8,
        skip_pump_fun -> Bool,
        skip_mintable -> Bool,
        buy_delay_ms -> Int8,
        skip_if_price_drops_percent -> Float8,
    }
}

diesel::table! {
    solana_actions (uuid) {
        uuid -> Text,
        sniper -> Text,
        fee_payer -> Text,
        created_at -> Timestamptz,
        action_payload -> Jsonb,
        status -> Nullable<Text>,
        tx_hash -> Nullable<Text>,
        balance_before -> Nullable<Jsonb>,
        balance_after -> Nullable<Jsonb>,
        fee -> Nullable<Int8>,
        sent_at -> Nullable<Timestamptz>,
        confirmed_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    traders (id) {
        id -> Int4,
        strategy_instance_id -> Nullable<Int4>,
        wallet -> Text,
        private_key -> Text,
        created -> Timestamptz,
        is_active -> Bool,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        chat_id -> Int8,
        tg_name -> Text,
        wallet_address -> Text,
        wallet_private_key -> Text,
        created -> Timestamptz,
        last_login -> Timestamptz,
        first_name -> Nullable<Text>,
        last_name -> Nullable<Text>,
        is_active -> Bool,
        is_superuser -> Bool,
    }
}

diesel::table! {
    volumestrategyinstances (id) {
        id -> Int4,
        user_id -> Int4,
        target_pool -> Text,
        started_at -> Timestamptz,
        completed_at -> Nullable<Timestamptz>,
        tranche_size_sol -> Float8,
        tranche_frequency_hbs -> Int8,
        tranche_length_hbs -> Int8,
        agents_buying_in_tranche -> Int4,
        agents_selling_in_tranche -> Int4,
        agents_keep_tokens_lamports -> Int8,
    }
}

diesel::joinable!(depositswithdrawals -> users (user_id));
diesel::joinable!(snipingstrategyinstances -> users (user_id));
diesel::joinable!(traders -> volumestrategyinstances (strategy_instance_id));
diesel::joinable!(volumestrategyinstances -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    bot_events,
    depositswithdrawals,
    prices,
    snipingstrategyinstances,
    solana_actions,
    traders,
    users,
    volumestrategyinstances,
);
