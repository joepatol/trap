import logging

from fastapi import FastAPI

app = FastAPI(debug=True)
server_logger = logging.getLogger("aras")
server_logger.setLevel(logging.ERROR)
server_logger.addHandler(logging.StreamHandler())


@app.get("/health_check")
async def health_check():
    return {"status": "ok"}
