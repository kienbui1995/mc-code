#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Vimmode.
pub enum VimMode {
    Normal,
    Insert,
}

#[derive(Debug, Default)]
/// Inputbuffer.
pub struct InputBuffer {
    buffer: String,
    cursor: usize,
}

impl InputBuffer {
    /// Insert.
    pub fn insert(&mut self, ch: char) {
        self.buffer.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// Insert newline.
    pub fn insert_newline(&mut self) {
        self.insert('\n');
    }

    /// Backspace.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.buffer[..self.cursor]
            .char_indices()
            .last()
            .map_or(0, |(i, _)| i);
        self.buffer.drain(prev..self.cursor);
        self.cursor = prev;
    }

    /// Delete the previous word (Ctrl+W).
    pub fn delete_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.buffer[..self.cursor];
        let trimmed = before.trim_end();
        let word_start = trimmed.rfind(char::is_whitespace).map_or(0, |i| i + 1);
        self.buffer.drain(word_start..self.cursor);
        self.cursor = word_start;
    }

    /// Move left.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map_or(0, |(i, _)| i);
        }
    }

    /// Move right.
    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            if let Some(ch) = self.buffer[self.cursor..].chars().next() {
                self.cursor += ch.len_utf8();
            }
        }
    }

    /// Move cursor to next word boundary (vim w).
    pub fn word_forward(&mut self) {
        let rest = &self.buffer[self.cursor..];
        let skip_word = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let skip_space = rest[skip_word..]
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(rest.len() - skip_word);
        self.cursor = (self.cursor + skip_word + skip_space).min(self.buffer.len());
    }

    /// Move cursor to previous word boundary (vim b).
    pub fn word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.buffer[..self.cursor];
        let trimmed = before.trim_end();
        self.cursor = trimmed.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    }

    /// Delete character under cursor (vim x).
    pub fn delete_char(&mut self) {
        if self.cursor < self.buffer.len() {
            let ch_len = self.buffer[self.cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            self.buffer.drain(self.cursor..self.cursor + ch_len);
        }
    }

    /// Delete entire line (vim dd).
    pub fn delete_line(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Move to start of line (vim 0).
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move to end of line (vim $).
    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Append after cursor (vim a).
    pub fn move_right_for_append(&mut self) {
        self.move_right();
    }

    #[must_use]
    /// As str.
    pub fn as_str(&self) -> &str {
        &self.buffer
    }

    #[must_use]
    /// Cursor pos.
    pub fn cursor_pos(&self) -> usize {
        self.cursor
    }

    /// Take.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.buffer)
    }

    /// Set.
    pub fn set(&mut self, text: &str) {
        self.buffer = text.to_string();
        self.cursor = self.buffer.len();
    }

    /// Clear.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    #[must_use]
    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
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

    #[test]
    fn vim_word_movement() {
        let mut buf = InputBuffer::default();
        buf.set("hello world foo");
        buf.move_home();
        buf.word_forward();
        assert_eq!(buf.cursor_pos(), 6);
        buf.word_forward();
        assert_eq!(buf.cursor_pos(), 12);
        buf.word_backward();
        assert_eq!(buf.cursor_pos(), 6);
    }

    #[test]
    fn vim_delete_char() {
        let mut buf = InputBuffer::default();
        buf.set("abc");
        buf.move_home();
        buf.delete_char();
        assert_eq!(buf.as_str(), "bc");
    }
}
