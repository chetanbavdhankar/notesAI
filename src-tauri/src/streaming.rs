use anyhow::{Context, Result};
use serde_json::Value;

#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    pub text: String,
    pub finished: bool,
    pub reason: Option<String>,
    pub thinking: bool,
}
impl Decoder {
    pub fn feed(&mut self, bytes: &[u8], native: bool, eof: bool) -> Result<Vec<String>> {
        self.buffer.extend_from_slice(bytes);
        anyhow::ensure!(
            self.buffer.len() < 2_000_000,
            "Provider stream frame exceeds 2 MB"
        );
        let mut tokens = vec![];
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let line = self.buffer.drain(..=end).collect::<Vec<_>>();
            if let Some(token) = self.line(&line, native)? {
                tokens.push(token)
            }
        }
        if eof && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            if let Some(token) = self.line(&line, native)? {
                tokens.push(token)
            }
        }
        Ok(tokens)
    }
    fn line(&mut self, line: &[u8], native: bool) -> Result<Option<String>> {
        let line = std::str::from_utf8(line)?.trim();
        if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
            return Ok(None);
        }
        let data = if native {
            line
        } else if let Some(data) = line.strip_prefix("data:") {
            data.trim()
        } else if line.starts_with('{') {
            line
        } else {
            return Ok(None);
        };
        if data == "[DONE]" {
            self.finished = true;
            return Ok(None);
        }
        let value: Value =
            serde_json::from_str(data).context("Provider returned malformed streaming JSON")?;
        anyhow::ensure!(
            value.get("error").is_none(),
            "Provider reported a generation error; check the model and server logs"
        );
        let (content, reasoning) = if native {
            if value["done"].as_bool() == Some(true) {
                self.finished = true;
                self.reason = value["done_reason"].as_str().map(str::to_owned);
            }
            (
                value["message"]["content"].as_str(),
                value["message"]["thinking"].as_str(),
            )
        } else {
            let choice = &value["choices"][0];
            if let Some(reason) = choice["finish_reason"].as_str() {
                self.finished = true;
                self.reason = Some(reason.into())
            }
            (
                choice["delta"]["content"]
                    .as_str()
                    .or_else(|| choice["message"]["content"].as_str()),
                choice["delta"]["reasoning"]
                    .as_str()
                    .or_else(|| choice["delta"]["reasoning_content"].as_str()),
            )
        };
        if reasoning.is_some_and(|s| !s.is_empty()) {
            self.thinking = true;
        }
        if let Some(token) = content.filter(|s| !s.is_empty()) {
            self.text.push_str(token);
            return Ok(Some(token.to_owned()));
        }
        Ok(None)
    }
    pub fn validate_completion(&self) -> Result<()> {
        anyhow::ensure!(
            self.finished,
            "Connection ended before the model finished. Any text shown is partial; please retry."
        );
        if self.text.trim().is_empty() {
            if self.thinking {
                anyhow::bail!("The model used its output budget for thinking without producing an answer. Try a non-reasoning model or a larger context window.")
            }
            anyhow::bail!("The model finished without answer text. Check that the selected model supports chat, then retry.")
        }
        anyhow::ensure!(self.reason.as_deref()!=Some("length"),"The answer reached the model's output limit. The text shown is partial; ask a narrower question or increase the context window.");
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_handles_unicode_fragmentation_and_final_line_without_newline() {
        let mut d = Decoder::default();
        let bytes="{\"message\":{\"content\":\"Café [^1]\"},\"done\":false}\n{\"message\":{\"content\":\"\"},\"done\":true,\"done_reason\":\"stop\"}".as_bytes();
        for b in bytes {
            d.feed(&[*b], true, false).unwrap();
        }
        d.feed(&[], true, true).unwrap();
        assert_eq!(d.text, "Café [^1]");
        d.validate_completion().unwrap();
    }
    #[test]
    fn reasoning_only_length_is_not_an_answer() {
        let mut d = Decoder::default();
        d.feed(b"data: {\"choices\":[{\"delta\":{\"reasoning\":\"private trace\"},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]",false,true).unwrap();
        assert!(d.text.is_empty());
        assert!(d
            .validate_completion()
            .unwrap_err()
            .to_string()
            .contains("thinking"));
    }
    #[test]
    fn unterminated_stream_remains_an_error() {
        let mut d = Decoder::default();
        d.feed(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n",
            false,
            true,
        )
        .unwrap();
        assert!(d.validate_completion().is_err());
    }
}
