# What's New

## Version 0.2.0

### 🎉 New Features

- **Image support** — Send screenshots and diagrams to the AI with `/image`
- **Long-term memory** — The AI remembers project facts across sessions
- **Smart thinking** — See the AI's reasoning process for complex tasks
- **File mentions** — Type `@src/main.rs` to include file content in your prompt
- **Undo changes** — `/undo` reverts the last set of file changes
- **Conversation branching** — Fork conversations to try different approaches
- **Faster tool execution** — Multiple tools now run in parallel

### 🔧 Improvements

- **Lower costs** — Prompt caching reduces API costs by up to 90%
- **Smarter responses** — Dynamic token budget adapts to conversation length
- **More reliable** — Automatic retry when connection drops mid-response
- **Read-only caching** — Repeated file reads are instant

### 📋 New Commands

| Command | What it does |
|---|---|
| `/image <path>` | Attach an image to your prompt |
| `/memory` | View saved project facts |
| `/undo` | Revert last turn's file changes |
| `/thinking` | Toggle AI reasoning display |
| `/fork` | Fork the conversation |
| `/branches` | List conversation branches |
| `/switch <name>` | Switch to a branch |
