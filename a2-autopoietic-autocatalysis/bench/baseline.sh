#!/usr/bin/env bash
# Baseline benchmark: run a model DIRECTLY on tasks without A² machinery.
# Compares raw model capability against A²-mediated capability.
#
# Usage: ./bench/baseline.sh [model_provider/model]
# Default: zai-coding-plan/glm-5.1

set -euo pipefail

MODEL="${1:-zai-coding-plan/glm-5.1}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Git root may differ from REPO_ROOT if A² is inside a monorepo
GIT_ROOT="$(git -C "$REPO_ROOT" rev-parse --show-toplevel)"
# Relative path from git root to A² project root
A2_PREFIX="$(python3 -c "import os; print(os.path.relpath('$REPO_ROOT', '$GIT_ROOT'))")"
BASELINE_REF="bench-baseline"
RESULTS_FILE="/tmp/a2_baseline_results.txt"

echo "A² Baseline Benchmark — raw model without A² machinery"
echo "Model: $MODEL"
echo "Baseline ref: $BASELINE_REF"
echo ""

> "$RESULTS_FILE"
PASS=0
FAIL=0
TOTAL=0

for task_file in "$REPO_ROOT/bench/tasks/"*.toml; do
    TITLE=$(grep '^title' "$task_file" | head -1 | sed 's/title = "\(.*\)"/\1/')
    DESCRIPTION=$(sed -n '/^description/,/"""/p' "$task_file" | sed '1s/description = """//' | sed '$d')
    VERIFY_CMD=$(grep '^command' "$task_file" | head -1 | sed 's/command = "\(.*\)"/\1/')
    TEST_FILE=$(grep '^test_file' "$task_file" | head -1 | sed 's/test_file = "\(.*\)"/\1/')
    # Extract test_content between triple quotes
    TEST_CONTENT=$(sed -n '/^test_content = """/,/^"""/p' "$task_file" | sed '1d;$d')

    TOTAL=$((TOTAL + 1))
    echo "[$TOTAL] $TITLE"

    # Create worktree from baseline
    BRANCH="baseline-$(uuidgen | tr '[:upper:]' '[:lower:]' | head -c 8)"
    WORKTREE="/tmp/$BRANCH"
    git -C "$GIT_ROOT" worktree add -b "$BRANCH" "$WORKTREE" "$BASELINE_REF" 2>/dev/null

    # Working dir inside the worktree (accounts for monorepo nesting)
    WORK_DIR="$WORKTREE/$A2_PREFIX"

    # Append test content to the worktree (not workspace)
    if [ -n "$TEST_FILE" ] && [ -n "$TEST_CONTENT" ]; then
        echo "$TEST_CONTENT" >> "$WORK_DIR/$TEST_FILE"
    fi

    # Run model directly in worktree
    START=$(date +%s)
    opencode run \
        --model "$MODEL" \
        --dir "$WORK_DIR" \
        --format json \
        "You are in a Rust project at $WORK_DIR. Your task: $TITLE

$DESCRIPTION

After making changes, run: $VERIFY_CMD

Make sure the verify command passes before finishing." \
        < /dev/null 2>/dev/null \
        | jq -r 'select(.type == "text") | .text // empty' \
        > /dev/null 2>&1 || true
    END=$(date +%s)
    DURATION=$((END - START))

    # Verify in worktree
    if (cd "$WORK_DIR" && eval "$VERIFY_CMD" > /dev/null 2>&1); then
        echo "  PASS (${DURATION}s)"
        echo "PASS $TITLE ${DURATION}s" >> "$RESULTS_FILE"
        PASS=$((PASS + 1))
    else
        echo "  FAIL (${DURATION}s)"
        echo "FAIL $TITLE ${DURATION}s" >> "$RESULTS_FILE"
        FAIL=$((FAIL + 1))
    fi

    # Cleanup worktree
    git -C "$GIT_ROOT" worktree remove --force "$WORKTREE" 2>/dev/null || true
    git -C "$GIT_ROOT" branch -D "$BRANCH" 2>/dev/null || true
done

echo ""
echo "=== Baseline Results ==="
echo "Model: $MODEL (raw, no A²)"
echo "Score: $PASS/$TOTAL"
echo ""
cat "$RESULTS_FILE"
echo ""
echo "Compare against: cargo run -p a2ctl -- bench --model opencode"
