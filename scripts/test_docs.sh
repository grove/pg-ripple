#!/usr/bin/env bash
# scripts/test_docs.sh — CI harness for documentation code examples
#
# Spins up pg_ripple via Docker (or uses an existing connection),
# extracts fenced SQL blocks from docs/src/, executes them in document
# order, and compares stdout against expected-output comment blocks.
#
# Usage:
#   bash scripts/test_docs.sh                  # use Docker
#   PG_CONNSTR="host=localhost dbname=test" bash scripts/test_docs.sh  # use existing DB
#
# Expected output format in markdown:
#   ```sql
#   SELECT pg_ripple.triple_count();
#   ```
#   <!-- expected
#    triple_count
#   --------------
#              0
#   (1 row)
#   -->
#
# If no <!-- expected ... --> block follows a SQL block, the block is
# executed but its output is not checked (fire-and-forget).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DOCS_DIR="$PROJECT_DIR/docs/src"
FIXTURES_DIR="$PROJECT_DIR/docs/fixtures"
CONTAINER_NAME="pg_ripple_docs_test"
FAILURES=0
TESTS=0
SKIPPED=0

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

cleanup() {
    if [[ -z "${PG_CONNSTR:-}" && "$(docker ps -q -f name=$CONTAINER_NAME 2>/dev/null)" ]]; then
        echo "Stopping test container..."
        docker stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
        docker rm "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

# Start database
if [[ -z "${PG_CONNSTR:-}" ]]; then
    echo "Starting pg_ripple test container..."
    if docker ps -q -f name=$CONTAINER_NAME 2>/dev/null | grep -q .; then
        docker stop "$CONTAINER_NAME" >/dev/null 2>&1
        docker rm "$CONTAINER_NAME" >/dev/null 2>&1
    fi

    docker run -d --name "$CONTAINER_NAME" \
        -e POSTGRES_PASSWORD=test \
        -e POSTGRES_DB=docs_test \
        -p 15432:5432 \
        pg_ripple:latest >/dev/null 2>&1 || {
            echo -e "${YELLOW}Docker image pg_ripple:latest not found. Trying cargo pgrx...${NC}"
            # Fall back to using a running pgrx instance
            PG_CONNSTR="host=localhost port=28818 dbname=pg_ripple_test user=$(whoami)"
        }

    if [[ -z "${PG_CONNSTR:-}" ]]; then
        PG_CONNSTR="host=localhost port=15432 dbname=docs_test user=postgres password=test"
        echo "Waiting for PostgreSQL to be ready..."
        for i in $(seq 1 30); do
            if psql "$PG_CONNSTR" -c "SELECT 1" >/dev/null 2>&1; then
                break
            fi
            sleep 1
        done
    fi
fi

run_sql() {
    psql "$PG_CONNSTR" -X --no-psqlrc -q 2>&1 <<< "$1"
}

# Load fixtures
echo "Loading fixtures..."
if [[ -f "$FIXTURES_DIR/bibliography.sql" ]]; then
    run_sql "$(cat "$FIXTURES_DIR/bibliography.sql")" >/dev/null 2>&1 || {
        echo -e "${YELLOW}Warning: fixture load had errors (extension may not be installed)${NC}"
    }
fi

# Extract and run SQL blocks from markdown files
process_file() {
    local file="$1"
    local relative_path="${file#$DOCS_DIR/}"
    local in_sql_block=false
    local sql_block=""
    local line_num=0
    local block_start=0
    local expecting_output=false
    local expected_output=""
    local in_expected=false
    local blocks_in_file=0

    while IFS= read -r line || [[ -n "$line" ]]; do
        ((line_num++))

        # Check for expected output block
        if [[ "$expecting_output" == true ]]; then
            if [[ "$line" =~ ^'<!-- expected' ]]; then
                in_expected=true
                expected_output=""
                continue
            elif [[ "$in_expected" == true ]]; then
                if [[ "$line" == "-->" ]]; then
                    in_expected=false
                    expecting_output=false

                    # Run the SQL and compare output
                    ((TESTS++))
                    actual_output=$(run_sql "$sql_block" 2>&1 || true)
                    actual_trimmed=$(echo "$actual_output" | sed 's/[[:space:]]*$//' | sed '/^$/d')
                    expected_trimmed=$(echo "$expected_output" | sed 's/[[:space:]]*$//' | sed '/^$/d')

                    if [[ "$actual_trimmed" == "$expected_trimmed" ]]; then
                        echo -e "  ${GREEN}PASS${NC} $relative_path:$block_start"
                    else
                        echo -e "  ${RED}FAIL${NC} $relative_path:$block_start"
                        echo "    Expected:"
                        echo "$expected_output" | head -5 | sed 's/^/      /'
                        echo "    Actual:"
                        echo "$actual_output" | head -5 | sed 's/^/      /'
                        ((FAILURES++))
                    fi
                    sql_block=""
                    continue
                else
                    expected_output+="$line"$'\n'
                    continue
                fi
            else
                # No expected block found, just execute without checking
                if [[ -n "$sql_block" ]]; then
                    run_sql "$sql_block" >/dev/null 2>&1 || true
                    ((SKIPPED++))
                fi
                expecting_output=false
                sql_block=""
            fi
        fi

        # Check for SQL code block markers
        if [[ "$line" =~ ^\`\`\`sql ]]; then
            in_sql_block=true
            sql_block=""
            block_start=$line_num
            ((blocks_in_file++))
            continue
        fi

        if [[ "$in_sql_block" == true && "$line" == '```' ]]; then
            in_sql_block=false
            expecting_output=true
            continue
        fi

        if [[ "$in_sql_block" == true ]]; then
            sql_block+="$line"$'\n'
        fi
    done < "$file"

    # Handle trailing SQL block with no expected output
    if [[ "$expecting_output" == true && -n "$sql_block" ]]; then
        run_sql "$sql_block" >/dev/null 2>&1 || true
        ((SKIPPED++))
    fi

    if [[ $blocks_in_file -gt 0 ]]; then
        echo "  Processed $blocks_in_file SQL blocks in $relative_path"
    fi
}

echo ""
echo "Running documentation SQL tests..."
echo "==================================="

# Process files in SUMMARY.md order
while IFS= read -r md_file; do
    if [[ -f "$DOCS_DIR/$md_file" ]]; then
        process_file "$DOCS_DIR/$md_file"
    fi
done < <(grep -oP '\(([^)]+\.md)\)' "$DOCS_DIR/SUMMARY.md" | tr -d '()' | sort -u)

# Also process any .md files not in SUMMARY.md
while IFS= read -r md_file; do
    relative="${md_file#$DOCS_DIR/}"
    if ! grep -q "$relative" "$DOCS_DIR/SUMMARY.md" 2>/dev/null; then
        process_file "$md_file"
    fi
done < <(find "$DOCS_DIR" -name "*.md" -type f | sort)

echo ""
echo "==================================="
echo -e "Tests: $TESTS  Passed: $((TESTS - FAILURES))  Failed: $FAILURES  Executed-only: $SKIPPED"

if [[ $FAILURES -gt 0 ]]; then
    echo -e "${RED}FAILED${NC}: $FAILURES test(s) failed"
    exit 1
else
    echo -e "${GREEN}PASSED${NC}: All documentation tests passed"
    exit 0
fi
