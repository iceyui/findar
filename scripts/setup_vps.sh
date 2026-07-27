#!/usr/bin/env bash
set -euo pipefail

DEFAULT_APP_DIR="/var/www/ar-bbn"
DEFAULT_DOMAIN="ar.vanila.id"
DEFAULT_API_DOMAIN="api-ar.vanila.id"
DEFAULT_SERVICE_NAME="ar-bbn-api"
DEFAULT_SUPABASE_URL=""
DEFAULT_SUPABASE_PUBLISHABLE_KEY=""

read -r -p "Masukkan path project [${DEFAULT_APP_DIR}]: " APP_DIR
APP_DIR="${APP_DIR:-$DEFAULT_APP_DIR}"
read -r -p "Domain FE [${DEFAULT_DOMAIN}]: " DOMAIN
DOMAIN="${DOMAIN:-$DEFAULT_DOMAIN}"
read -r -p "Domain API [${DEFAULT_API_DOMAIN}]: " API_DOMAIN
API_DOMAIN="${API_DOMAIN:-$DEFAULT_API_DOMAIN}"
read -r -p "Nama service systemd [${DEFAULT_SERVICE_NAME}]: " SERVICE_NAME
SERVICE_NAME="${SERVICE_NAME:-$DEFAULT_SERVICE_NAME}"
read -r -p "Supabase URL [${DEFAULT_SUPABASE_URL}]: " SUPABASE_URL
SUPABASE_URL="${SUPABASE_URL:-$DEFAULT_SUPABASE_URL}"
read -r -p "Supabase publishable/anon key [${DEFAULT_SUPABASE_PUBLISHABLE_KEY}]: " SUPABASE_PUBLISHABLE_KEY
SUPABASE_PUBLISHABLE_KEY="${SUPABASE_PUBLISHABLE_KEY:-$DEFAULT_SUPABASE_PUBLISHABLE_KEY}"

if [[ $EUID -ne 0 ]]; then
  echo "Please run as root (use sudo)." >&2
  exit 1
fi

if [[ ! -d "$APP_DIR" ]]; then
  echo "App directory not found: $APP_DIR" >&2
  exit 1
fi

apt update
apt install -y python3-venv python3-pip nginx

# Backend venv + deps
cd "$APP_DIR"
python3 -m venv venv
source venv/bin/activate
pip install -r backend/requirements.txt

# Ensure .env exists
if [[ ! -f backend/.env ]]; then
  cp backend/.env.example backend/.env
fi
if [[ -n "$SUPABASE_URL" ]]; then
  grep -q '^SUPABASE_URL=' backend/.env \
    && sed -i "s|^SUPABASE_URL=.*|SUPABASE_URL=${SUPABASE_URL}|" backend/.env \
    || echo "SUPABASE_URL=${SUPABASE_URL}" >> backend/.env
fi
if [[ -n "$SUPABASE_PUBLISHABLE_KEY" ]]; then
  grep -q '^SUPABASE_PUBLISHABLE_KEY=' backend/.env \
    && sed -i "s|^SUPABASE_PUBLISHABLE_KEY=.*|SUPABASE_PUBLISHABLE_KEY=${SUPABASE_PUBLISHABLE_KEY}|" backend/.env \
    || echo "SUPABASE_PUBLISHABLE_KEY=${SUPABASE_PUBLISHABLE_KEY}" >> backend/.env
fi
PORT_VALUE=$(grep -E '^PORT=' backend/.env | tail -n 1 | cut -d '=' -f2 | tr -d '\r')
if [[ -z "$PORT_VALUE" ]]; then
  PORT_VALUE="9001"
fi

# Frontend build
cd "$APP_DIR/frontend"
npm install
if [[ ! -f .env ]]; then
  echo "VITE_API_BASE=https://$API_DOMAIN" > .env
  echo "VITE_SUPABASE_URL=$SUPABASE_URL" >> .env
  echo "VITE_SUPABASE_PUBLISHABLE_KEY=$SUPABASE_PUBLISHABLE_KEY" >> .env
fi
npm run build

# Ensure runtime dirs exist with correct permissions
mkdir -p "$APP_DIR/backend/uploads" "$APP_DIR/backend/outputs"
chown -R www-data:www-data "$APP_DIR/backend/uploads" "$APP_DIR/backend/outputs"
chmod -R 775 "$APP_DIR/backend/uploads" "$APP_DIR/backend/outputs"

# systemd service
cat > "/etc/systemd/system/${SERVICE_NAME}.service" <<EOF
[Unit]
Description=AR Vanila Matcher API
After=network.target

[Service]
User=www-data
WorkingDirectory=${APP_DIR}
EnvironmentFile=${APP_DIR}/backend/.env
ExecStart=${APP_DIR}/venv/bin/uvicorn backend.app.main:app --host 127.0.0.1 --port \${PORT}
Restart=always

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now "$SERVICE_NAME"

# nginx config
cat > "/etc/nginx/sites-available/${SERVICE_NAME}" <<EOF
server {
    listen 80;
    server_name ${DOMAIN};

    root ${APP_DIR}/frontend/dist;
    index index.html;

    location / {
        try_files \$uri /index.html;
    }

    location /api/ {
        proxy_read_timeout 300s;
        proxy_connect_timeout 60s;
        proxy_send_timeout 300s;
        proxy_pass http://127.0.0.1:${PORT_VALUE};
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
EOF

cat > "/etc/nginx/sites-available/${SERVICE_NAME}-api" <<EOF
server {
    listen 80;
    server_name ${API_DOMAIN};

    location / {
        proxy_read_timeout 300s;
        proxy_connect_timeout 60s;
        proxy_send_timeout 300s;
        proxy_pass http://127.0.0.1:${PORT_VALUE};
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
EOF

ln -sf "/etc/nginx/sites-available/${SERVICE_NAME}" "/etc/nginx/sites-enabled/${SERVICE_NAME}"
ln -sf "/etc/nginx/sites-available/${SERVICE_NAME}-api" "/etc/nginx/sites-enabled/${SERVICE_NAME}-api"
nginx -t
systemctl reload nginx

echo "Setup complete."
