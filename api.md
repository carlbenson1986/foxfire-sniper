### API Endpoints for the Solana Market-Making Bots

#### Client Authentication

- **Login**
    - **Endpoint:** `POST /api/auth/login`
    - **Description:** Authenticates the client by verifying the signed message from the Phantom wallet (or similar).
    - **Request Body:**
      ```json
      {
        "wallet_address": "string", // User wallet
        "signed_message": "string", // The signed message from the Solana wallet, e.g., Phantom - will be checked against the timestamp and the message
        "message": "string"  // The message that was signed, generated on the frontend with the timestamp, e.g., "Login to Solana Market-Making Bot at 2024-07-01 12:00:00"
      }
      ```
    - **Response:**
      ```json
      {
        "success": true,
        "message": "Login successful",
        "deposit_wallet": "string",
        "client_subscription": "int" // Subscription plan, e.g. 0 - free, 1 - volume only, 2 - mm only, 3 - premium, all included
      }
      ```

#### Client Management

- **New Customer**
    - **Endpoint:** `POST /api/user/new`
    - **Description:** Registers a new user. We assume a user can handle multiple tokens, so a pair user-token is a
      unique key 1-1 related to a subscription.
    - **Request Body:**
      ```json
      {
        "wallet_address": "string", // User wallet
        "signed_message": "string", // The signed message from the Solana wallet
        "message": "string" // The message that was signed
      }
      ```
    - **Response:**
      ```json
      {
        "success": true,
        "message": "User registered successfully",
        "deposit_address": "string" // deposit wallet address
      }
      ```

#### Subscription Management

- **Set Subscription for a Token to Pump**
    - **Endpoint:** `POST /api/subscription`
    - **Description:** Sets or updates the subscription plan for a client-token pair. Payments will be taken
      automatically from the user's wallet.
    - **Request Body:**
      ```json
      {
        "wallet_address": "string", // User wallet
        "signed_message": "string", // The signed message from the Solana wallet
        "message": "string", // The message that was signed
        "token_id": "string", // Token address - that's a subject of market making for which the subscription is being set
        "subscription": "int" // Subscription plan, e.g., 0 - free (changed forcefully to 0 if ti was >0 but no funds on the wallet), 1 - volume only, 2 - mm only, 3 - premium, all included
        "subscribed_at": "datetime" // The date and time when the subscription was set,UTC  e.g., "2024-07-01 12:00:00"
      }
      ```
    - **Response:**
      ```json
      {
        "success": true,
        "message": "Subscription updated successfully",
        "client_subscription": "int" // The updated subscription plan, the payment is managed automatically
      }
      ```

- **Check Payment Status for Token**
    - **Endpoint:** `GET /api/subscription/payment/status`
    - **Description:** Checks the payment status of the subscription for a client-token pair.
    - **Query Parameters:**
        - `wallet_address`: User wallet
        - `message`: message to sign with the timestamp
        - `signed_message`:The signed message from the Solana wallet
    - **Response:**
      ```json
      {
        "success": true,
        "message": "Payment status retrieved successfully",
       "payments": [ // List of payments with dates
        {
        "date": "datetime", // Date of the payment
        "amount": "float", // Amount of the payment in SOL 
        "subscription": "int", // Subscription tier that was paid for
        "tx_hash": "string" // tx_hash of the payment
        }
        ]
      }
      ```

#### Client Deposit

- **Get Deposit Wallet Address**
    - **Endpoint:** `GET /api/deposit/address`
    - **Description:** Retrieves the deposit wallet address for the client to deposit funds.
    - **Query Parameters:**
        - `wallet_address`: User wallet, the deposit wallet is not a secret, anyone can deposit funds to it, since it's
          a one-way operation
    - **Response:**
      ```json
      {
        "deposit_wallet": "string"
      }
      ```

#### Bot Management

- **Start Bot**
    - **Endpoint:** `POST /api/bot/start`
    - **Description:** Starts the market-making bot for the client. Requires Solana wallet signature for authentication.
    - **Request Body:**
      ```json
      {
        "wallet_address": "string",
        "signed_message": "string",
        "message": "string",  // The message that was signed
        "client_id": "int",
        "strategy_id": "int" // The ID of the strategy to use for the bot, - 0 for the default strategy 
      }
      ```
    - **Response:**
      ```json
      {
        "success": true,
        "start_time": "datetime",
        "message": "Bot started successfully"
      }
      ```

- **Stop Bot**
    - **Endpoint:** `POST /api/bot/stop`
    - **Description:** Stops the market-making bot for the client. Requires Solana wallet signature for authentication.
    - **Request Body:**
      ```json
      {
        "wallet_address": "string",
        "signed_message": "string",
        "message": "string",  // The message that was signed
        "client_id": "int",
        "strategy_id": "int"
      }
      ```
    - **Response:**
      ```json
      {
        "success": true,
        "stop_time": "datetime",
        "message": "Bot stopped successfully"
      }
      ```

- **Get Bot Status**
    - **Endpoint:** `GET /api/bot/status`
    - **Description:** Retrieves the status of the market-making bot for the client.
    - **Query Parameters:**
        - `client_id`: The ID of the client.
        - `strategy_id`: The ID of the strategy.
    - **Response:**
      ```json
      {
        "success": true,
        "message": "Bot status retrieved successfully",
        "data": {
          "status": "string", // e.g., "running", "stopped", "error"
          "last_start_time": "datetime",
          "last_stop_time": "datetime", // if running - null
          "error_message": "string" // if any
        }
      }
      ```
#### Strategy Management

Not sure here - probably it'd be better to make presets since it's hard to explain all these parameters to the user, or
make defaults for the most advanced control.
Probably we can recommend parameters for the strategy and let the user adjust them if needed.

- **Create Strategy**
    - **Endpoint:** `POST /api/strategy`
    - **Description:** Creates a new market-making strategy for the client.
    - **Request Body:**
      ```json
      {
        "client_id": "int",
        "tranche_size_sol": "float", // The interval in ticks to perform a trade of the summarized volume
        "tranche_frequency": "int", // The number of ticks used to trade the summarized volume
        "tranche_length": "int",
        "min_agents": "int",
        "max_agents": "int", // 0 for default
        "proportion": "float",  // The proportion of the agents of the whole population to trade in the tranche
        "algorithm": "string", // The algorithm to use for the market-making strategy, e.g., "bollinger", "random"
        "target_pool": "string" // The target pool for the market-making strategy
      }
      ```
    - **Response:**
      ```json
      {
        "success": true,
        "message": "Strategy created successfully",
        "strategy_id": "int"
      }
      ```

- **Update Strategy**
    - **Endpoint:** `PUT /api/strategy/{strategy_id}`
    - **Description:** Creates a new market-making strategy for the client.
    - **Request Body:**
      ```json
      {
        "client_id": "int",
        "tranche_size_sol": "float", // The interval in ticks to perform a trade of the summarized volume
        "tranche_frequency": "int", // The number of ticks used to trade the summarized volume
        "tranche_length": "int",
        "min_agents": "int",
        "max_agents": "int", // 0 for default
        "proportion": "float",  // The proportion of the agents of the whole population to trade in the tranche
        "algorithm": "string",
        "target_pool": "string" // The target pool for the market-making strategy
      }
      ```
    - **Response:**
      ```json
      { 
        "success": true,
        "message": "Strategy created successfully"
      }
      ```

- **Get Strategy**
    - **Endpoint:** `GET /api/strategy/{strategy_id}`
    - **Description:** Retrieves the details of a specific strategy.
    - **Response:**
      ```json
      {
        "strategy_id": "string",
        "client_id": "int",
        "tranche_size_sol": "float",
        "tranche_frequency": "int",
        "tranche_length": "int",
        "min_agents": "int",
        "max_agents": "int",
        "proportion": "float",
        "target_pool": "string"
      }
      ```

#### Event and Action Logs

- **Get Event Logs**
    - **Endpoint:** `GET /api/logs/events`
    - **Description:** Retrieves event logs related to the client's strategy.
    - **Query Parameters:**
        - `client_id`: Filter logs by client ID (optional).
        - `strategy_id`: Filter logs by strategy ID (optional).
    - **Response:**
      ```json
      {
        "events": [
          {
            "id": "int",
            "timestamp": "datetime",
            "strategy_id": "int",
            "event_type": "string",
            "event_data": "string"
          }
        ]
      }
      ```

- **Get Execution Logs**
    - **Endpoint:** `GET /api/logs/executions`
    - **Description:** Retrieves execution logs related to the client's strategy actions.
    - **Query Parameters:**
        - `client_id`: Filter logs by client ID (optional).
        - `strategy_id`: Filter logs by strategy ID (optional).
        - `action_id`: Filter logs by action ID (optional).
    - **Response:**
      ```json
      {
        "executions": [
          {
            "id": "int",
            "timestamp": "datetime",
            "action_id": "int",
            "success": "bool",
            "result_data": "string"
          }
        ]
      }
      ```

#### Get Strategy Effectiveness Data

- **Endpoint:** `GET /api/strategy/{strategy_id}/stats`
- **Description:** Retrieves various parameters to measure the effectiveness of the market-making strategy.
- **Query Parameters:**
    - `strategy_id`: The ID of the strategy.
    - `start_date`: The start date for the data range (optional).
    - `end_date`: The end date for the data range (optional).
- **Response:**
  ```json
  {
    "success": true,
    "message": "Effectiveness data retrieved successfully",
    "data": {
      "trade_count": "int",
      "average_trade_size": "float",
      "volume_generated": "float",
      "win_rate": "float", // Percentage
      "liquidity": "float",
      "spread": "float",
      "market_impact": "float",
      "execution_efficiency": "float",
      "pnl": "float",
      "profit_factor": "float",
      "cost_of_trades": "float", // gas fees, raydium %, and other exeuction fees like bloxroute tip
      "volume_levels": [
        {
          "date": "string",
          "volume": "float" // only generated by the strategy - used for charts
        }
      ],
      "pnl_levels": [
        {
          "date": "string",
          "pnl": "float"  // only generated by the strategy - used for charts
        }
      ],
      "inventory_levels": [
        {
          "date": "string",
          "level": "float"
        }
      ],
      "sol_levels": [
        {
          "date": "string",
          "level": "float"
        }
      ],
      "price_deviation": "float"
    }
  }
  ```

### Example Request to Get Strategy Effectiveness Data

- **Example Request:**
  ```bash
  GET /api/strategy/1/stats?start_date=2024-07-01&end_date=2024-07-31
  ```
