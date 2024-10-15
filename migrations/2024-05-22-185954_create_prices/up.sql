CREATE TABLE prices (
                        id SERIAL PRIMARY KEY,
                        pool VARCHAR NOT NULL,
                        price DOUBLE PRECISION NOT NULL,
                        created_at TIMESTAMP NOT NULL
);
