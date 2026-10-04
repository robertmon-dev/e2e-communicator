# ==============================================================================
# e2e-communicator Makefile
# ==============================================================================

DOCKER_COMPOSE := docker compose -f docker/docker-compose.yml

CYAN  := \033[36m
GREEN := \033[32m
RESET := \033[0m

.DEFAULT_GOAL := help

# ==============================================================================
# Utility
# ==============================================================================

.PHONY: help
help: ## Display this help message
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "$(CYAN)%-16s$(RESET) %s\n", $$1, $$2}'

# ==============================================================================
# Build & Test (Cargo)
# ==============================================================================

.PHONY: build
build: ## Build all workspace crates
	@printf "$(CYAN)=> Building workspace...$(RESET)\n"
	@cargo build --workspace
	@printf "$(GREEN)=> Build complete.$(RESET)\n"

.PHONY: build-release
build-release: ## Build all workspace crates in release mode
	@printf "$(CYAN)=> Building workspace (release)...$(RESET)\n"
	@cargo build --workspace --release
	@printf "$(GREEN)=> Release build complete.$(RESET)\n"

.PHONY: test
test: ## Run all workspace tests
	@printf "$(CYAN)=> Running tests...$(RESET)\n"
	@cargo test --workspace
	@printf "$(GREEN)=> Tests complete.$(RESET)\n"

.PHONY: check
check: ## Type-check the workspace without building artifacts
	@printf "$(CYAN)=> Checking workspace...$(RESET)\n"
	@cargo check --workspace

.PHONY: clean
clean: ## Remove build artifacts
	@printf "$(CYAN)=> Cleaning build artifacts...$(RESET)\n"
	@cargo clean
	@printf "$(GREEN)=> Clean complete.$(RESET)\n"

# ==============================================================================
# Run
# ==============================================================================

.PHONY: run-server
run-server: ## Run the server
	@printf "$(CYAN)=> Starting server...$(RESET)\n"
	@cd server && cargo run

.PHONY: run-client
run-client: ## Run the client
	@printf "$(CYAN)=> Starting client...$(RESET)\n"
	@cd client && cargo run --bin client

# ==============================================================================
# Quality
# ==============================================================================

.PHONY: fmt
fmt: ## Format code
	@printf "$(CYAN)=> Formatting code...$(RESET)\n"
	@cargo fmt
	@printf "$(GREEN)=> Format complete.$(RESET)\n"

.PHONY: fmt-check
fmt-check: ## Check formatting without modifying files
	@printf "$(CYAN)=> Checking formatting...$(RESET)\n"
	@cargo fmt --check

.PHONY: lint
lint: ## Run clippy (reports warnings; the workspace has pre-existing ones, so this doesn't fail on them)
	@printf "$(CYAN)=> Running clippy...$(RESET)\n"
	@cargo clippy --workspace --all-targets

# ==============================================================================
# Environment
# ==============================================================================

.PHONY: env-setup
env-setup: ## Copy .env.sample -> .env for client and server (skips existing files)
	@printf "$(CYAN)=> Setting up env files...$(RESET)\n"
	@[ -f client/.env ] || cp client/.env.sample client/.env
	@[ -f server/.env ] || cp server/.env.sample server/.env
	@printf "$(GREEN)=> env files ready: client/.env, server/.env$(RESET)\n"

# ==============================================================================
# Database (sea-orm migrations)
# ==============================================================================

.PHONY: migrate-up
migrate-up: ## Apply pending client database migrations
	@printf "$(CYAN)=> Applying migrations...$(RESET)\n"
	@cd client && cargo run -p migration -- up

.PHONY: migrate-down
migrate-down: ## Revert the last client database migration
	@printf "$(CYAN)=> Reverting last migration...$(RESET)\n"
	@cd client && cargo run -p migration -- down

.PHONY: migrate-status
migrate-status: ## Show applied/pending client database migrations
	@cd client && cargo run -p migration -- status

.PHONY: migrate-new
migrate-new: ## Generate a new client migration (usage: make migrate-new NAME=add_foo)
	@printf "$(CYAN)=> Generating migration $(NAME)...$(RESET)\n"
	@cd client && sea-orm-cli migrate generate -d migration/src $(NAME)

# ==============================================================================
# Infrastructure (Redis Sentinel)
# ==============================================================================

.PHONY: redis-up
redis-up: ## Start local Redis Sentinel stack
	@printf "$(CYAN)=> Starting Redis Sentinel stack...$(RESET)\n"
	@$(DOCKER_COMPOSE) up -d
	@printf "$(GREEN)=> Redis Sentinel stack is up.$(RESET)\n"

.PHONY: redis-down
redis-down: ## Stop local Redis Sentinel stack
	@printf "$(CYAN)=> Stopping Redis Sentinel stack...$(RESET)\n"
	@$(DOCKER_COMPOSE) down

.PHONY: redis-logs
redis-logs: ## Tail logs from the Redis Sentinel stack
	@$(DOCKER_COMPOSE) logs -f

# ==============================================================================
# Convenience
# ==============================================================================

.PHONY: dev
dev: env-setup redis-up ## Bootstrap a local dev environment (env files + redis)
	@printf "$(GREEN)=> Dev environment ready. Run 'make run-server' / 'make run-client' to start.$(RESET)\n"
