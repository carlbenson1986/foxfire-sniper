-- Drop the table that depends on other tables first

-- Drop the dependent table next
-- DROP TABLE IF EXISTS Traders CASCADE;

-- Drop tables that reference Users table
DROP TABLE IF EXISTS VolumeStrategyInstances CASCADE;
DROP TABLE IF EXISTS DepositsWithdrawals CASCADE;

-- Finally, drop the Users table
-- Don't want to get private keys dropped
-- DROP TABLE IF EXISTS Users CASCADE;

-- Drop the custom enum type
DROP TYPE IF EXISTS Action CASCADE;
