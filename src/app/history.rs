use crate::maze::grid::GridEvent;
use std::collections::VecDeque;

pub struct GridEventHistory {
    /// History of grid events, with the most recent event at the front.
    event_history: VecDeque<GridEvent>,
    /// Current index in the history for browsing. Should always be between 0 and event_history.len() - 1
    /// 0 represents the most recent event.
    history_index: usize,
    /// Maximum number of events to keep in history. If 0, no history is kept.
    max_num_events: usize,
}

impl GridEventHistory {
    pub fn new(max_num_events: usize) -> Self {
        GridEventHistory {
            event_history: VecDeque::with_capacity(max_num_events),
            history_index: 0,
            max_num_events,
        }
    }

    pub fn history_forward(&mut self) -> Option<&GridEvent> {
        match self.history_index {
            0 => None, // Already at the most recent event
            _ => {
                self.history_index -= 1;
                self.event_history.get(self.history_index)
            }
        }
    }

    pub fn history_backward(&mut self) -> Option<&GridEvent> {
        if self.history_index + 1 >= self.event_history.len() {
            None
        } else {
            self.history_index += 1;
            self.event_history.get(self.history_index)
        }
    }

    pub fn add_event(&mut self, current_event: GridEvent) {
        match self.max_num_events {
            0 => {} // No history to maintain
            _ => {
                // Remove oldest events if we exceed max history size
                self.event_history.truncate(self.max_num_events - 1);
                // Add new event to the front of the history
                self.event_history.push_front(current_event);
                // Reset history index to the most recent event
                self.history_index = 0;
            }
        }
    }

    pub fn current_event(&self) -> Option<&GridEvent> {
        self.event_history.get(self.history_index)
    }
}
