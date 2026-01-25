#!/usr/bin/env bash
# Test script to verify otto progress indicator functionality

set -e

echo "=== Otto Progress Indicator Test ==="
echo ""

# Build otto
echo "1. Building otto..."
cargo build --release --quiet
echo "   ✓ Build successful"
echo ""

# Check ready beads
echo "2. Checking ready beads..."
./target/release/otto --help > /dev/null 2>&1 || true
echo "   Ready beads:"
bd ready | head -5
echo ""

# Create a simple test task that completes quickly
echo "3. Creating test bead..."
TEST_TASK=$(bd create --title="Quick test: Create hello.txt with hello world" --type=task --priority=3 2>&1 | grep -oP 'otto-\w+' || true)
if [ -z "$TEST_TASK" ]; then
    echo "   Using existing test beads"
else
    echo "   ✓ Created $TEST_TASK"
fi
echo ""

# Test manual agent launch
echo "4. Testing progress indicator with a manual agent session..."
echo "   This will launch one agent session to demonstrate:"
echo "   - Progress indicator updating every 2 seconds"
echo "   - Continuously rewritten line on stderr"
echo "   - Session duration printed on completion"
echo ""
echo "   Starting otto in single-pass mode..."
echo ""

# Run otto once (it will pick one task and exit)
timeout 600 ./target/release/otto || true

echo ""
echo "5. Verifying session completion..."
echo "   Check above output for:"
echo "   ✓ 'Agent working... (Xs)' progress messages on stderr"
echo "   ✓ 'Agent finished (duration: X)' message on stdout"
echo ""

echo "=== Test Complete ==="
