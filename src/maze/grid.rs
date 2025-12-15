use super::cell::GridCell;
use crossterm::style::{ContentStyle, StyledContent, Stylize};

pub struct Grid {
    data: Box<[GridCell]>,
    width: u16,
    height: u16,
    grid_event_tx: Option<std::sync::mpsc::SyncSender<GridEvent>>,
}

#[derive(Debug, Clone, Copy)]
pub enum GridEvent {
    Initial {
        cell: GridCell,
        width: u16,
        height: u16,
    },
    Update {
        coord: (u16, u16),
        old: GridCell,
        new: GridCell,
    },
}

impl Stylize for GridEvent {
    type Styled = StyledContent<String>;

    fn stylize(self) -> Self::Styled {
        StyledContent::new(ContentStyle::default(), format!("{:?}", self))
    }
}

impl std::fmt::Display for GridEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let content = match self {
            GridEvent::Initial {
                cell,
                width,
                height,
            } => {
                format!("Initial Grid: {}x{} with cell {:?}", width, height, cell)
            }
            GridEvent::Update { coord, old, new } => format!(
                "Update at ({}, {}): {:?} -> {:?}",
                coord.0, coord.1, old, new
            ),
        };
        write!(f, "{}", content)
    }
}

impl Grid {
    pub fn new(
        width: u16,
        height: u16,
        cell: GridCell,
        grid_event_tx: Option<std::sync::mpsc::SyncSender<GridEvent>>,
    ) -> Self {
        let data = vec![cell; width as usize * height as usize].into_boxed_slice();
        if let Some(s) = &grid_event_tx {
            let _ = s.send(GridEvent::Initial {
                cell,
                width,
                height,
            });
        }
        Grid {
            data,
            width,
            height,
            grid_event_tx,
        }
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn is_boundary(&self, x: u16, y: u16) -> bool {
        x == 0 || y == 0 || x == self.width - 1 || y == self.height - 1
    }

    fn ravel_index(&self, x: u16, y: u16) -> usize {
        // Overflow-safe since width and height are u16 (assuming usize is at least 32 bits)
        y as usize * self.width as usize + x as usize
    }

    pub fn set(&mut self, coord: (u16, u16), cell: GridCell) {
        let idx = self.ravel_index(coord.0, coord.1);
        let old = self.data[idx];
        if old != cell {
            self.data[idx] = cell;
            if let Some(sender) = &self.grid_event_tx {
                // This operation will block if the channel is full.
                // The receiver should be running in a separate thread to avoid deadlocks.
                let _ = sender.send(GridEvent::Update {
                    coord,
                    old,
                    new: cell,
                });
            }
        }
    }
}

impl std::ops::Index<(u16, u16)> for Grid {
    type Output = GridCell;

    fn index(&self, index: (u16, u16)) -> &Self::Output {
        &self.data[self.ravel_index(index.0, index.1)]
    }
}
