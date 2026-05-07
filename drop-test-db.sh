#!/usr/bin/env bash

# Drop all non-system PostgreSQL databases inside a Docker container
#
# Usage:
#   ./drop-test-db.sh <container_name> [postgres_user]
#
# Example:
#   ./drop-test-db.sh db postgres

set -euo pipefail

CONTAINER_NAME="${1:-}"
POSTGRES_USER="${2:-postgres}"

if [[ -z "$CONTAINER_NAME" ]]; then
  echo "Usage: $0 <container_name> [postgres_user]"
  exit 1
fi

echo "Ensuring postgres service is up..."

docker compose up -d "$CONTAINER_NAME"

echo "Waiting for PostgreSQL to become ready..."

until docker compose exec -T "$CONTAINER_NAME" \
  pg_isready -U "$POSTGRES_USER" >/dev/null 2>&1; do
  sleep 2
done

echo "PostgreSQL is ready."

echo "Fetching databases from container: $CONTAINER_NAME"

# Get only user test databases
DBS=$(docker compose exec -T "$CONTAINER_NAME" \
  psql -U "$POSTGRES_USER" -d postgres -Atc "
SELECT datname
FROM pg_database
WHERE datistemplate = false
  AND datname NOT IN ('postgres')
  AND datname LIKE 'test%';
")

if [[ -z "$DBS" ]]; then
  echo "No user databases found."
  exit 0
fi

echo "Databases to drop:"
echo "$DBS"
echo

for DB in $DBS; do
  echo "Dropping database: $DB"

  # Kill active connections
  docker compose exec -i "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d postgres -c "
    SELECT pg_terminate_backend(pid)
    FROM pg_stat_activity
    WHERE datname = '$DB'
      AND pid <> pg_backend_pid();
  " >/dev/null

  # Drop DB
  docker compose exec -i "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d postgres -c "
    DROP DATABASE IF EXISTS \"$DB\";
  "
done

echo
echo "All user databases dropped."