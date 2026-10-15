### Overview
This project allows indexing blocks of a given Ethereum compatible blockchain (mainnet, testnet, sepolia, etc.). 
It stores every data of every block (transaction, log, account etc.) and can detect reorg and 
correct itself by deleting all unneeded entities.

### Architecture
The app runs in one of five modes, selected with `--mode`: indexer, simulator, migration, create-user and disable-user
- indexer: This is the main mode. It indexes the blockchain, and stores its entities in postgres database.
It also exposes the database through an HTTP server. Endpoints require a Bearer JWT (access token). The app issues
access and refresh tokens through `POST /api/auth/signup` (if `enable_http_signup`), `/api/auth/login` and `/api/auth/refresh`
(see `src/auth`). Users with other roles are created with `cargo run -- --mode create-user --username <name> --roles admin,user --config example.env`.
Users are disabled (not deleted) with `--mode disable-user --username <name>`.
- simulator: simulates executing transactions. It also exposes an HTTP api
to see data of executed transactions; this is stored in the memory and would go away
on app shutdown.
- migration: runs the database migrations.

### Indexer
It is executed using the following command: `cargo run -- --mode indexer --config example.env`

### Database
It stores data in postgres. Migration is run using the following command: `cargo run -- --mode migration --config example.env`
With `default_user_enabled=true` the migration command also creates the default user from the `default_user_*` config entries, but only while the users table is empty.

### Testing
Tests are grouped into normal tests and integration tests. Normal tests are unit tests, but integration
tests are database-oriented tests (and might contain http or server-bound tests as well).
Run normal tests with: `cargo test`
When running integration tests, make sure test database configs are set properly; to run integration tests
point `CONFIG_FILE` at a config with test database credentials and pass the feature flag:
`CONFIG_FILE=path/to/test.env cargo test --features integration-tests`

### Coding
Make sure the code is as simple as possible. Do not overengineer.
Make sure any corresponding repository has a mock version. Keep the mocks both updated and relevant.
Make sure logging is done properly using "tracing" crate. Do not use raw printing. Do not log unnecessary info.
In this project consistency wins (must win) over availability in areas where one must be explicitly chosen.


### HTTP Servers
- For indexer mode, all http APIs are served under: `/api/indexer/`
- For simulator mode, all http APIs are served under: `/api/simulator/`

