#!/bin/sh
set -eu

# Poll for http://localhost:3000/api/healthz to be ready
echo "Waiting for Forgejo API to be ready..."
max_attempts=60
attempt=0
while [ $attempt -lt $max_attempts ]; do
  if curl --silent -f -o /dev/null http://localhost:3000/api/healthz; then
    echo "Forgejo API is ready"
    break
  fi
  attempt=$((attempt + 1))
  if [ $attempt -lt $max_attempts ]; then
    sleep 1
  fi
done

if [ $attempt -eq $max_attempts ]; then
  echo "Timeout waiting for Forgejo API to be ready"
  exit 1
fi

# Create admin user
su git -s /bin/sh -c 'forgejo admin user create \
  --username admin2 \
  --password secret \
  --email admin2@forgejo.localhost \
  --admin \
  --must-change-password=false'

# Generate API token
su git -s /bin/sh -c 'forgejo admin user generate-access-token \
  --username admin2 \
  --token-name jj-api \
  --scopes all \
  ' > /data/api-key

chmod 600 /data/api-key

echo "Forgejo initialization complete"
