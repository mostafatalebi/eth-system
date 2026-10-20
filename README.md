## Ethereum Simulator & Indexer
This is a rust project that enables you to simulate
transaction executions or index the blockchain.

This project exposes two modes: Simulator and Indexer. Database migration and user management are
separate commands (see below).

### Configuration
`config.env.example` is the committed template with placeholder values (they are nonsense on purpose: replace
them). Copy it to `example.env`, which is git-ignored, and put your real values (database password, upstream API
keys, `jwt_secret`) there. The commands below pass that file with `--config example.env`.
Every key the app requires must be in the template; a unit test fails when one is missing.

### Simulator
Simulator is a simple pending transaction executor (from a configured upstream)
and you can query the executed transactions through the exposed HTTP API (listed further below).

How to run simulator:

`./eth_app --mode simulator --config example.env`



### Indexer
This literally indexes and stores blocks, transactions, logs and events
fetched from the upstream (Ethereum mainnet for example). It uses a postgres
database to store the indexed content. A set of HTTP endpoint allows you
to search and view various indexed entities (such as a block or list of transactions).

`./eth_app --mode indexer --config example.env`

#### HTTP API
All routes are under `/api`:

- `/api/indexer/` : indexed blocks (`blocks/list`, `blocks/number/{number}`, `blocks/hash/{hash}`, `logs`, filterable by `address`, `topic0`..`topic3` and a block range, and `events`, the logs decoded into ERC-20/721/1155 events, searchable by `event`, `standard`, `address`, a block range and any decoded argument by name, for example `events?event=Transfer&to=0x…`)
- `/api/simulator/` : simulated transactions (`list`, `tx?id={hash}`)
- `/api/auth/` : `signup`, `login`, `refresh` (see Users and authentication)

A Postman collection of all endpoints is in `resources/api.postman_collection.json`.


### Users and authentication
Every `/api` endpoint except `/api/auth/*` needs the header `Authorization: Bearer <access_token>`.

| Endpoint | Body | Notes |
|---|---|---|
| `POST /api/auth/signup` | `{"username", "password"}` | Creates a user with the `user` role. Only served when `enable_http_signup=true`. Username: 3-32 characters of `a-z 0-9 _ - .`; password: 8-128 characters. |
| `POST /api/auth/login` | `{"username", "password"}` | |
| `POST /api/auth/refresh` | `{"refresh_token"}` | A refresh token can be used only once; the response contains a new one. |

Each of them returns `{"data": {"access_token": "...", "refresh_token": "..."}}`. The access token is a JWT
(HS256) with the payload `{"data": {"id", "username"}, "roles": [...], "iat", "exp"}`.

Settings (see `config.env.example`): `jwt_secret` (at least 32 characters, the built-in default is a public
dummy: always set your own), `access_token_expiry`, `refresh_token_expiry`, `enable_http_signup`.

Refresh tokens are stored in Redis (only their hash), under keys like `<redis_prefix>:refresh_token:<hash>`, and
Redis removes them when they expire. Set `redis_url` (default `redis://127.0.0.1:6379`) and `redis_prefix`
(default `eth_app`; use a different one per environment sharing a Redis). The HTTP server needs Redis at startup.
Users are stored in Postgres. Both sit behind repository traits in `src/auth/repository.rs`, so a store can be
swapped by implementing `UserRepository` or `RefreshTokenRepository`.

#### Managing users from the command line
A fresh database has no users, and none is created automatically (not by the migration, not on the first
launch). Users come from the `create-default-user` command, or from the signup endpoint when
`enable_http_signup=true`.

`create-default-user` creates the one user described by the config (both username and password are read from the
config file, nothing is asked for interactively):

```
default_user_username=admin
default_user_password=a-password-of-8-or-more-characters
default_user_roles=admin
```

`./eth_app --mode migration --config example.env`

`./eth_app --mode create-default-user --config example.env`

It is the only way to give a user roles other than `user` (`default_user_roles` defaults to `admin`), and it fails
if that username already exists. The password sits in the config file in plain text: keep that file out of git
(`example.env` is ignored), restrict its permissions, and empty `default_user_password` once the user exists.

Disable a user:

`./eth_app --mode disable-user --username alice --config example.env`

A disabled user can no longer log in or refresh: its refresh tokens are rejected, and they expire by themselves.
Access tokens that were already issued stay valid until they expire (`access_token_expiry`). The user is kept, so its username cannot
be signed up again. To enable it again, run `update users set is_active = true where username = 'alice'`.

#### Running Tests
Tests can be run normally via Cargo's test command:
```
cargo test
```

Integration tests (need a running Postgres; `db_pg_url` is read from the config file, and the test applies migrations itself):

```
CONFIG_FILE=example.env cargo test --features integration-tests
```

