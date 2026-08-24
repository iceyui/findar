import os

from dotenv import load_dotenv
import uvicorn

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
ENV_PATH = os.path.join(BASE_DIR, ".env")
load_dotenv(ENV_PATH)

host = os.getenv("HOST", "0.0.0.0")
port = os.getenv("PORT", "9001")

try:
    port_int = int(port)
except ValueError:
    raise SystemExit(f"Invalid PORT value: {port}")

if __name__ == "__main__":
    uvicorn.run("app.main:app", host=host, port=port_int, reload=True)
