from fastapi_utils.guid_type import GUID, GUID_DEFAULT_SQLITE
from sqlalchemy import TIMESTAMP, Boolean, Column, String
from sqlalchemy.sql import func

from .database import Base


class Note(Base):
    __tablename__ = "notes"

    id = Column(GUID, primary_key=True, default=GUID_DEFAULT_SQLITE)  # type: ignore
    title = Column(String, nullable=False)
    content = Column(String, nullable=False)
    category = Column(String, nullable=True)
    published = Column(Boolean, nullable=False, default=True)
    createdAt = Column(
        TIMESTAMP(timezone=True), nullable=False, server_default=func.now()
    )
    updatedAt = Column(TIMESTAMP(timezone=True), default=None, onupdate=func.now())
