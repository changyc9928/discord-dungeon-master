#!/usr/bin/env bash

# Dump PostgreSQL databases as INSERT statements
#
# Usage:
#   ./dump-test-dbs.sh <service_name> [postgres_user]
#
# Example:
#   ./dump-test-dbs.sh db postgres
#
# Output:
#   ./dumps/<db_name>.sql

set -euo pipefail

CONTAINER_NAME="${1:-}"
POSTGRES_USER="${2:-postgres}"
OUTPUT_DIR="./dumps"

if [[ -z "$CONTAINER_NAME" ]]; then
  echo "Usage: $0 <service_name> [postgres_user]"
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

echo "Ensuring postgres service is up..."

docker compose up -d "$CONTAINER_NAME"

echo "Waiting for PostgreSQL to become ready..."

until docker compose exec -T "$CONTAINER_NAME" \
  pg_isready -U "$POSTGRES_USER" >/dev/null 2>&1; do
  sleep 2
done

echo "PostgreSQL is ready."

# Get only test databases
DBS=$(docker compose exec -T "$CONTAINER_NAME" \
  psql -U "$POSTGRES_USER" -d postgres -Atc "
SELECT datname
FROM pg_database
WHERE datistemplate = false
  AND datname NOT IN ('postgres')
  AND datname LIKE 'test%';
")

if [[ -z "$DBS" ]]; then
  echo "No test databases found."
  exit 0
fi

echo "Databases to dump:"
echo "$DBS"
echo

for DB in $DBS; do
  echo "Dumping database: $DB"

  docker compose exec -T "$CONTAINER_NAME" \
    pg_dump \
      -U "$POSTGRES_USER" \
      --data-only \
      --inserts \
      --column-inserts \
      "$DB" \
      > "$OUTPUT_DIR/${DB}.sql"

  echo "Saved to: $OUTPUT_DIR/${DB}.sql"
done

echo
echo "All test databases dumped."