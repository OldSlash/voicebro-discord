//! What the voice reads of an answer (change `whisper-mode`, design D7): the streamed text
//! cut into pieces at sentence ends, with what a voice should not read removed.
//!
//! - A piece ends at `.`, `!`, `?` or `…` followed by a space, or at a line break; the
//!   first piece of an answer also at the first comma once it has 20 characters, so the
//!   first audio starts sooner.
//! - A piece shorter than 20 characters waits for the next one; a piece over 300
//!   characters is cut at its last comma, semicolon or space.
//! - Code blocks and table rows are dropped (the chat still has them); `[text](url)` reads
//!   the text, a bare URL its host; heading, list, quote and emphasis markers and emoji go.

/// Pieces shorter than this wait for the next sentence.
pub const MIN_PIECE: usize = 20;
/// Pieces longer than this are cut.
pub const MAX_PIECE: usize = 300;

/// The line-level state of the cleaning, carried from one chunk to the next.
#[derive(Debug, Clone, Default)]
struct Lines {
    in_fence: bool,
    /// Inside a line that is dropped whole (a fence line or a table row).
    skipping: bool,
    /// The next character starts a line.
    mid_line: bool,
}

fn is_emoji(character: char) -> bool {
    matches!(u32::from(character),
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE0F | 0x200D | 0x20E3
            | 0xE0020..=0xE007F)
}

/// The host of a URL (`https://www.example.com/a?b` → `example.com`).
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host)
}

/// Inline cleaning of a part of one line.
fn clean_inline(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let rest: String = characters[index..characters.len().min(index + 8)]
            .iter()
            .collect();
        // [text](url) and ![alt](url): the text.
        if character == '[' || (character == '!' && characters.get(index + 1) == Some(&'[')) {
            let open = if character == '!' { index + 1 } else { index };
            if let Some(close) = characters[open..].iter().position(|&c| c == ']') {
                let close = open + close;
                if characters.get(close + 1) == Some(&'(') {
                    if let Some(end) = characters[close + 1..].iter().position(|&c| c == ')') {
                        let label: String = characters[open + 1..close].iter().collect();
                        out.push_str(&clean_inline(&label));
                        index = close + 1 + end + 1;
                        continue;
                    }
                }
            }
        }
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = characters[index..]
                .iter()
                .position(|c| c.is_whitespace() || matches!(c, ')' | '>' | '"'))
                .map_or(characters.len(), |end| index + end);
            let url: String = characters[index..end].iter().collect();
            let url = url.trim_end_matches(['.', ',', ';', ':', '!', '?']);
            out.push_str(host_of(url));
            index += url.chars().count();
            continue;
        }
        if matches!(character, '*' | '`' | '~') || is_emoji(character) {
            index += 1;
            continue;
        }
        // `__bold__` and `_emphasis_`; an underscore inside a word stays (snake_case).
        if character == '_' {
            let after_word = out.chars().last().is_some_and(char::is_alphanumeric);
            let before_word = characters
                .get(index + 1)
                .is_some_and(|next| next.is_alphanumeric());
            if !(after_word && before_word) {
                index += 1;
                continue;
            }
        }
        out.push(character);
        index += 1;
    }
    out
}

/// The markers a line starts with: headings, quotes, list bullets and numbers.
fn strip_line_markers(line: &str) -> &str {
    let mut rest = line.trim_start();
    loop {
        let before = rest;
        rest = rest
            .trim_start_matches('#')
            .trim_start_matches('>')
            .trim_start();
        for bullet in ["- ", "* ", "+ ", "• "] {
            if let Some(stripped) = rest.strip_prefix(bullet) {
                rest = stripped;
            }
        }
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && digits < 4 {
            let after = &rest[digits..];
            if let Some(stripped) = after
                .strip_prefix(". ")
                .or_else(|| after.strip_prefix(") "))
            {
                rest = stripped;
            }
        }
        if rest == before {
            return rest;
        }
    }
}

impl Lines {
    /// Cleans a chunk of the answer; a chunk may end in the middle of a line.
    fn clean(&mut self, chunk: &str) -> String {
        let mut out = String::new();
        for part in chunk.split_inclusive('\n') {
            let ends_line = part.ends_with('\n');
            let body = part.trim_end_matches(['\n', '\r']);
            if !self.mid_line {
                let trimmed = body.trim_start();
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    self.in_fence = !self.in_fence;
                    self.skipping = true;
                } else {
                    self.skipping = self.in_fence || trimmed.starts_with('|');
                }
            }
            if !self.skipping {
                let text = if self.mid_line {
                    body
                } else {
                    strip_line_markers(body)
                };
                out.push_str(&clean_inline(text));
                if ends_line {
                    out.push(' ');
                }
            }
            self.mid_line = !ends_line;
            if ends_line {
                self.skipping = false;
            }
        }
        out
    }
}

/// Collapses whitespace and trims.
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a voice reads of a whole text.
pub fn speakable(text: &str) -> String {
    tidy(&Lines::default().clean(text))
}

/// Cuts a piece over [`MAX_PIECE`] characters at its last comma, semicolon or space before
/// the limit.
fn cut_long(piece: String) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut rest = piece;
    while rest.chars().count() > MAX_PIECE {
        let limit = rest
            .char_indices()
            .nth(MAX_PIECE)
            .map_or(rest.len(), |(index, _)| index);
        let head = &rest[..limit];
        let cut = head
            .rfind([',', ';'])
            .map(|index| index + 1)
            .or_else(|| head.rfind(' '))
            .filter(|&index| index > 0)
            .unwrap_or(limit);
        pieces.push(tidy(&rest[..cut]));
        rest = rest[cut..].trim_start().to_owned();
    }
    if !rest.trim().is_empty() {
        pieces.push(tidy(&rest));
    }
    pieces
}

/// Cuts one answer's streamed text into pieces for the voice. One per answer item.
#[derive(Debug, Default)]
pub struct SentenceSplitter {
    /// Raw text not yet cut.
    pending: String,
    /// Clean text of the sentences shorter than [`MIN_PIECE`], waiting for more.
    waiting: String,
    lines: Lines,
    /// Whether a piece was already given: the first one may end at a comma.
    started: bool,
}

impl SentenceSplitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The byte index just after the first boundary in `pending`, if any: a sentence end
    /// followed by whitespace or a line break; for the first piece, also a comma.
    fn boundary(&self) -> Option<usize> {
        let mut in_fence = self.lines.in_fence;
        let mut line_start = !self.lines.mid_line;
        let characters: Vec<(usize, char)> = self.pending.char_indices().collect();
        for (position, &(index, character)) in characters.iter().enumerate() {
            if line_start {
                let rest = self.pending[index..].trim_start_matches([' ', '\t']);
                if rest.starts_with("```") || rest.starts_with("~~~") {
                    in_fence = !in_fence;
                }
                line_start = false;
            }
            if character == '\n' {
                return Some(index + 1);
            }
            let &(_, next) = characters.get(position + 1)?;
            let ends =
                matches!(character, '.' | '!' | '?' | '…') || (character == ',' && !self.started);
            if !in_fence && ends && next.is_whitespace() {
                return Some(index + character.len_utf8());
            }
        }
        None
    }

    fn take(&mut self, raw: &str) -> Option<String> {
        let clean = self.lines.clean(raw);
        self.waiting.push(' ');
        self.waiting.push_str(&clean);
        let ready = tidy(&self.waiting);
        if ready.chars().count() < MIN_PIECE {
            return None;
        }
        self.waiting.clear();
        Some(ready)
    }

    /// The next part of the answer; returns the pieces it completed.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.pending.push_str(delta);
        let mut pieces = Vec::new();
        loop {
            let cut = match self.boundary() {
                Some(cut) => cut,
                // A long run without a boundary: cut it before it grows past the limit.
                None if !self.lines.in_fence
                    && self.pending.chars().count() > MAX_PIECE + MIN_PIECE =>
                {
                    let limit = self
                        .pending
                        .char_indices()
                        .nth(MAX_PIECE)
                        .map_or(self.pending.len(), |(index, _)| index);
                    self.pending[..limit]
                        .rfind([',', ';', ' '])
                        .map_or(limit, |index| index + 1)
                }
                None => break,
            };
            let raw: String = self.pending.drain(..cut).collect();
            if let Some(piece) = self.take(&raw) {
                self.started = true;
                pieces.extend(cut_long(piece));
            }
        }
        pieces
    }

    /// The answer item completed: whatever remains, of any length.
    pub fn finish(&mut self) -> Vec<String> {
        let raw = std::mem::take(&mut self.pending);
        let clean = self.lines.clean(&raw);
        self.waiting.push(' ');
        self.waiting.push_str(&clean);
        let rest = tidy(&std::mem::take(&mut self.waiting));
        self.started = false;
        self.lines = Lines::default();
        if rest.is_empty() {
            Vec::new()
        } else {
            cut_long(rest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_streamed(text: &str, step: usize) -> Vec<String> {
        let mut splitter = SentenceSplitter::new();
        let characters: Vec<char> = text.chars().collect();
        let mut pieces = Vec::new();
        for chunk in characters.chunks(step) {
            pieces.extend(splitter.push(&chunk.iter().collect::<String>()));
        }
        pieces.extend(splitter.finish());
        pieces
    }

    #[test]
    fn code_blocks_and_tables_are_not_read() {
        let text = "Here is the fix:\n```rust\nfn main() { println!(\"a. b\"); }\n```\nThat is all.\n\n| City | Time |\n|---|---|\n| London | 9:00. |\n\nDone.";
        assert_eq!(speakable(text), "Here is the fix: That is all. Done.");
    }

    #[test]
    fn links_read_their_text_and_bare_urls_their_host() {
        assert_eq!(
            speakable("See [the forecast](https://weather.example.com/bkk?d=1) or https://www.example.org/a/b."),
            "See the forecast or example.org."
        );
        assert_eq!(speakable("![a map](https://x.io/m.png) here"), "a map here");
    }

    #[test]
    fn markers_emphasis_and_emoji_go() {
        assert_eq!(
            speakable("## Weather\n> **Hot** and _humid_ 🌞\n- 33 °C\n2. rain at __night__ ✅"),
            "Weather Hot and humid 33 °C rain at night"
        );
    }

    #[test]
    fn the_first_piece_ends_at_the_first_comma_after_20_characters() {
        let pieces = split_streamed(
            "In Bangkok right now it is 33 degrees, sunny and humid, with rain later. In London it is 9 degrees, cloudy.",
            3,
        );
        assert_eq!(
            pieces,
            vec![
                "In Bangkok right now it is 33 degrees,",
                "sunny and humid, with rain later.",
                "In London it is 9 degrees, cloudy."
            ]
        );
    }

    #[test]
    fn short_sentences_wait_and_long_ones_are_cut() {
        let pieces = split_streamed("Yes. Sure. It is nine in London now. Ok.", 5);
        assert_eq!(pieces, vec!["Yes. Sure. It is nine in London now.", "Ok."]);
        let long = "word ".repeat(100);
        let pieces = split_streamed(&format!("{long}end."), 7);
        assert!(pieces.len() >= 2, "{pieces:?}");
        assert!(pieces
            .iter()
            .all(|piece| piece.chars().count() <= MAX_PIECE));
        assert_eq!(pieces.concat().matches("word").count(), 100);
    }

    #[test]
    fn a_code_block_streamed_in_pieces_is_skipped_whole() {
        let text = "Run this:\n```\nls -la. Then cd ..\nrm x. y\n```\nThen it is done, all of it.";
        let pieces = split_streamed(text, 2);
        assert_eq!(pieces, vec!["Run this: Then it is done,", "all of it."]);
        assert_eq!(
            speakable("call snake_case_name now"),
            "call snake_case_name now"
        );
    }

    #[test]
    fn numbers_and_versions_are_not_sentence_ends() {
        let pieces = split_streamed("Version 3.5 is out, priced at 9.99 dollars today. Nice!", 4);
        assert_eq!(
            pieces,
            vec!["Version 3.5 is out, priced at 9.99 dollars today.", "Nice!"]
        );
    }
}
