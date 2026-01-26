#!/usr/bin/env bash

# Test Framework for Otto Integration Tests
# Provides helper functions for running tests with colored output

# Test counters
TEST_COUNT=0
TEST_PASS=0
TEST_FAIL=0

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test functions
test_start() {
    local test_name="$1"
    TEST_COUNT=$((TEST_COUNT + 1))
    echo -n "Test ${TEST_COUNT}: ${test_name}... "
}

test_pass() {
    TEST_PASS=$((TEST_PASS + 1))
    echo -e "${GREEN}PASS${NC}"
}

test_fail() {
    local reason="$1"
    TEST_FAIL=$((TEST_FAIL + 1))
    echo -e "${RED}FAIL${NC}"
    if [[ -n "${reason}" ]]; then
        echo "  Reason: ${reason}"
    fi
}

test_skip() {
    local reason="$1"
    echo -e "${YELLOW}SKIP${NC}"
    if [[ -n "${reason}" ]]; then
        echo "  Reason: ${reason}"
    fi
}
