# syntax=docker/dockerfile:1

FROM node:22-alpine AS build

WORKDIR /app

ARG VITE_API_BASE
ARG VITE_SUPABASE_URL
ARG VITE_SUPABASE_PUBLISHABLE_KEY
ARG VITE_TURNSTILE_SITE_KEY

ENV VITE_API_BASE=$VITE_API_BASE \
    VITE_SUPABASE_URL=$VITE_SUPABASE_URL \
    VITE_SUPABASE_PUBLISHABLE_KEY=$VITE_SUPABASE_PUBLISHABLE_KEY \
    VITE_TURNSTILE_SITE_KEY=$VITE_TURNSTILE_SITE_KEY

COPY package*.json ./
RUN npm ci --no-audit --no-fund

COPY . .
RUN npm run build

FROM nginxinc/nginx-unprivileged:1.27-alpine

LABEL org.opencontainers.image.title="ar-vanila-web" \
      org.opencontainers.image.description="AR Vanila Matcher frontend (React/Vite via Nginx)" \
      org.opencontainers.image.source="https://github.com/vanila-id/findar"

COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY security-headers.conf /etc/nginx/conf.d/security-headers.conf
COPY --from=build /app/dist /usr/share/nginx/html

EXPOSE 8080
