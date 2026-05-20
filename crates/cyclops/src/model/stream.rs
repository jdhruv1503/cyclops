use bytes::{BufMut, Bytes, BytesMut};

#[derive(Debug, Default)]
pub struct SseFramer {
    pending: BytesMut,
    data_lines: Vec<Bytes>,
}

impl SseFramer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: Bytes) -> Vec<Bytes> {
        self.pending.extend_from_slice(&chunk);

        let mut frames = Vec::new();
        while let Some(line_end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.split_to(line_end + 1).freeze();
            line.truncate(line.len() - 1);
            if line.ends_with(b"\r") {
                line.truncate(line.len() - 1);
            }

            if let Some(frame) = self.process_line(line) {
                frames.push(frame);
            }
        }

        frames
    }

    fn process_line(&mut self, line: Bytes) -> Option<Bytes> {
        if line.is_empty() {
            return self.finish_event();
        }

        if line.starts_with(b":") {
            return None;
        }

        let (field, value) = match line.iter().position(|byte| *byte == b':') {
            Some(colon) => {
                let field = line.slice(..colon);
                let mut value = line.slice(colon + 1..);
                if value.starts_with(b" ") {
                    value = value.slice(1..);
                }
                (field, value)
            }
            None => (line.clone(), Bytes::new()),
        };

        if field.as_ref() == b"data" {
            self.data_lines.push(value);
        }

        None
    }

    fn finish_event(&mut self) -> Option<Bytes> {
        match self.data_lines.len() {
            0 => None,
            1 => self.data_lines.pop(),
            _ => {
                let payload_len = self.data_lines.iter().map(Bytes::len).sum::<usize>()
                    + self.data_lines.len()
                    - 1;
                let mut payload = BytesMut::with_capacity(payload_len);

                for (index, line) in self.data_lines.drain(..).enumerate() {
                    if index > 0 {
                        payload.put_u8(b'\n');
                    }
                    payload.extend_from_slice(&line);
                }

                Some(payload.freeze())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_byte_by_byte(input: &[u8]) -> Vec<Bytes> {
        let mut framer = SseFramer::new();
        let mut frames = Vec::new();

        for byte in input {
            frames.extend(framer.push(Bytes::copy_from_slice(&[*byte])));
        }

        frames
    }

    #[test]
    fn frames_recorded_openai_shape_sse_byte_by_byte() {
        let transcript = b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\
data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\r\n\r\n\
data: [DONE]\n\n";

        let frames = feed_byte_by_byte(transcript);

        assert_eq!(
            frames,
            vec![
                Bytes::from_static(
                    b"{\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n{\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}"
                ),
                Bytes::from_static(b"{\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}"),
                Bytes::from_static(b"[DONE]"),
            ]
        );
    }

    #[test]
    fn ignores_comments_and_non_data_fields() {
        let transcript = b": keepalive\n\
event: message\n\
id: 42\n\
data: first\n\
: ignored between data lines\n\
retry: 1000\n\
data: second\n\n";

        let frames = feed_byte_by_byte(transcript);

        assert_eq!(frames, vec![Bytes::from_static(b"first\nsecond")]);
    }

    #[test]
    fn waits_for_blank_line_before_emitting() {
        let mut framer = SseFramer::new();

        assert!(framer.push(Bytes::from_static(b"data: partial")).is_empty());
        assert!(framer.push(Bytes::from_static(b"\n")).is_empty());
        assert_eq!(
            framer.push(Bytes::from_static(b"\n")),
            vec![Bytes::from_static(b"partial")]
        );
    }

    #[test]
    fn supports_empty_data_lines_and_field_without_colon() {
        let transcript = b"data\n\
data:\n\
data: third\n\n";

        let frames = feed_byte_by_byte(transcript);

        assert_eq!(frames, vec![Bytes::from_static(b"\n\nthird")]);
    }
}
