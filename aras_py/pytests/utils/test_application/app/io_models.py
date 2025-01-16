from datetime import datetime
from typing import List
from pydantic import BaseModel

class NoteBaseSchema(BaseModel):
    id: str | None = None
    title: str
    content: str
    category: str | None = None
    published: bool = False
    createdAt: datetime | None = None
    updatedAt: datetime | None = None
    
    class Config:
        from_attributes = True
        populate_by_name = True
        arbitrary_types_allowed = True
        
        
class ListNoteResponse(BaseModel):
    status: str
    results: int
    notes: List[NoteBaseSchema]
