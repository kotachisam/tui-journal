pub struct InlineCompleterState<C> {
    pub anchor_line: usize,
    pub anchor_col: usize,
    pub query: String,
    pub candidates: Vec<C>,
    pub selected_idx: usize,
}

impl<C> InlineCompleterState<C> {
    pub fn new(anchor_line: usize, anchor_col: usize) -> Self {
        Self {
            anchor_line,
            anchor_col,
            query: String::new(),
            candidates: Vec::new(),
            selected_idx: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_idx + 1 < self.candidates.len() {
            self.selected_idx += 1;
        }
    }

    pub fn selected(&self) -> Option<&C> {
        self.candidates.get(self.selected_idx)
    }
}
