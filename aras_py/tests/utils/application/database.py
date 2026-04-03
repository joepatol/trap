import os
from pathlib import Path

from sqlalchemy import create_engine
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy.orm import sessionmaker

db_dir = Path(os.path.dirname(__file__)).parent.parent.parent.parent

SQLITE_DATABASE_URL = f"sqlite:///{db_dir}/note.db"

engine = create_engine(SQLITE_DATABASE_URL, connect_args={"check_same_thread": False})

SessionLocal = sessionmaker(autocommit=False, autoflush=False, bind=engine)

Base = declarative_base()

def get_db():
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()
