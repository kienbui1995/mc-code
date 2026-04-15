#!/bin/bash
set -e
WS="${1:?Usage: setup.sh <workspace_dir>}"
rm -rf "$WS" && mkdir -p "$WS"/{src/components,src/hooks,src/__tests__,.magic-code}

cat > "$WS/package.json" << 'EOF'
{"name":"todo-app","version":"0.1.0","private":true,"dependencies":{"react":"^18.3.0","react-dom":"^18.3.0","typescript":"^5.5.0"},"devDependencies":{"@testing-library/react":"^16.0.0","jest":"^29.7.0","@types/react":"^18.3.0"},"scripts":{"start":"react-scripts start","test":"jest","build":"react-scripts build"}}
EOF

cat > "$WS/tsconfig.json" << 'EOF'
{"compilerOptions":{"target":"ES2020","lib":["dom","es2020"],"jsx":"react-jsx","module":"esnext","moduleResolution":"node","strict":true,"esModuleInterop":true,"outDir":"dist"},"include":["src"]}
EOF

cat > "$WS/CLAUDE.md" << 'EOF'
# todo-app (React/TypeScript)
## Conventions
- Functional components only (no class components)
- Custom hooks for shared logic (useXxx naming)
- TypeScript strict mode — no `any` types
- Tests use React Testing Library + Jest
- CSS modules for styling (no inline styles)
- Props interfaces named XxxProps
## Architecture: types.ts (data) → hooks/ (logic) → components/ (UI) → App.tsx (root)
EOF

cat > "$WS/.magic-code/instructions.md" << 'EOF'
- Run npm test after editing TypeScript files
- Use TypeScript strict types everywhere
- Prefer edit_file over write_file for existing files
- Read a file before editing it
EOF

cat > "$WS/src/types.ts" << 'EOF'
export type Priority = 'low' | 'medium' | 'high';

export interface Todo {
  id: number;
  title: string;
  completed: boolean;
  priority: Priority;
  tags: string[];
  createdAt: Date;
}

export interface TodoCreate {
  title: string;
  priority?: Priority;
  tags?: string[];
}
EOF

cat > "$WS/src/hooks/useTodos.ts" << 'EOF'
import { useState, useCallback } from 'react';
import { Todo, TodoCreate, Priority } from '../types';

let nextId = 1;

export function useTodos() {
  const [todos, setTodos] = useState<Todo[]>([]);

  const addTodo = useCallback((input: TodoCreate) => {
    const todo: Todo = {
      id: nextId++,
      title: input.title,
      completed: false,
      priority: input.priority ?? 'medium',
      tags: input.tags ?? [],
      createdAt: new Date(),
    };
    setTodos(prev => [...prev, todo]);
    return todo;
  }, []);

  const toggleTodo = useCallback((id: number) => {
    setTodos(prev =>
      prev.map(t => t.id === id ? { ...t, completed: !t.completed } : t)
    );
  }, []);

  const deleteTodo = useCallback((id: number) => {
    setTodos(prev => prev.filter(t => t.id !== id));
  }, []);

  const searchTodos = useCallback((query: string) => {
    const q = query.toLowerCase();
    return todos.filter(t => t.title.toLowerCase().includes(q));
  }, [todos]);

  const stats = {
    total: todos.length,
    completed: todos.filter(t => t.completed).length,
    pending: todos.filter(t => !t.completed).length,
  };

  return { todos, addTodo, toggleTodo, deleteTodo, searchTodos, stats };
}
EOF

cat > "$WS/src/components/TodoItem.tsx" << 'EOF'
import React from 'react';
import { Todo } from '../types';

interface TodoItemProps {
  todo: Todo;
  onToggle: (id: number) => void;
  onDelete: (id: number) => void;
}

export const TodoItem: React.FC<TodoItemProps> = ({ todo, onToggle, onDelete }) => {
  return (
    <div className="todo-item">
      <input
        type="checkbox"
        checked={todo.completed}
        onChange={() => onToggle(todo.id)}
        aria-label={`Toggle ${todo.title}`}
      />
      <span className={todo.completed ? 'completed' : ''}>
        {todo.title}
      </span>
      <span className="priority">{todo.priority}</span>
      <button onClick={() => onDelete(todo.id)} aria-label={`Delete ${todo.title}`}>
        Delete
      </button>
    </div>
  );
};
EOF

cat > "$WS/src/components/TodoList.tsx" << 'EOF'
import React from 'react';
import { Todo } from '../types';
import { TodoItem } from './TodoItem';

interface TodoListProps {
  todos: Todo[];
  onToggle: (id: number) => void;
  onDelete: (id: number) => void;
}

export const TodoList: React.FC<TodoListProps> = ({ todos, onToggle, onDelete }) => {
  if (todos.length === 0) {
    return <p>No todos yet. Add one above!</p>;
  }
  return (
    <div className="todo-list">
      {todos.map(todo => (
        <TodoItem key={todo.id} todo={todo} onToggle={onToggle} onDelete={onDelete} />
      ))}
    </div>
  );
};
EOF

cat > "$WS/src/App.tsx" << 'EOF'
import React, { useState } from 'react';
import { useTodos } from './hooks/useTodos';
import { TodoList } from './components/TodoList';

const App: React.FC = () => {
  const { todos, addTodo, toggleTodo, deleteTodo, stats } = useTodos();
  const [input, setInput] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (input.trim()) {
      addTodo({ title: input.trim() });
      setInput('');
    }
  };

  return (
    <div className="app">
      <h1>Todo App</h1>
      <form onSubmit={handleSubmit}>
        <input value={input} onChange={e => setInput(e.target.value)} placeholder="Add todo..." />
        <button type="submit">Add</button>
      </form>
      <p>{stats.completed}/{stats.total} completed</p>
      <TodoList todos={todos} onToggle={toggleTodo} onDelete={deleteTodo} />
    </div>
  );
};

export default App;
EOF

cat > "$WS/src/__tests__/useTodos.test.ts" << 'EOF'
import { renderHook, act } from '@testing-library/react';
import { useTodos } from '../hooks/useTodos';

describe('useTodos', () => {
  it('adds a todo', () => {
    const { result } = renderHook(() => useTodos());
    act(() => { result.current.addTodo({ title: 'Test' }); });
    expect(result.current.todos).toHaveLength(1);
    expect(result.current.todos[0].title).toBe('Test');
  });

  it('toggles a todo', () => {
    const { result } = renderHook(() => useTodos());
    act(() => { result.current.addTodo({ title: 'Test' }); });
    const id = result.current.todos[0].id;
    act(() => { result.current.toggleTodo(id); });
    expect(result.current.todos[0].completed).toBe(true);
  });

  it('deletes a todo', () => {
    const { result } = renderHook(() => useTodos());
    act(() => { result.current.addTodo({ title: 'Test' }); });
    const id = result.current.todos[0].id;
    act(() => { result.current.deleteTodo(id); });
    expect(result.current.todos).toHaveLength(0);
  });
});
EOF

echo "React webapp fixture ready: $(find "$WS" -name '*.ts' -o -name '*.tsx' | wc -l) files"
