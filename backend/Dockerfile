FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1
ENV PORT=9001

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . ./backend

RUN mkdir -p /app/backend/uploads /app/backend/outputs \
    && useradd --system --uid 10001 --create-home appuser \
    && chown -R appuser:appuser /app

EXPOSE 9001

USER appuser

CMD ["sh", "-c", "uvicorn backend.app.main:app --host 0.0.0.0 --port ${PORT}"]
