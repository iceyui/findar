# AR Vanila Matcher

Versi web untuk mencari kombinasi piutang toko dari file Excel berdasarkan nominal pembayaran dan toleransi.

## Struktur
- `backend/` FastAPI untuk upload, proses, dan download hasil
- `frontend/` React (Vite) untuk UI upload & hasil

## Backend (FastAPI)
1) Buat virtual env dan install dependencies:
```
python -m venv venv
venv\\Scripts\\activate
pip install -r backend\\requirements.txt
```

2) Jalankan API:
```
copy backend\\.env.example backend\\.env
python backend\\run_dev.py
```

3) Isi konfigurasi Supabase Auth di `backend\\.env`:
```
SUPABASE_URL=https://your-project-ref.supabase.co
SUPABASE_PUBLISHABLE_KEY=your-publishable-or-anon-key
ALLOWED_AUTH_EMAILS=admin@vanila.id
```

## Frontend (React + Vite)
1) Install dependencies:
```
cd frontend
npm install
```

2) Set API base (opsional):
```
copy .env.example .env
```

3) Isi konfigurasi Supabase Auth di `frontend\\.env`:
```
VITE_SUPABASE_URL=https://your-project-ref.supabase.co
VITE_SUPABASE_PUBLISHABLE_KEY=your-publishable-or-anon-key
```

4) Run dev server:
```
npm run dev
```

## API
- Semua endpoint proses/download membutuhkan `Authorization: Bearer <supabase_access_token>`
- `POST /api/process` upload file + target(s) + tolerance
- `POST /api/upload` upload file, return `upload_id`
- `DELETE /api/upload/{upload_id}` delete uploaded file
- `GET /api/download/{file_name}` download hasil

## Catatan
- Batas upload default 10MB
- Cleanup otomatis file sementara tiap 1 jam (bisa diubah lewat `CLEANUP_TTL_SECONDS`)
- Interval cleanup bisa diubah lewat `CLEANUP_INTERVAL_SECONDS`
- Tema UI: vanilla/warm untuk branding vanila.id

## Deploy via Coolify

Gunakan 1 Project Coolify berisi 2 Application/resource terpisah:

### Backend API
- Root directory: `backend`
- Build pack: `Dockerfile`
- Dockerfile: `Dockerfile`
- Port / exposed port: `9001`
- Domain: `api-ar.vanila.id`
- Health check path: `/api/health`
- Environment variables:
```
HOST=0.0.0.0
PORT=9001
CORS_ORIGINS=https://ar.vanila.id
SUPABASE_URL=https://your-project-ref.supabase.co
SUPABASE_PUBLISHABLE_KEY=your-publishable-or-anon-key
ALLOWED_AUTH_EMAILS=admin@vanila.id
```

### Frontend Web
- Root directory: `frontend`
- Build pack: `Dockerfile`
- Dockerfile: `Dockerfile`
- Port / exposed port: `80`
- Domain: `ar.vanila.id`
- Environment variables:
```
VITE_API_BASE=https://api-ar.vanila.id
VITE_SUPABASE_URL=https://your-project-ref.supabase.co
VITE_SUPABASE_PUBLISHABLE_KEY=your-publishable-or-anon-key
```

Catatan: variable `VITE_*` harus tersedia saat build frontend karena Vite memasukkannya ke bundle static. Di Coolify, tandai variable frontend tersebut sebagai build/build-time variable jika opsi itu tersedia.

## Deploy di VPS Ubuntu (contoh Nginx + systemd)

### 1) Persiapan server
```
sudo apt update
sudo apt install -y python3-venv python3-pip nginx
```

### Automasi (opsional)
Lihat `deploy/DEPLOY.md` untuk script setup dan deploy otomatis.

### 2) Upload project ke server
Contoh lokasi: `/var/www/ar-bbn`

### 3) Backend (FastAPI) sebagai service
```
cd /var/www/ar-bbn
python3 -m venv venv
source venv/bin/activate
pip install -r backend/requirements.txt
```

Buat file service:
```
sudo nano /etc/systemd/system/ar-bbn-api.service
```
Isi:
```
[Unit]
Description=AR Vanila Matcher API
After=network.target

[Service]
User=www-data
WorkingDirectory=/var/www/ar-bbn
EnvironmentFile=/var/www/ar-bbn/backend/.env
ExecStart=/var/www/ar-bbn/venv/bin/uvicorn backend.app.main:app --host 127.0.0.1 --port ${PORT}
Restart=always

[Install]
WantedBy=multi-user.target
```

Aktifkan service:
```
sudo systemctl daemon-reload
sudo systemctl enable --now ar-bbn-api
sudo systemctl status ar-bbn-api
```

### 4) Build frontend
```
cd /var/www/ar-bbn/frontend
npm install
echo "VITE_API_BASE=https://api-ar.vanila.id" > .env
echo "VITE_SUPABASE_URL=https://your-project-ref.supabase.co" >> .env
echo "VITE_SUPABASE_PUBLISHABLE_KEY=your-publishable-or-anon-key" >> .env
npm run build
```

### 5) Nginx config
```
sudo nano /etc/nginx/sites-available/ar-bbn
```
Isi:
```
server {
    listen 80;
    server_name ar.vanila.id;

    root /var/www/ar-bbn/frontend/dist;
    index index.html;

    location / {
        try_files $uri /index.html;
    }

    location /api/ {
        proxy_pass http://127.0.0.1:9001;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Tambahkan server block untuk API subdomain:
```
server {
    listen 80;
    server_name api-ar.vanila.id;

    location / {
        proxy_pass http://127.0.0.1:9001;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Aktifkan Nginx:
```
sudo ln -s /etc/nginx/sites-available/ar-bbn /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### 6) (Opsional) HTTPS dengan Let's Encrypt
```
sudo apt install -y certbot python3-certbot-nginx
sudo certbot --nginx -d ar.vanila.id -d api-ar.vanila.id
```
