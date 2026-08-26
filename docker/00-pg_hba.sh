#!/bin/bash
# 00-pg_hba.sh
# Opt-in: adds passwordless (trust) rules for external TCP connections.
#
# Background:
# The official postgres:18 Docker image generates pg_hba.conf during initdb with
# a catch-all "host all all all scram-sha-256" rule for TCP connections.
# When pg_ripple's Docker image is used with port forwarding, the connection source
# IP may not match the localhost-only trust rules, causing authentication to fail.
#
# v0.128.1 (emergency containment): this script used to apply unconditionally,
# so every pg_ripple image — including production pulls — accepted passwordless
# remote PostgreSQL connections. It is now inert unless the operator explicitly
# opts in with PG_RIPPLE_DEV_TRUST_AUTH=1. NEVER set this in a production or
# otherwise network-reachable deployment — anyone who can reach the port gets
# an unauthenticated superuser connection.

set -e

if [ "$PG_RIPPLE_DEV_TRUST_AUTH" != "1" ]; then
  echo "INFO: PG_RIPPLE_DEV_TRUST_AUTH not set to 1, skipping pg_hba.conf trust rules (SCRAM auth stays in effect)"
  exit 0
fi

# PGDATA is set by the official postgres entrypoint
if [ -z "$PGDATA" ]; then
  echo "INFO: PGDATA not set, skipping pg_hba.conf modification"
  exit 0
fi

PG_HBA="$PGDATA/pg_hba.conf"

if [ ! -f "$PG_HBA" ]; then
  echo "WARNING: pg_hba.conf not found at $PG_HBA"
  exit 0
fi

# Check if we already have trust rules for external connections
if grep -q "0.0.0.0/0.*trust" "$PG_HBA" || grep -q "::/0.*trust" "$PG_HBA"; then
  echo "INFO: pg_hba.conf already has trust rules for external connections"
  exit 0
fi

# Find the line number of the catch-all scram-sha-256 rule and insert trust rules before it
if LINE=$(grep -n "^host all all all scram-sha-256" "$PG_HBA" | head -1 | cut -d: -f1); then
  if [ -n "$LINE" ]; then
    # Use sed to insert the new rules BEFORE the scram-sha-256 line
    # Use backslash-escaped newlines for proper sed syntax on both macOS and Linux
    sed -i "${LINE}i\\
host    all             all             0.0.0.0/0               trust\\
host    all             all             ::/0                    trust" "$PG_HBA"
    
    echo "✓ Modified pg_hba.conf to allow external TCP connections"
  fi
fi
