import os
import tempfile
from fastapi import FastAPI, UploadFile, File, BackgroundTasks
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import List
from fastapi.middleware.httpsredirect import HTTPSRedirectMiddleware

from services import db, stats

app = FastAPI(title="ProdzCLI Web App Backend")

# Allow requests from Next.js frontend and nginx proxy
origins = [
    "http://localhost:3000",
    "https://localhost:3000",
    "http://localhost",
    "https://localhost",
    "http://nginx",
    "https://nginx",
]

app.add_middleware(
    CORSMiddleware,
    allow_origins=origins,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Redirect to HTTPS if SSL is enabled
# app.add_middleware(HTTPSRedirectMiddleware)


@app.on_event("startup")
def startup_event():
    # Ensure DB is initialized
    db.init_db()

# Health check endpoint
@app.get("/health")
def health_check():
    return {"status": "healthy", "service": "backend", "timestamp": __import__("datetime").datetime.now().isoformat()}


class LogSessionRequest(BaseModel):
    activity: str
    duration_minutes: float


@app.post("/api/sessions/log")
def log_session(request: LogSessionRequest):
    result = db.log_session(request.activity, request.duration_minutes)
    if result["status"] == "error":
        from fastapi import HTTPException
        raise HTTPException(status_code=500, detail=result["message"])
    return {"message": "Session logged successfully", "data": result}


class StatEntry(BaseModel):
    date: str
    activity: str
    duration_minutes: float


@app.get("/api/stats", response_model=List[StatEntry])
def get_stats():
    data = stats.get_stats_data()
    return data


@app.post("/api/database/merge")
async def merge_database(file: UploadFile = File(...)):
    if not file.filename.endswith('.db'):
        from fastapi import HTTPException
        raise HTTPException(status_code=400, detail="Only .db files are allowed")

    # Save uploaded file to a temporary file
    temp_file = tempfile.NamedTemporaryFile(delete=False, suffix=".db")
    try:
        content = await file.read()
        temp_file.write(content)
        temp_file.close()

        # Merge the temp database file
        result = db.merge_db(temp_file.name)
        if result["status"] == "error":
            from fastapi import HTTPException
            raise HTTPException(status_code=500, detail=result.get("message", "Unknown error"))
        
        return {"message": f"Successfully merged {result['inserted_count']} records."}
    finally:
        # Cleanup temp file
        os.unlink(temp_file.name)
