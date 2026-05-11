#!/bin/bash

set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
PURPLE='\033[0;35m'
NC='\033[0m'

PORT=5167

log_info()    { echo -e "${BLUE}ℹ️  [INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}✅ [SUCCESS]${NC} $1"; }
log_warning() { echo -e "${YELLOW}⚠️  [WARNING]${NC} $1"; }
log_error()   { echo -e "${RED}❌ [ERROR]${NC} $1"; return 1; }
log_section() { echo -e "\n${PURPLE}🔄 === $1 ===${NC}\n"; }

handle_error() {
    log_error "$1"
    cleanup
    exit 1
}

cleanup() {
    log_section "Cleanup"
    if [ -n "$PYTHON_PID" ]; then
        log_info "Stopping Python backend..."
        kill -9 $PYTHON_PID 2>/dev/null || true
        log_success "Python backend stopped"
    fi
}

trap cleanup EXIT INT TERM

log_section "Environment Check"

if [ ! -d "app" ]; then
    handle_error "Python backend directory not found. Please check your installation"
fi

if [ ! -f "app/main.py" ]; then
    handle_error "Python backend main.py not found. Please check your installation"
fi

if [ ! -d "venv" ]; then
    handle_error "Virtual environment not found. Please run: python -m venv venv && source venv/bin/activate && pip install -r requirements.txt"
fi

log_section "Backend App Check"

log_info "Checking for processes on port $PORT..."
if lsof -i :$PORT | grep -q LISTEN; then
    log_warning "Backend app is running on port $PORT"
    read -p "$(echo -e "${YELLOW}🤔 Kill it? (y/N)${NC} ")" -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        handle_error "User chose not to terminate existing backend app"
    fi
    kill -9 $(lsof -t -i :$PORT) 2>/dev/null || true
    log_success "Backend app terminated"
    sleep 1
fi

log_section "Starting Python Backend"

if [ -z "$VIRTUAL_ENV" ]; then
    log_info "Activating virtual environment..."
    source venv/bin/activate || handle_error "Failed to activate virtual environment"
fi

pip show fastapi >/dev/null 2>&1 || handle_error "FastAPI not found. Run: pip install -r requirements.txt"

source venv/bin/activate && python app/main.py &
PYTHON_PID=$!

sleep 5
if ! kill -0 $PYTHON_PID 2>/dev/null; then
    handle_error "Python backend failed to start"
fi

log_success "🎉 Python backend started (PID: $PYTHON_PID) on port $PORT"
echo -e "${BLUE}Press Ctrl+C to stop${NC}"

wait $PYTHON_PID
