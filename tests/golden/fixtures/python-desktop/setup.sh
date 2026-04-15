#!/bin/bash
set -e
WS="${1:?Usage: setup.sh <workspace_dir>}"
rm -rf "$WS" && mkdir -p "$WS"/{app,tests,.magic-code}

cat > "$WS/CLAUDE.md" << 'EOF'
# todo-desktop (Python/Tkinter)
## Conventions
- Type hints on all functions
- MVC pattern: model (data), store (logic), ui (Tkinter widgets)
- Tests use pytest
- No external dependencies beyond stdlib + pytest
- Docstrings on all public functions
## Architecture: app/model.py → app/store.py → app/ui.py → main.py
EOF

cat > "$WS/.magic-code/instructions.md" << 'EOF'
- Run pytest after editing Python files
- Use type hints everywhere
- Follow MVC pattern strictly
- Read a file before editing it
EOF

cat > "$WS/app/__init__.py" << 'EOF'
EOF

cat > "$WS/app/model.py" << 'EOF'
"""Data models for the todo desktop app."""
from dataclasses import dataclass, field
from enum import Enum
from datetime import datetime

class Priority(Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"

_next_id = 0

def _gen_id() -> int:
    global _next_id
    _next_id += 1
    return _next_id

@dataclass
class Todo:
    title: str
    priority: Priority = Priority.MEDIUM
    completed: bool = False
    tags: list[str] = field(default_factory=list)
    id: int = field(default_factory=_gen_id)
    created_at: datetime = field(default_factory=datetime.now)

    def toggle(self) -> None:
        self.completed = not self.completed

    def add_tag(self, tag: str) -> None:
        if tag not in self.tags:
            self.tags.append(tag)
EOF

cat > "$WS/app/store.py" << 'EOF'
"""In-memory todo storage."""
from app.model import Todo, Priority

class TodoStore:
    def __init__(self) -> None:
        self._todos: dict[int, Todo] = {}

    def add(self, title: str, priority: Priority = Priority.MEDIUM) -> Todo:
        todo = Todo(title=title, priority=priority)
        self._todos[todo.id] = todo
        return todo

    def get(self, todo_id: int) -> Todo | None:
        return self._todos.get(todo_id)

    def list_all(self) -> list[Todo]:
        return sorted(self._todos.values(), key=lambda t: t.id)

    def delete(self, todo_id: int) -> bool:
        return self._todos.pop(todo_id, None) is not None

    def search(self, query: str) -> list[Todo]:
        q = query.lower()
        return [t for t in self._todos.values() if q in t.title.lower()]

    def stats(self) -> tuple[int, int]:
        total = len(self._todos)
        done = sum(1 for t in self._todos.values() if t.completed)
        return total, done
EOF

cat > "$WS/app/ui.py" << 'EOF'
"""Tkinter UI for the todo desktop app."""
import tkinter as tk
from tkinter import ttk, messagebox
from app.store import TodoStore
from app.model import Priority

class TodoApp:
    def __init__(self, root: tk.Tk) -> None:
        self.root = root
        self.root.title("Todo Desktop")
        self.store = TodoStore()
        self._build_ui()
        self._refresh()

    def _build_ui(self) -> None:
        frame = ttk.Frame(self.root, padding=10)
        frame.grid(row=0, column=0, sticky="nsew")

        self.entry = ttk.Entry(frame, width=40)
        self.entry.grid(row=0, column=0, padx=5)

        ttk.Button(frame, text="Add", command=self._add).grid(row=0, column=1)

        self.listbox = tk.Listbox(frame, width=50, height=15)
        self.listbox.grid(row=1, column=0, columnspan=2, pady=10)

        btn_frame = ttk.Frame(frame)
        btn_frame.grid(row=2, column=0, columnspan=2)
        ttk.Button(btn_frame, text="Toggle", command=self._toggle).pack(side=tk.LEFT, padx=5)
        ttk.Button(btn_frame, text="Delete", command=self._delete).pack(side=tk.LEFT, padx=5)

        self.status = ttk.Label(frame, text="")
        self.status.grid(row=3, column=0, columnspan=2, pady=5)

    def _add(self) -> None:
        title = self.entry.get().strip()
        if title:
            self.store.add(title)
            self.entry.delete(0, tk.END)
            self._refresh()

    def _toggle(self) -> None:
        sel = self.listbox.curselection()
        if sel:
            todos = self.store.list_all()
            todos[sel[0]].toggle()
            self._refresh()

    def _delete(self) -> None:
        sel = self.listbox.curselection()
        if sel:
            todos = self.store.list_all()
            self.store.delete(todos[sel[0]].id)
            self._refresh()

    def _refresh(self) -> None:
        self.listbox.delete(0, tk.END)
        for t in self.store.list_all():
            mark = "✓" if t.completed else "○"
            self.listbox.insert(tk.END, f"[{mark}] {t.title} ({t.priority.value})")
        total, done = self.store.stats()
        self.status.config(text=f"{done}/{total} completed")
EOF

cat > "$WS/main.py" << 'EOF'
"""Entry point for the todo desktop app."""
import tkinter as tk
from app.ui import TodoApp

def main() -> None:
    root = tk.Tk()
    TodoApp(root)
    root.mainloop()

if __name__ == "__main__":
    main()
EOF

cat > "$WS/tests/test_store.py" << 'EOF'
import pytest
from app.store import TodoStore
from app.model import Priority

@pytest.fixture
def store() -> TodoStore:
    return TodoStore()

def test_add(store: TodoStore) -> None:
    t = store.add("Test")
    assert t.title == "Test"
    assert store.get(t.id) is not None

def test_delete(store: TodoStore) -> None:
    t = store.add("Test")
    assert store.delete(t.id)
    assert not store.delete(t.id)

def test_search(store: TodoStore) -> None:
    store.add("Buy milk", Priority.HIGH)
    store.add("Clean", Priority.LOW)
    assert len(store.search("buy")) == 1
EOF

echo "Python desktop fixture ready: $(find "$WS" -name '*.py' | wc -l) files"
