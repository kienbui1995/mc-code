use crate::error::ProviderError;
use crate::types::AnthropicStreamEvent;

#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    /// Push.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<AnthropicStreamEvent>, ProviderError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(frame) = self.next_frame() {
            if let Some(event) = parse_frame(&frame)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    /// Finish.
    pub fn finish(&mut self) -> Result<Vec<AnthropicStreamEvent>, ProviderError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let trailing = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned();
        match parse_frame(&trailing)? {
            Some(event) => Ok(vec![event]),
            None => Ok(Vec::new()),
        }
    }

    fn next_frame(&mut self) -> Option<String> {
        let (pos, sep_len) = self
            .buffer
            .windows(2)
            .position(|w| w == b"\n\n")
            .map(|p| (p, 2))
            .or_else(|| {
                self.buffer
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|p| (p, 4))
            })?;

        let frame = self.buffer.drain(..pos + sep_len).collect::<Vec<_>>();
        Some(String::from_utf8_lossy(&frame[..frame.len() - sep_len]).into_owned())
    }
}

fn parse_frame(frame: &str) -> Result<Option<AnthropicStreamEvent>, ProviderError> {
    let trimmed = frame.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let mut data_lines = Vec::new();
    let mut event_name: Option<&str> = None;

    for line in trimmed.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(name) = line.strip_prefix("event:") {
            event_name = Some(name.trim());
        } else if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
        }
    }

    if matches!(event_name, Some("ping")) || data_lines.is_empty() {
        return Ok(None);
    }

    let payload = data_lines.join("\n");
    if payload == "[DONE]" {
        return Ok(None);
    }

    serde_json::from_str(&payload)
        .map(Some)
        .map_err(|e| ProviderError::InvalidSse(format!("failed to parse SSE payload: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_delta_frame() {
        let frame = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}"
        );
        let event = parse_frame(frame)
            .expect("should parse")
            .expect("should have event");
        assert!(matches!(
            event,
            AnthropicStreamEvent::ContentBlockDelta { .. }
        ));
    }

    #[test]
    fn handles_chunked_stream() {
        let mut parser = SseParser::default();
        let part1 = b"event: message_stop\ndata: {\"type\":\"mes";
        let part2 = b"sage_stop\"}\n\n";

        assert!(parser.push(part1).expect("push 1").is_empty());
        let events = parser.push(part2).expect("push 2");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            AnthropicStreamEvent::MessageStop { .. }
        ));
    }

    #[test]
    fn ignores_ping_and_done() {
        let mut parser = SseParser::default();
        let payload = concat!("event: ping\ndata: {}\n\n", "data: [DONE]\n\n",);
        let events = parser.push(payload.as_bytes()).expect("should parse");
        assert!(events.is_empty());
    }
}
