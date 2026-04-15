#!/bin/bash
set -e
WS="${1:?Usage: setup.sh <workspace_dir>}"
rm -rf "$WS" && mkdir -p "$WS"/{src/components,src/hooks,src/screens,src/__tests__,.magic-code}

cat > "$WS/package.json" << 'EOF'
{"name":"todo-mobile","version":"0.1.0","private":true,"dependencies":{"react":"^18.3.0","react-native":"^0.75.0","@react-navigation/native":"^6.1.0"},"devDependencies":{"@testing-library/react-native":"^12.0.0","jest":"^29.7.0","typescript":"^5.5.0","@types/react":"^18.3.0"},"scripts":{"test":"jest","start":"react-native start"}}
EOF

cat > "$WS/tsconfig.json" << 'EOF'
{"compilerOptions":{"target":"ES2020","lib":["es2020"],"jsx":"react-native","module":"esnext","moduleResolution":"node","strict":true,"esModuleInterop":true},"include":["src"]}
EOF

cat > "$WS/CLAUDE.md" << 'EOF'
# todo-mobile (React Native/TypeScript)
## Conventions
- Functional components with hooks only
- Custom hooks for business logic
- TypeScript strict — no `any`
- Tests use React Native Testing Library
- Screens in src/screens/, reusable components in src/components/
- Use StyleSheet.create for styles (no inline)
- Accessibility: all touchable elements need accessibilityLabel
## Architecture: types.ts → hooks/ (logic) → components/ (UI) → screens/ (pages) → App.tsx
EOF

cat > "$WS/.magic-code/instructions.md" << 'EOF'
- Run npm test after editing TypeScript files
- All components must have accessibilityLabel props
- Use TypeScript strict types
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
EOF

cat > "$WS/src/hooks/useTodos.ts" << 'EOF'
import { useState, useCallback } from 'react';
import { Todo, Priority } from '../types';

let nextId = 1;

export function useTodos() {
  const [todos, setTodos] = useState<Todo[]>([]);

  const addTodo = useCallback((title: string, priority: Priority = 'medium') => {
    const todo: Todo = {
      id: nextId++, title, completed: false, priority, tags: [], createdAt: new Date(),
    };
    setTodos(prev => [...prev, todo]);
  }, []);

  const toggleTodo = useCallback((id: number) => {
    setTodos(prev => prev.map(t => t.id === id ? { ...t, completed: !t.completed } : t));
  }, []);

  const deleteTodo = useCallback((id: number) => {
    setTodos(prev => prev.filter(t => t.id !== id));
  }, []);

  return { todos, addTodo, toggleTodo, deleteTodo };
}
EOF

cat > "$WS/src/components/TodoItem.tsx" << 'EOF'
import React from 'react';
import { View, Text, TouchableOpacity, StyleSheet } from 'react-native';
import { Todo } from '../types';

interface TodoItemProps {
  todo: Todo;
  onToggle: (id: number) => void;
  onDelete: (id: number) => void;
}

export const TodoItem: React.FC<TodoItemProps> = ({ todo, onToggle, onDelete }) => (
  <View style={styles.container}>
    <TouchableOpacity onPress={() => onToggle(todo.id)} accessibilityLabel={`Toggle ${todo.title}`}>
      <Text style={[styles.title, todo.completed && styles.completed]}>
        {todo.completed ? '✓' : '○'} {todo.title}
      </Text>
    </TouchableOpacity>
    <Text style={styles.priority}>{todo.priority}</Text>
    <TouchableOpacity onPress={() => onDelete(todo.id)} accessibilityLabel={`Delete ${todo.title}`}>
      <Text style={styles.deleteBtn}>✕</Text>
    </TouchableOpacity>
  </View>
);

const styles = StyleSheet.create({
  container: { flexDirection: 'row', alignItems: 'center', padding: 12, borderBottomWidth: 1, borderBottomColor: '#eee' },
  title: { flex: 1, fontSize: 16 },
  completed: { textDecorationLine: 'line-through', color: '#999' },
  priority: { fontSize: 12, color: '#666', marginRight: 8 },
  deleteBtn: { fontSize: 18, color: 'red', padding: 4 },
});
EOF

cat > "$WS/src/screens/HomeScreen.tsx" << 'EOF'
import React, { useState } from 'react';
import { View, TextInput, FlatList, TouchableOpacity, Text, StyleSheet } from 'react-native';
import { useTodos } from '../hooks/useTodos';
import { TodoItem } from '../components/TodoItem';

export const HomeScreen: React.FC = () => {
  const { todos, addTodo, toggleTodo, deleteTodo } = useTodos();
  const [input, setInput] = useState('');

  const handleAdd = () => {
    if (input.trim()) {
      addTodo(input.trim());
      setInput('');
    }
  };

  return (
    <View style={styles.container}>
      <View style={styles.inputRow}>
        <TextInput style={styles.input} value={input} onChangeText={setInput} placeholder="Add todo..." />
        <TouchableOpacity style={styles.addBtn} onPress={handleAdd} accessibilityLabel="Add todo">
          <Text style={styles.addBtnText}>+</Text>
        </TouchableOpacity>
      </View>
      <FlatList
        data={todos}
        keyExtractor={item => String(item.id)}
        renderItem={({ item }) => <TodoItem todo={item} onToggle={toggleTodo} onDelete={deleteTodo} />}
      />
    </View>
  );
};

const styles = StyleSheet.create({
  container: { flex: 1, padding: 16 },
  inputRow: { flexDirection: 'row', marginBottom: 16 },
  input: { flex: 1, borderWidth: 1, borderColor: '#ccc', borderRadius: 8, padding: 8 },
  addBtn: { marginLeft: 8, backgroundColor: '#007AFF', borderRadius: 8, padding: 12 },
  addBtnText: { color: 'white', fontSize: 18, fontWeight: 'bold' },
});
EOF

cat > "$WS/src/__tests__/useTodos.test.ts" << 'EOF'
import { renderHook, act } from '@testing-library/react-native';
import { useTodos } from '../hooks/useTodos';

describe('useTodos', () => {
  it('adds a todo', () => {
    const { result } = renderHook(() => useTodos());
    act(() => { result.current.addTodo('Test'); });
    expect(result.current.todos).toHaveLength(1);
  });

  it('toggles a todo', () => {
    const { result } = renderHook(() => useTodos());
    act(() => { result.current.addTodo('Test'); });
    const id = result.current.todos[0].id;
    act(() => { result.current.toggleTodo(id); });
    expect(result.current.todos[0].completed).toBe(true);
  });
});
EOF

echo "React Native mobile fixture ready: $(find "$WS" -name '*.ts' -o -name '*.tsx' | wc -l) files"
