## Ethereum Simulator & Indexer
This is a rust project that enables you to simulate
transaction executions or index the blockchain.

This project exposes two modes: Simulator and Indexer. Database migration and user management are
separate commands (see below).

### Simulator
Simulator is a simple pending transaction executor (from a configured upstream)
and you can query the executed transactions through the exposed HTTP API (listed further below).

How to run simulator:

`./eth_app --mode simulator --config example.env`



### Indexer
This literally indexes and stores blocks, transactions, account, logs and events
fetched from the upstream (Ethereum mainnet for example). It uses a postgres
database to store the indexed content. A set of HTTP endpoint allows you
to search and view various indexed entities (such as a block or list of transactions).

`./eth_app --mode indexer --config example.env`

#### HTTP API
All routes are under `/api`:

- `/api/indexer/` : indexed blocks (`blocks/list`, `blocks/number/{number}`, `blocks/hash/{hash}`)
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

Settings (see `example.env`): `jwt_secret` (at least 32 characters, the built-in default is a public
dummy: always set your own), `access_token_expiry`, `refresh_token_expiry`, `enable_http_signup`.

#### Managing users from the command line
Create a user (the password is asked for interactively; roles default to `user`):

`./eth_app --mode create-user --username alice --roles admin,user --config example.env`

Disable a user:

`./eth_app --mode disable-user --username alice --config example.env`

A disabled user can no longer log in or refresh, and its refresh tokens are revoked. Access tokens that were
already issued stay valid until they expire (`access_token_expiry`). The user is kept, so its username cannot
be signed up again. To enable it again, run `update users set is_active = true where username = 'alice'`.

#### Default user
With `default_user_enabled=true`, the migration command creates the user described by `default_user_username`,
`default_user_password` and `default_user_roles` (default `admin`), but only while there are no users at all:

`./eth_app --mode migration --config example.env`

The default user has well-known credentials, so disable it (see above) after the first launch and set
`default_user_enabled=false`. A disabled default user is not created again.


#### Running Tests
Tests can be run normally via Cargo's test command:
```
cargo test
```

Integration tests (need a running Postgres; `db_pg_url` is read from the config file, and the test applies migrations itself):

```
CONFIG_FILE=example.env cargo test --features integration-tests
```

