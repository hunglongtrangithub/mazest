# Mazest

A high-performance, responsive, concurrent maze generation and solving visualizer for the terminal, built with Rust.

## Demo

[![asciicast](https://asciinema.org/a/37n2GR48FxXtdM3w4afXky4Ku.svg)](https://asciinema.org/a/37n2GR48FxXtdM3w4afXky4Ku)

## Features

### Core Functionality

- **Real-time visualization** of maze generation and pathfinding algorithms
- **Concurrent architecture** with separate compute and render threads for smooth performance
- **Event-driven design** with responsive cancellation and cleanup
- **Loop mode** - continuously generates and solves mazes with random algorithm combinations

### Implemented Algorithms

**Maze Generators:**

- Recursive Backtracking (DFS-based)
- Randomized Prim's Algorithm
- Randomized Kruskal's Algorithm
- Recursive Division

**Pathfinding Solvers:**

- Depth-First Search (DFS)
- Breadth-First Search (BFS)
- Dijkstra's Algorithm
- A\* (A-Star) Search

### Current Controls

- **Escape (Esc)** - Cancel generation/solving at any time
- **Loop mode** - Run continuous random algorithm combinations

### Maze Dimensions

- Supports maze sizes up to **255 by 255** (grid sizes up to **511 by 511**)
- Sizing based on terminal dimensions by default or manual user input

## Usage

```bash
cargo run
```

## Technical Details

### Architecture

- **Event-driven design**
- Only uses `crossterm` for terminal I/O
- **Multi-threaded execution:**
  - Main thread: handles user input events and orchestration
  - Compute thread: runs maze generation and solving algorithms
  - Render thread: processes and displays grid events
  - User input thread: listens to terminal events and send to main thread
- **Atomic flags** for thread synchronization and cancellation
- **Channel-based communication** between threads
- **Buffered rendering** to render grid events to stdout in batches for better performance

### Performance Optimizations

- `std::sync::mpsc::sync_channel` is used between compute thread and render thread to regulate the compute thread aggressively sending grid events to render thread
- Non-blocking user input polling for responsiveness

## Upcoming Features

### Interactive Controls (Planned)

- **Pause/Resume** - Spacebar to pause/resume rendering
- **Navigation** - Left/Right arrow keys to traverse rendering history
- **Speed Control** - Up/Down arrow keys to adjust rendering speed with on-screen indicator
- **Restart** - 'R' key to restart the current generation
- **Loop Toggle** - 'L' key to enable/disable indefinite rendering mode

Apart from user experience reason, the reason I want to have the speed control feature is because different terminal emulators's rendering performance can have vary significantly. Based on my experience on my Macbook:

- **WezTerm** & **Ghostty** & **Alacritty** - Excellent performance, no lag at max sizes
- **Kitty** - Minor lag noticeable at max sizes
- **macOS Terminal.app** & **Iterm** - Significant lag at max sizes

### Terminal Resize Handling (Planned)

- Resume from last valid state when terminal size is restored
- Maybe start a new run when terminal size is too small for current maze size?

### Future Enhancements

- Additional maze generation algorithms
- More pathfinding algorithms
- Support for larger maze sizes (u16 dimensions: up to 65,535×65,535)?
