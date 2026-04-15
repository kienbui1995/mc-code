#!/bin/bash
set -e
WS="${1:?Usage: setup.sh <workspace_dir>}"
rm -rf "$WS" && mkdir -p "$WS"/{app,tests,.magic-code}

cat > "$WS/requirements.txt" << 'EOF'
fastapi==0.115.0
uvicorn==0.30.0
pydantic==2.9.0
EOF

cat > "$WS/CLAUDE.md" << 'EOF'
# todo-api (Python/FastAPI)
## Conventions
- Type hints on all functions
- Pydantic models for request/response
- Tests use pytest with fixtures
- No global mutable state — use dependency injection
- Docstrings on all public functions (Google style)
## Architecture: app/models.py (data) → app/store.py (logic) → app/main.py (API)
EOF

cat > "$WS/.magic-code/instructions.md" << 'EOF'
- Run pytest after editing Python files
- Use type hints everywhere
- Prefer edit_file over write_file for existing files
- Read a file before editing it
EOF

cat > "$WS/app/__init__.py" << 'EOF'
EOF

cat > "$WS/app/models.py" << 'EOF'
from enum import Enum
from pydantic import BaseModel
from typing import Optional
from datetime import datetime

class Priority(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"

class TodoCreate(BaseModel):
    title: str
    priority: Priority = Priority.MEDIUM
    tags: list[str] = []

class TodoResponse(BaseModel):
    id: int
    title: str
    completed: bool
    priority: Priority
    tags: list[str]
    created_at: datetime

class Todo:
    _next_id: int = 1

    def __init__(self, title: str, priority: Priority = Priority.MEDIUM):
        self.id = Todo._next_id
        Todo._next_id += 1
        self.title = title
        self.completed = False
        self.priority = priority
        self.tags: list[str] = []
        self.created_at = datetime.now()

    def toggle(self) -> None:
        self.completed = not self.completed

    def add_tag(self, tag: str) -> None:
        if tag not in self.tags:
            self.tags.append(tag)

    def to_response(self) -> TodoResponse:
        return TodoResponse(
            id=self.id, title=self.title, completed=self.completed,
            priority=self.priority, tags=self.tags, created_at=self.created_at,
        )
EOF

cat > "$WS/app/store.py" << 'EOF'
from app.models import Todo, Priority
from typing import Optional

class TodoStore:
    def __init__(self) -> None:
        self._todos: dict[int, Todo] = {}

    def add(self, title: str, priority: Priority = Priority.MEDIUM) -> Todo:
        todo = Todo(title, priority)
        self._todos[todo.id] = todo
        return todo

    def get(self, todo_id: int) -> Optional[Todo]:
        return self._todos.get(todo_id)

    def list_all(self) -> list[Todo]:
        return sorted(self._todos.values(), key=lambda t: t.id)

    def delete(self, todo_id: int) -> bool:
        return self._todos.pop(todo_id, None) is not None

    def search(self, query: str) -> list[Todo]:
        q = query.lower()
        return [t for t in self._todos.values() if q in t.title.lower()]

    def by_priority(self, priority: Priority) -> list[Todo]:
        return [t for t in self._todos.values() if t.priority == priority]

    def stats(self) -> dict[str, int]:
        total = len(self._todos)
        done = sum(1 for t in self._todos.values() if t.completed)
        return {"total": total, "completed": done, "pending": total - done}
EOF

cat > "$WS/app/main.py" << 'EOF'
from fastapi import FastAPI, HTTPException
from app.models import TodoCreate, TodoResponse, Priority
from app.store import TodoStore

app = FastAPI(title="Todo API")
store = TodoStore()

@app.post("/todos", response_model=TodoResponse)
def create_todo(todo: TodoCreate) -> TodoResponse:
    t = store.add(todo.title, todo.priority)
    for tag in todo.tags:
        t.add_tag(tag)
    return t.to_response()

@app.get("/todos", response_model=list[TodoResponse])
def list_todos() -> list[TodoResponse]:
    return [t.to_response() for t in store.list_all()]

@app.get("/todos/{todo_id}", response_model=TodoResponse)
def get_todo(todo_id: int) -> TodoResponse:
    t = store.get(todo_id)
    if not t:
        raise HTTPException(status_code=404, detail="Todo not found")
    return t.to_response()

@app.delete("/todos/{todo_id}")
def delete_todo(todo_id: int) -> dict:
    if not store.delete(todo_id):
        raise HTTPException(status_code=404, detail="Todo not found")
    return {"deleted": True}

@app.patch("/todos/{todo_id}/toggle", response_model=TodoResponse)
def toggle_todo(todo_id: int) -> TodoResponse:
    t = store.get(todo_id)
    if not t:
        raise HTTPException(status_code=404, detail="Todo not found")
    t.toggle()
    return t.to_response()
EOF

cat > "$WS/tests/__init__.py" << 'EOF'
EOF

cat > "$WS/tests/test_store.py" << 'EOF'
import pytest
from app.store import TodoStore
from app.models import Priority

@pytest.fixture
def store() -> TodoStore:
    return TodoStore()

def test_add_and_get(store: TodoStore) -> None:
    todo = store.add("Test", Priority.MEDIUM)
    assert store.get(todo.id) is not None

def test_delete(store: TodoStore) -> None:
    todo = store.add("Test", Priority.LOW)
    assert store.delete(todo.id)
    assert not store.delete(todo.id)

def test_search(store: TodoStore) -> None:
    store.add("Buy milk", Priority.HIGH)
    store.add("Buy eggs", Priority.MEDIUM)
    store.add("Clean", Priority.LOW)
    assert len(store.search("buy")) == 2
EOF

echo "Python webapp fixture ready: $(find "$WS" -name '*.py' | wc -l) files"
