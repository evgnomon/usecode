#!/usr/bin/env bash
set -euo pipefail

PG="./pg"

# Create the resources table
$PG db add z

$PG tab add resources

# Normalized columns - frequently queried scalar fields
$PG col add resources api_version string
$PG col add resources kind string
$PG col add resources name string
$PG col add resources namespace string --nullable
$PG col add resources tenant string --nullable

# JSONB columns - arbitrary key/value pairs and deeply nested structures
$PG col add resources labels jsonb --nullable
$PG col add resources annotations jsonb --nullable
$PG col add resources spec jsonb --nullable

# Indexes
$PG idx add resources api_version kind name namespace tenant --unique
$PG idx add resources kind
$PG idx add resources labels
$PG idx add resources annotations
