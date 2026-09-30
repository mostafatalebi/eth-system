CONFIG_FILE ?= example.env

.PHONY: config
migrate:
	cargo run -- --mode migration --config $(CONFIG_FILE)
