## Ethereum Simulator & Indexer
This is a rust project that enables you to simulate
transaction executions or index the blockchain.

This project exposes two modes: Simulator and Indexer

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
```/simulator/```


#### Running Tests
Tests can be run normally via Cargo's test command:
```
cargo test
```

Integration tests (need a running Postgres; `db_pg_url` is read from the config file, and the test applies migrations itself):

```
CONFIG_FILE=example.env cargo test --features integration-tests
```

