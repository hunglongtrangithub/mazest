use crate::{
    generators::get_rng,
    maze::{Cell, Maze, Orientation},
};
use rand::seq::SliceRandom;

struct UnionFind {
    parent: Vec<u16>,
    rank: Vec<u16>,
}

impl UnionFind {
    fn new(size: u16) -> Self {
        UnionFind {
            parent: (0..size).collect(),
            rank: vec![0; size as usize],
        }
    }

    fn find(&mut self, x: u16) -> u16 {
        if self.parent[x as usize] != x {
            self.parent[x as usize] = self.find(self.parent[x as usize]);
        }
        self.parent[x as usize]
    }

    fn unite(&mut self, x: u16, y: u16) -> bool {
        let root_x = self.find(x);
        let root_y = self.find(y);

        if root_x == root_y {
            return false; // Already in same set
        }

        match self.rank[root_x as usize].cmp(&self.rank[root_y as usize]) {
            std::cmp::Ordering::Greater => {
                self.parent[root_y as usize] = root_x;
            }
            std::cmp::Ordering::Less => {
                self.parent[root_x as usize] = root_y;
            }
            std::cmp::Ordering::Equal => {
                self.parent[root_y as usize] = root_x;
                self.rank[root_x as usize] += 1;
            }
        }
        true
    }
}

/// Wall edge between two adjacent cells
#[derive(Clone, Copy)]
struct Edge {
    cell1: (u8, u8),
    cell2: (u8, u8),
}

pub fn randomized_kruskal(maze: &mut Maze, seed: Option<u64>) {
    if maze.is_empty() {
        return;
    }

    let width = maze.width();
    let height = maze.height();

    maze.fill_walls();
    (0..height).for_each(|y| {
        (0..width).for_each(|x| maze[(x, y)] = Cell::PATH);
    });
    maze.render().ok();

    // Initialize Union-Find for all cells
    let total_cells = (width as u16) * (height as u16);
    let mut uf = UnionFind::new(total_cells);

    // Collect all possible edges (walls between adjacent cells)
    let mut edges: Vec<Edge> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            [
                (x + 1 < width).then(|| Edge {
                    cell1: (x, y),
                    cell2: (x + 1, y),
                }),
                (y + 1 < height).then(|| Edge {
                    cell1: (x, y),
                    cell2: (x, y + 1),
                }),
            ]
        })
        .flatten()
        .collect();

    // Shuffle edges randomly
    let mut rng = get_rng(seed);
    edges.shuffle(&mut rng);

    // Process each edge
    for edge in edges {
        let (x1, y1) = edge.cell1;
        let (x2, y2) = edge.cell2;

        // Convert cell coordinates to UnionFind indices
        let idx1 = (y1 as u16) * (width as u16) + (x1 as u16);
        let idx2 = (y2 as u16) * (width as u16) + (x2 as u16);

        // If cells are not yet connected, remove the wall between them
        if uf.find(idx1) != uf.find(idx2) {
            uf.unite(idx1, idx2);

            // Remove the wall between cell1 and cell2
            let (from, orientation) = if x1 == x2 {
                // Vertical edge (same column)
                (
                    std::cmp::min_by_key(edge.cell1, edge.cell2, |&e| e.1),
                    Orientation::Horizontal,
                )
            } else {
                // Horizontal edge (same row)
                (
                    std::cmp::min_by_key(edge.cell1, edge.cell2, |&e| e.0),
                    Orientation::Vertical,
                )
            };
            maze.remove_wall_cell_after(from, orientation);

            maze.render().ok();
        }
    }
}
