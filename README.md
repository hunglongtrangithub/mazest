# Mazest

A responsive maze generation and solving visualizer for the terminal, built with Rust.

## Demo

[![asciicast](https://asciinema.org/a/750044.svg)](https://asciinema.org/a/750044)

## Key Features

Mazest is extremely responsive even with large maze sizes. Here are its key features:

- **Real-time visualization** of maze generation and pathfinding algorithms
- **Concurrent architecture** with separate threads for smooth performance
- **Event-driven design** with responsive user interaction
- **Loop mode** - continuously generates and solves mazes with random algorithm combinations

The interactive controls and terminal resize handling features:

- [x] **Pause/Resume** - Enter to pause/resume rendering
- [x] **Navigation** - Left/Right arrow keys to traverse rendering history with on-screen logs
- [x] **Speed Control** - Up/Down arrow keys to adjust rendering speed with on-screen indicator
- [x] **Terminal Resize Handling** - Resume from last valid state when terminal size is restored

## Implemented Algorithms

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

For maze dimensions, currently supports maze sizes up to **255 by 255** (grid sizes up to **511 by 511**). Sizing is based on terminal dimensions by default or manual user input

## Usage

```bash
cargo run
```

## Technical Details

The architecture is **event-driven**, using channels and atomic bools to coordinate termination of threads and communication among threads. There are four main threads:
1. Input thread: listens to terminal events and forward certain events to the main thread
2. Main thread: spawns other threads, and runs the main app loop logic
3. Render thread: listens to user action events and grid update events to render grid animation to the screen, and handles grid display on terminal resizing
4. Compute thread: produces grid update events with a combination of maze generator and solver

The input thread polls terminal events for a 100ms timeout and checks the status in the atomic bools to terminate itself. `std::sync::mpsc::sync_channel` is used between compute thread and render thread to prevent the compute thread aggressively sending grid update events to render thread and blowing up the channel queue.

## Features

## Future Enhancements

- Additional maze generation algorithms
- More pathfinding algorithms
- Support for larger maze sizes (u16 dimensions: up to 65,535×65,535)?

## Terminal Emulator Rendering Experience

When using Mazest, different terminal emulators's rendering performance can have vary significantly for very large mazes. Based on my experience with my Macbook:

- **WezTerm** & **Ghostty** & **Alacritty** - Excellent performance, no lag at max sizes
- **Kitty** - Minor lag noticeable at max sizes
- **macOS Terminal.app** & **Iterm** - Significant lag at max sizes
