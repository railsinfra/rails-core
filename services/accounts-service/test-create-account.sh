#!/bin/bash

# Test script to create an account
# Make sure the API server is running on port 8080

API_URL="http://localhost:8080/api/v1/accounts"

# Create a checking account
curl -X POST "${API_URL}" \
  -H "Content-Type: application/json" \
  -d '{
    "account_number": "ACC-001-2024",
    "account_type": "checking",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "currency": "USD"
  }' \
  -w "\n\nHTTP Status: %{http_code}\n" \
  -v

echo -e "\n---\n"

# Create a saving account
curl -X POST "${API_URL}" \
  -H "Content-Type: application/json" \
  -d '{
    "account_number": "ACC-002-2024",
    "account_type": "saving",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "currency": "USD"
  }' \
  -w "\n\nHTTP Status: %{http_code}\n" \
  -v
