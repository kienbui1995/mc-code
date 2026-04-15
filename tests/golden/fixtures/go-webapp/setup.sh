#!/bin/bash
set -e
WS="${1:?Usage: setup.sh <workspace_dir>}"
rm -rf "$WS" && mkdir -p "$WS"/{handler,store,model,.magic-code}

cat > "$WS/go.mod" << 'EOF'
module github.com/example/todo-api

go 1.22
EOF

cat > "$WS/CLAUDE.md" << 'EOF'
# todo-api (Go)
## Conventions
- Standard library only (no frameworks like Gin/Echo)
- Interfaces for testability (Store interface)
- Error wrapping with fmt.Errorf("...: %w", err)
- Table-driven tests
- Package comments on every package
- Context as first parameter where applicable
## Architecture: model/ (data) → store/ (logic) → handler/ (HTTP) → main.go (server)
EOF

cat > "$WS/.magic-code/instructions.md" << 'EOF'
- Run go test ./... after editing Go files
- Use Go idioms: error handling, interfaces, goroutines
- Prefer edit_file over write_file for existing files
- Read a file before editing it
EOF

cat > "$WS/model/todo.go" << 'EOF'
// Package model defines the core data types.
package model

import (
	"fmt"
	"sync/atomic"
	"time"
)

var nextID atomic.Int64

// Priority represents the urgency level of a todo.
type Priority string

const (
	PriorityLow    Priority = "low"
	PriorityMedium Priority = "medium"
	PriorityHigh   Priority = "high"
)

// Todo represents a single todo item.
type Todo struct {
	ID        int64     `json:"id"`
	Title     string    `json:"title"`
	Completed bool      `json:"completed"`
	Priority  Priority  `json:"priority"`
	Tags      []string  `json:"tags"`
	CreatedAt time.Time `json:"created_at"`
}

// NewTodo creates a new todo with the given title and priority.
func NewTodo(title string, priority Priority) *Todo {
	return &Todo{
		ID:        nextID.Add(1),
		Title:     title,
		Priority:  priority,
		Tags:      []string{},
		CreatedAt: time.Now(),
	}
}

// Toggle flips the completed status.
func (t *Todo) Toggle() {
	t.Completed = !t.Completed
}

// AddTag adds a tag if not already present.
func (t *Todo) AddTag(tag string) {
	for _, existing := range t.Tags {
		if existing == tag {
			return
		}
	}
	t.Tags = append(t.Tags, tag)
}

// String implements fmt.Stringer.
func (t *Todo) String() string {
	status := "○"
	if t.Completed {
		status = "✓"
	}
	return fmt.Sprintf("[%s] %s (%s) #%d", status, t.Title, t.Priority, t.ID)
}
EOF

cat > "$WS/store/store.go" << 'EOF'
// Package store provides in-memory todo storage.
package store

import (
	"fmt"
	"sort"
	"strings"
	"sync"

	"github.com/example/todo-api/model"
)

// Store manages todo items.
type Store struct {
	mu    sync.RWMutex
	todos map[int64]*model.Todo
}

// New creates a new Store.
func New() *Store {
	return &Store{todos: make(map[int64]*model.Todo)}
}

// Add creates and stores a new todo.
func (s *Store) Add(title string, priority model.Priority) *model.Todo {
	todo := model.NewTodo(title, priority)
	s.mu.Lock()
	defer s.mu.Unlock()
	s.todos[todo.ID] = todo
	return todo
}

// Get returns a todo by ID.
func (s *Store) Get(id int64) (*model.Todo, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	t, ok := s.todos[id]
	if !ok {
		return nil, fmt.Errorf("todo %d: not found", id)
	}
	return t, nil
}

// List returns all todos sorted by ID.
func (s *Store) List() []*model.Todo {
	s.mu.RLock()
	defer s.mu.RUnlock()
	result := make([]*model.Todo, 0, len(s.todos))
	for _, t := range s.todos {
		result = append(result, t)
	}
	sort.Slice(result, func(i, j int) bool { return result[i].ID < result[j].ID })
	return result
}

// Delete removes a todo by ID.
func (s *Store) Delete(id int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, ok := s.todos[id]; !ok {
		return fmt.Errorf("todo %d: not found", id)
	}
	delete(s.todos, id)
	return nil
}

// Search returns todos matching the query (case-insensitive).
func (s *Store) Search(query string) []*model.Todo {
	s.mu.RLock()
	defer s.mu.RUnlock()
	q := strings.ToLower(query)
	var result []*model.Todo
	for _, t := range s.todos {
		if strings.Contains(strings.ToLower(t.Title), q) {
			result = append(result, t)
		}
	}
	return result
}

// Stats returns total and completed counts.
func (s *Store) Stats() (total, completed int) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	for _, t := range s.todos {
		total++
		if t.Completed {
			completed++
		}
	}
	return
}
EOF

cat > "$WS/store/store_test.go" << 'EOF'
package store

import (
	"testing"

	"github.com/example/todo-api/model"
)

func TestAddAndGet(t *testing.T) {
	s := New()
	todo := s.Add("Test", model.PriorityMedium)
	got, err := s.Get(todo.ID)
	if err != nil {
		t.Fatalf("Get(%d): %v", todo.ID, err)
	}
	if got.Title != "Test" {
		t.Errorf("title = %q, want %q", got.Title, "Test")
	}
}

func TestDelete(t *testing.T) {
	s := New()
	todo := s.Add("Test", model.PriorityLow)
	if err := s.Delete(todo.ID); err != nil {
		t.Fatalf("Delete: %v", err)
	}
	if err := s.Delete(todo.ID); err == nil {
		t.Error("Delete twice: expected error")
	}
}

func TestSearch(t *testing.T) {
	s := New()
	s.Add("Buy milk", model.PriorityHigh)
	s.Add("Buy eggs", model.PriorityMedium)
	s.Add("Clean", model.PriorityLow)
	if got := s.Search("buy"); len(got) != 2 {
		t.Errorf("Search(buy) = %d results, want 2", len(got))
	}
}
EOF

cat > "$WS/main.go" << 'EOF'
package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"

	"github.com/example/todo-api/model"
	"github.com/example/todo-api/store"
)

var todoStore = store.New()

func main() {
	http.HandleFunc("/todos", handleTodos)
	fmt.Println("Server starting on :8080")
	log.Fatal(http.ListenAndServe(":8080", nil))
}

func handleTodos(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		todos := todoStore.List()
		json.NewEncoder(w).Encode(todos)
	case http.MethodPost:
		var req struct {
			Title    string         `json:"title"`
			Priority model.Priority `json:"priority"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		todo := todoStore.Add(req.Title, req.Priority)
		w.WriteHeader(http.StatusCreated)
		json.NewEncoder(w).Encode(todo)
	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}
EOF

echo "Go webapp fixture ready: $(find "$WS" -name '*.go' | wc -l) files"
