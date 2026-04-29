import asyncio
import os
import re
import time
import uuid
import multiprocessing as mp
from pathlib import Path

from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from dotenv import load_dotenv
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse

from .invoice_finder import find_invoice_combinations_for_targets

BASE_DIR = Path(__file__).resolve().parent.parent

load_dotenv(BASE_DIR / ".env")
UPLOAD_DIR = BASE_DIR / "uploads"
OUTPUT_DIR = BASE_DIR / "outputs"

DEFAULT_TOLERANCE = 100
DEFAULT_MAX_INVOICES = 5
MAX_UPLOAD_BYTES = 10 * 1024 * 1024
CLEANUP_TTL_SECONDS = int(os.getenv("CLEANUP_TTL_SECONDS", "3600"))
CLEANUP_INTERVAL_SECONDS = int(
    os.getenv("CLEANUP_INTERVAL_SECONDS", str(CLEANUP_TTL_SECONDS))
)
PROCESS_TIMEOUT_SECONDS = int(os.getenv("PROCESS_TIMEOUT_SECONDS", "600"))

FILENAME_RE = re.compile(r"^[A-Za-z0-9_.-]+$")

app = FastAPI(title="Cari Piutang API")

cors_origins = os.getenv("CORS_ORIGINS", "").strip()
if cors_origins:
    allowed_origins = [item.strip() for item in cors_origins.split(",") if item.strip()]
else:
    allowed_origins = ["https://ar.cisan.id", "http://ar.cisan.id"]

app.add_middleware(
    CORSMiddleware,
    allow_origins=allowed_origins,
    allow_credentials=False,
    allow_methods=["*"],
    allow_headers=["*"],
)

def _run_with_timeout(func, args, timeout_seconds):
    queue = mp.Queue()

    def _worker(q, f, f_args):
        try:
            q.put(f(*f_args))
        except Exception as exc:
            q.put(exc)

    proc = mp.Process(target=_worker, args=(queue, func, args))
    proc.start()
    proc.join(timeout_seconds)

    if proc.is_alive():
        proc.terminate()
        proc.join()
        raise TimeoutError("Processing timed out")

    result = queue.get()
    if isinstance(result, Exception):
        raise result
    return result


def _is_windows() -> bool:
    return os.name == "nt"


def _cleanup_old_files(base_dir: Path, ttl_seconds: int) -> None:
    if not base_dir.exists():
        return
    now = time.time()
    for entry in base_dir.iterdir():
        if not entry.is_file():
            continue
        try:
            age = now - entry.stat().st_mtime
            if age >= ttl_seconds:
                entry.unlink()
        except OSError:
            pass


def _sanitize_filename(name: str) -> str:
    name = os.path.basename(name)
    name = name.replace(" ", "_")
    return re.sub(r"[^A-Za-z0-9_.-]", "", name)


async def _periodic_cleanup_loop() -> None:
    while True:
        _cleanup_old_files(UPLOAD_DIR, CLEANUP_TTL_SECONDS)
        _cleanup_old_files(OUTPUT_DIR, CLEANUP_TTL_SECONDS)
        await asyncio.sleep(CLEANUP_INTERVAL_SECONDS)


async def _save_upload(upload: UploadFile, dest: Path) -> None:
    size = 0
    with dest.open("wb") as handle:
        while True:
            chunk = await upload.read(1024 * 1024)
            if not chunk:
                break
            size += len(chunk)
            if size > MAX_UPLOAD_BYTES:
                raise HTTPException(status_code=413, detail="File too large")
            handle.write(chunk)


def _parse_targets(raw_targets: str):
    if raw_targets is None:
        return []
    parts = [part.strip() for part in raw_targets.split(",")]
    targets = []
    for part in parts:
        digits = re.sub(r"\D", "", part)
        if not digits:
            continue
        value = int(digits)
        if value > 0:
            targets.append(value)
    return targets


@app.on_event("startup")
async def _startup() -> None:
    app.state.cleanup_task = asyncio.create_task(_periodic_cleanup_loop())


@app.on_event("shutdown")
async def _shutdown() -> None:
    task = getattr(app.state, "cleanup_task", None)
    if task:
        task.cancel()


@app.get("/api/health")
def health_check():
    return {"status": "ok"}


@app.post("/api/process")
async def process_file(
    file: UploadFile | None = File(None),
    upload_id: str | None = Form(None),
    targets: str = Form(...),
    tolerance: int = Form(DEFAULT_TOLERANCE),
    max_invoices: int = Form(DEFAULT_MAX_INVOICES),
):
    _cleanup_old_files(UPLOAD_DIR, CLEANUP_TTL_SECONDS)
    _cleanup_old_files(OUTPUT_DIR, CLEANUP_TTL_SECONDS)

    if file and (not file.filename or not file.filename.lower().endswith(".xlsx")):
        raise HTTPException(status_code=400, detail="Only .xlsx files are supported")

    target_values = _parse_targets(targets)
    if not target_values:
        raise HTTPException(
            status_code=400, detail="Target must be a positive number"
        )

    tolerance = max(0, tolerance)
    max_invoices = max(1, min(20, max_invoices))

    UPLOAD_DIR.mkdir(parents=True, exist_ok=True)
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    upload_path = None
    if upload_id:
        if not FILENAME_RE.match(upload_id):
            raise HTTPException(status_code=400, detail="Invalid upload id")
        upload_path = UPLOAD_DIR / upload_id
        if not upload_path.exists():
            raise HTTPException(status_code=404, detail="Upload not found")
    elif file:
        safe_name = _sanitize_filename(file.filename)
        if not safe_name:
            safe_name = "upload.xlsx"

        upload_path = UPLOAD_DIR / f"{uuid.uuid4().hex}_{safe_name}"
        try:
            await _save_upload(file, upload_path)
        except HTTPException:
            if upload_path.exists():
                upload_path.unlink(missing_ok=True)
            raise
        except Exception as exc:
            if upload_path.exists():
                upload_path.unlink(missing_ok=True)
            raise HTTPException(status_code=500, detail=str(exc)) from exc
    else:
        raise HTTPException(
            status_code=400, detail="File or upload id must be provided"
        )

    finder_args = (
        str(upload_path),
        target_values,
        tolerance,
        max_invoices,
        str(OUTPUT_DIR),
    )

    try:
        if _is_windows():
            # Windows + uvicorn reload + multiprocessing can fail with WinError 6
            output_file, total_rows = await asyncio.wait_for(
                asyncio.to_thread(
                    find_invoice_combinations_for_targets,
                    *finder_args,
                ),
                timeout=PROCESS_TIMEOUT_SECONDS,
            )
        else:
            output_file, total_rows = await asyncio.to_thread(
                _run_with_timeout,
                find_invoice_combinations_for_targets,
                finder_args,
                PROCESS_TIMEOUT_SECONDS,
            )
    except TimeoutError:
        raise HTTPException(
            status_code=504,
            detail="Proses terlalu lama. Coba kurangi jumlah invoice atau target.",
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except Exception as exc:
        raise HTTPException(status_code=500, detail="Proses gagal. Coba lagi.") from exc
    finally:
        if upload_path:
            upload_path.unlink(missing_ok=True)

    if not output_file:
        return {
            "found": False,
            "total_rows": 0,
            "download_url": None,
        }

    file_name = os.path.basename(output_file)
    return {
        "found": True,
        "total_rows": total_rows,
        "download_url": f"/api/download/{file_name}",
        "file_name": file_name,
    }


@app.get("/api/download/{file_name}")
def download_file(file_name: str):
    _cleanup_old_files(OUTPUT_DIR, CLEANUP_TTL_SECONDS)

    if not FILENAME_RE.match(file_name):
        raise HTTPException(status_code=400, detail="Invalid file name")

    file_path = OUTPUT_DIR / file_name
    if not file_path.exists():
        raise HTTPException(status_code=404, detail="File not found")

    return FileResponse(
        path=str(file_path),
        filename=file_name,
        media_type="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    )
@app.post("/api/upload")
async def upload_file(file: UploadFile = File(...)):
    _cleanup_old_files(UPLOAD_DIR, CLEANUP_TTL_SECONDS)

    if not file.filename or not file.filename.lower().endswith(".xlsx"):
        raise HTTPException(status_code=400, detail="Only .xlsx files are supported")

    UPLOAD_DIR.mkdir(parents=True, exist_ok=True)
    safe_name = _sanitize_filename(file.filename)
    if not safe_name:
        safe_name = "upload.xlsx"

    upload_id = f"{uuid.uuid4().hex}_{safe_name}"
    upload_path = UPLOAD_DIR / upload_id
    try:
        await _save_upload(file, upload_path)
    except HTTPException:
        if upload_path.exists():
            upload_path.unlink(missing_ok=True)
        raise
    except Exception as exc:
        if upload_path.exists():
            upload_path.unlink(missing_ok=True)
        raise HTTPException(status_code=500, detail=str(exc)) from exc

    return {"upload_id": upload_id, "file_name": safe_name}


@app.delete("/api/upload/{upload_id}")
async def delete_upload(upload_id: str):
    _cleanup_old_files(UPLOAD_DIR, CLEANUP_TTL_SECONDS)

    if not FILENAME_RE.match(upload_id):
        raise HTTPException(status_code=400, detail="Invalid upload id")

    upload_path = UPLOAD_DIR / upload_id
    if not upload_path.exists():
        return {"deleted": False}

    upload_path.unlink(missing_ok=True)
    return {"deleted": True}
