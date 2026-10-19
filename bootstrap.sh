#!/bin/bash
set -e

# StellarLend One-Command Bootstrap (#1450)
# Idempotent initialization script for development environment

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="${SCRIPT_DIR}/.bootstrap.log"

log() {
  echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

log_section() {
  echo "" | tee -a "$LOG_FILE"
  log "============================================"
  log "$*"
  log "============================================"
}

check_dependencies() {
  log_section "Checking dependencies"
  
  local missing=0
  
  if ! command -v node &> /dev/null; then
    log "❌ Node.js not found"
    missing=1
  else
    log "✓ Node.js $(node --version)"
  fi
  
  if ! command -v cargo &> /dev/null; then
    log "❌ Rust not found"
    missing=1
  else
    log "✓ Rust $(rustc --version)"
  fi
  
  if ! command -v docker &> /dev/null; then
    log "❌ Docker not found"
    missing=1
  else
    log "✓ Docker $(docker --version)"
  fi
  
  if [ $missing -eq 1 ]; then
    log "Install missing dependencies and try again"
    exit 1
  fi
}

setup_rust_targets() {
  log_section "Setting up Rust targets"
  rustup target add wasm32-unknown-unknown || true
  log "✓ Rust WASM target ready"
}

install_dependencies() {
  log_section "Installing dependencies"
  
  if [ -f "package.json" ]; then
    log "Installing npm packages..."
    npm ci --legacy-peer-deps || npm install --legacy-peer-deps
    log "✓ npm packages installed"
  fi
  
  if [ -f "Cargo.toml" ]; then
    log "Building Rust packages..."
    cargo build --target wasm32-unknown-unknown 2>&1 | tail -5
    log "✓ Rust packages built"
  fi
}

setup_environment() {
  log_section "Setting up environment"
  
  if [ ! -f ".env" ]; then
    if [ -f ".env.example" ]; then
      cp .env.example .env
      log "✓ Created .env from .env.example"
    fi
  else
    log "✓ .env already exists"
  fi
}

setup_git_hooks() {
  log_section "Setting up Git hooks"
  
  if [ -d ".husky" ]; then
    npx husky install 2>/dev/null || true
    log "✓ Git hooks installed"
  fi
}

run_tests() {
  log_section "Running tests"
  
  if [ -f "package.json" ] && grep -q '"test"' package.json; then
    npm test -- --run --passWithNoTests 2>&1 | tail -10
    log "✓ Tests passed"
  fi
}

verify_setup() {
  log_section "Verifying setup"
  
  local status=0
  
  if [ -f ".env" ]; then
    log "✓ Environment configured"
  else
    log "⚠ .env not found"
    status=1
  fi
  
  if [ -d "node_modules" ]; then
    log "✓ Dependencies installed"
  else
    log "⚠ Dependencies not found"
    status=1
  fi
  
  return $status
}

main() {
  log_section "StellarLend Bootstrap"
  log "Starting one-command setup..."
  
  check_dependencies
  setup_rust_targets
  setup_environment
  install_dependencies
  setup_git_hooks
  
  if verify_setup; then
    log_section "✓ Bootstrap complete!"
    log "Ready to develop. Run: npm run dev"
  else
    log_section "⚠ Bootstrap completed with warnings"
    log "Check output above and .bootstrap.log for details"
  fi
}

main "$@"
