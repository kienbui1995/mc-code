#[derive(Debug, Default)]
pub struct InputBuffer {
    buffer: String,
    cursor: usize,
}

impl InputBuffer {
    pub fn insert(&mut self, ch: char) {
        self.buffer.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_newline(&mut self) {
        self.insert('\n');
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 { return; }
        let prev = self.buffer[..self.cursor].char_indices().last().map_or(0, |(i, _)| i);
        self.buffer.drain(prev..self.cursor);
        self.cursor = prev;
    }

    /// Delete the previous word (Ctrl+W).
    pub fn delete_word(&mut self) {
        if self.cursor == 0 { return; }
        let before = &self.buffer[..self.cursor];
        // Skip trailing whitespace, then skip non-whitespace
        let trimmed = before.trim_end();
        let word_start = trimmed.rfind(char::is_whitespace).map_or(0, |i| i + 1);
        self.buffer.drain(word_start..self.cursor);
        self.cursor = word_start;
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor].char_indices().last().map_or(0, |(i, _)| i);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            if let Some(ch) = self.buffer[self.cursor..].chars().next() {
                self.cursor += ch.len_utf8();
            }
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str { &self.buffer }

    #[must_use]
    pub fn cursor_pos(&self) -> usize { self.cursor }

    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.buffer)
    }

    pub fn set(&mut self, text: &str) {
        self.buffer = text.to_string();
        self.cursor = self.buffer.len();
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    #[must_use]
    pub fn is_empty(&self) -> bool { self.buffer.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_editing() {
        let mut buf = InputBuffer::default();
        buf.insert('h');
        buf.insert('i');
        assert_eq!(buf.as_str(), "hi");
        buf.backspace();
        assert_eq!(buf.as_str(), "h");
        buf.insert_newline();
        buf.insert('x');
        assert_eq!(buf.as_str(), "h\nx");
    }

    #[test]
    fn take_clears() {
        let mut buf = InputBuffer::default();
        buf.insert('a');
        let taken = buf.take();
        assert_eq!(taken, "a");
        assert!(buf.is_empty());
    }
}
