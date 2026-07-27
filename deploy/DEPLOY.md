# Deployment automation

## Setup (first time, run as root on VPS)
```
sudo bash scripts/setup_vps.sh
```

## Deploy updates (run on VPS)
```
sudo bash scripts/deploy_vps.sh
```

## What it does
- Setup installs python venv + nginx, builds frontend, creates systemd service
- Deploy pulls latest code, rebuilds frontend, restarts API service
- Nginx timeouts are increased for long processing (300s)
- Upload/output dirs are created with www-data ownership

## Notes
- Service name: ar-bbn-api
- App path: /var/www/ar-bbn
- Domain (FE): ar.vanila.id
- Domain (API): api-ar.vanila.id
- Auth: isi `SUPABASE_URL` dan `SUPABASE_PUBLISHABLE_KEY` di `backend/.env`
- Frontend auth: isi `VITE_SUPABASE_URL` dan `VITE_SUPABASE_PUBLISHABLE_KEY` di `frontend/.env`

## Systemd template
Use the template at `deploy/systemd/ar-bbn-api.service` to ensure
`uploads/` and `outputs/` are created on service start (prevents 500s
when dirs are missing or permissions are wrong).
