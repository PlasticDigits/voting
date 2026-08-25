.PHONY: test test-lib test-pg test-frontend test-e2e

test-lib:
	cargo test --workspace --lib --bins

test-pg:
	LEDGER_TEST_DATABASE_URL=$${LEDGER_TEST_DATABASE_URL:-postgres://voting:voting@127.0.0.1:5433/voting} \
		cargo test --workspace --tests -- --test-threads=1

test-frontend:
	cd frontend && npm test

test-e2e:
	cd frontend && npm run test:e2e

test: test-lib test-frontend
