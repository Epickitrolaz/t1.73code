use crate::provider::error::PiError;
use tokio::io::{AsyncRead, AsyncReadExt};

pub const DEFAULT_MAX_RECORD_BYTES: usize = 16 * 1024 * 1024; // 16 MiB

#[derive(Debug)]
pub struct JsonlFramer {
    buffer: Vec<u8>,
    max_line_length: usize,
    dropping: bool,
    dropped_lengths: Vec<usize>,
}

impl JsonlFramer {
    pub fn new(max_line_length: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_line_length,
            dropping: false,
            dropped_lengths: Vec::new(),
        }
    }

    pub fn with_default_limit() -> Self {
        Self::new(DEFAULT_MAX_RECORD_BYTES)
    }

    pub fn max_line_length(&self) -> usize {
        self.max_line_length
    }

    pub fn dropped_lengths(&self) -> &[usize] {
        &self.dropped_lengths
    }

    pub fn push_chunk(&mut self, chunk: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        let mut start = 0;

        while start < chunk.len() {
            let newline = chunk[start..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|p| start + p);
            let end = newline.unwrap_or(chunk.len());

            if !self.dropping {
                if self.buffer.len() + (end - start) > self.max_line_length {
                    self.dropped_lengths.push(self.buffer.len() + (end - start));
                    self.buffer.clear();
                    self.dropping = true;
                } else {
                    self.buffer.extend_from_slice(&chunk[start..end]);
                }
            }

            if newline.is_none() {
                break;
            }

            if !self.dropping && !self.buffer.is_empty() {
                if self.buffer.ends_with(b"\r") {
                    self.buffer.pop();
                }
                if !self.buffer.is_empty() {
                    if let Ok(line) = String::from_utf8(std::mem::take(&mut self.buffer)) {
                        lines.push(line);
                    } else {
                        self.buffer.clear();
                    }
                } else {
                    self.buffer.clear();
                }
            } else {
                self.buffer.clear();
            }

            self.dropping = false;
            start = end + 1;
        }

        lines
    }

    pub fn flush(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.dropping && !self.buffer.is_empty() {
            if self.buffer.ends_with(b"\r") {
                self.buffer.pop();
            }
            if !self.buffer.is_empty() {
                if let Ok(line) = String::from_utf8(std::mem::take(&mut self.buffer)) {
                    lines.push(line);
                }
            }
        }
        self.buffer.clear();
        self.dropping = false;
        lines
    }

    pub async fn read_line<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<String>, PiError> {
        let mut chunk_buf = [0u8; 8192];
        loop {
            if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
                let mut line_bytes = self.buffer.drain(..=pos).collect::<Vec<u8>>();
                line_bytes.pop(); // Remove '\n'
                if line_bytes.ends_with(b"\r") {
                    line_bytes.pop();
                }
                if self.dropping {
                    self.dropping = false;
                    continue;
                }
                if line_bytes.is_empty() {
                    continue;
                }
                let line = String::from_utf8(line_bytes)
                    .map_err(|e| PiError::InvalidResponse(format!("UTF-8 decode error: {e}")))?;
                return Ok(Some(line));
            }

            let n = reader.read(&mut chunk_buf).await.map_err(PiError::Io)?;
            if n == 0 {
                if self.buffer.is_empty() || self.dropping {
                    self.buffer.clear();
                    self.dropping = false;
                    return Ok(None);
                }
                let mut line_bytes = std::mem::take(&mut self.buffer);
                if line_bytes.ends_with(b"\r") {
                    line_bytes.pop();
                }
                if line_bytes.is_empty() {
                    return Ok(None);
                }
                let line = String::from_utf8(line_bytes)
                    .map_err(|e| PiError::InvalidResponse(format!("UTF-8 decode error: {e}")))?;
                return Ok(Some(line));
            }

            if self.buffer.len() + n > self.max_line_length {
                if !self.dropping {
                    self.dropping = true;
                    self.dropped_lengths.push(self.buffer.len() + n);
                    self.buffer.clear();
                }
                if let Some(pos) = chunk_buf[..n].iter().position(|&b| b == b'\n') {
                    self.dropping = false;
                    self.buffer.extend_from_slice(&chunk_buf[pos + 1..n]);
                }
            } else {
                self.buffer.extend_from_slice(&chunk_buf[..n]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_framer_single_line() {
        let mut framer = JsonlFramer::new(1024);
        let lines = framer.push_chunk(b"{\"type\":\"ready\"}\n");
        assert_eq!(lines, vec!["{\"type\":\"ready\"}"]);
    }

    #[test]
    fn test_framer_crlf() {
        let mut framer = JsonlFramer::new(1024);
        let lines = framer.push_chunk(b"{\"type\":\"ready\"}\r\n");
        assert_eq!(lines, vec!["{\"type\":\"ready\"}"]);
    }

    #[test]
    fn test_framer_multiple_lines_in_single_chunk() {
        let mut framer = JsonlFramer::new(1024);
        let chunk = b"{\"id\":1}\n{\"id\":2}\r\n{\"id\":3}\n";
        let lines = framer.push_chunk(chunk);
        assert_eq!(lines, vec!["{\"id\":1}", "{\"id\":2}", "{\"id\":3}"]);
    }

    #[test]
    fn test_framer_lines_split_across_chunks() {
        let mut framer = JsonlFramer::new(1024);
        let lines1 = framer.push_chunk(b"{\"message\": \"hel");
        assert!(lines1.is_empty());
        let lines2 = framer.push_chunk(b"lo\"}\n");
        assert_eq!(lines2, vec!["{\"message\": \"hello\"}"]);
    }

    #[test]
    fn test_framer_empty_lines() {
        let mut framer = JsonlFramer::new(1024);
        let lines = framer.push_chunk(b"\n\r\n{\"valid\":true}\n\n");
        assert_eq!(lines, vec!["{\"valid\":true}"]);
    }

    #[test]
    fn test_framer_oversized_line_dropped() {
        let mut framer = JsonlFramer::new(20);
        let oversized = b"12345678901234567890EXTRA_BYTES\n{\"next\":1}\n";
        let lines = framer.push_chunk(oversized);
        assert_eq!(lines, vec!["{\"next\":1}"]);
        assert_eq!(framer.dropped_lengths().len(), 1);
    }

    #[test]
    fn test_framer_flush() {
        let mut framer = JsonlFramer::new(1024);
        let lines1 = framer.push_chunk(b"{\"incomplete\":true}");
        assert!(lines1.is_empty());
        let flushed = framer.flush();
        assert_eq!(flushed, vec!["{\"incomplete\":true}"]);
    }

    #[tokio::test]
    async fn test_framer_async_read_line() {
        let mut cursor = Cursor::new(b"{\"line\":1}\n{\"line\":2}\r\n{\"line\":3}");
        let mut framer = JsonlFramer::new(1024);

        let line1 = framer.read_line(&mut cursor).await.unwrap();
        assert_eq!(line1, Some("{\"line\":1}".to_string()));

        let line2 = framer.read_line(&mut cursor).await.unwrap();
        assert_eq!(line2, Some("{\"line\":2}".to_string()));

        let line3 = framer.read_line(&mut cursor).await.unwrap();
        assert_eq!(line3, Some("{\"line\":3}".to_string()));

        let line4 = framer.read_line(&mut cursor).await.unwrap();
        assert_eq!(line4, None);
    }
}
