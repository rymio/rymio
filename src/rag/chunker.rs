/// Configuration for content chunking.
pub struct ChunkConfig {
    /// Number of lines per chunk.
    pub chunk_lines: usize,
    /// Number of overlapping lines between consecutive chunks.
    pub overlap_lines: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            chunk_lines: 50,
            overlap_lines: 10,
        }
    }
}

/// A chunk of file content with positional metadata.
pub struct Chunk {
    /// The text content of this chunk.
    pub content: String,
    /// The 1-indexed starting line number.
    pub line_start: usize,
    /// The 1-indexed ending line number (inclusive).
    pub line_end: usize,
    /// The 0-indexed chunk sequence number.
    pub chunk_index: usize,
}

/// Splits file content into overlapping line-based chunks.
///
/// The algorithm:
/// 1. Split content by newlines
/// 2. If empty content, return empty vec
/// 3. Walk through lines with a step of (chunk_lines - overlap_lines)
/// 4. Each chunk takes `chunk_lines` lines starting from the current position
/// 5. The last chunk extends to the end of the file
/// 6. line_start is 1-indexed (first line is line 1)
/// 7. chunk_index starts at 0
pub fn chunk_file_content(content: &str, config: &ChunkConfig) -> Vec<Chunk> {
    if content.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    if total_lines == 0 {
        return Vec::new();
    }

    // Ensure chunk_lines is at least 1 to avoid infinite loops
    let chunk_lines = config.chunk_lines.max(1);
    // Ensure overlap is less than chunk_lines to guarantee forward progress
    let overlap_lines = config.overlap_lines.min(chunk_lines.saturating_sub(1));
    let step = chunk_lines - overlap_lines;

    let mut chunks = Vec::new();
    let mut pos = 0;
    let mut chunk_index = 0;

    while pos < total_lines {
        let end = if pos + chunk_lines >= total_lines {
            // Last chunk extends to end of file
            total_lines
        } else {
            pos + chunk_lines
        };

        let chunk_content = lines[pos..end].join("\n");

        chunks.push(Chunk {
            content: chunk_content,
            line_start: pos + 1, // 1-indexed
            line_end: end,       // 1-indexed (inclusive)
            chunk_index,
        });

        // If this chunk already reaches the end, we're done
        if end == total_lines {
            break;
        }

        pos += step;
        chunk_index += 1;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content_returns_empty_vec() {
        let config = ChunkConfig::default();
        let result = chunk_file_content("", &config);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_line_returns_one_chunk() {
        let config = ChunkConfig::default();
        let result = chunk_file_content("hello world", &config);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "hello world");
        assert_eq!(result[0].line_start, 1);
        assert_eq!(result[0].line_end, 1);
        assert_eq!(result[0].chunk_index, 0);
    }

    #[test]
    fn test_content_shorter_than_chunk_lines_returns_one_chunk() {
        let config = ChunkConfig {
            chunk_lines: 50,
            overlap_lines: 10,
        };
        let content = "line1\nline2\nline3";
        let result = chunk_file_content(content, &config);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "line1\nline2\nline3");
        assert_eq!(result[0].line_start, 1);
        assert_eq!(result[0].line_end, 3);
        assert_eq!(result[0].chunk_index, 0);
    }

    #[test]
    fn test_exact_chunk_lines_returns_one_chunk() {
        let config = ChunkConfig {
            chunk_lines: 3,
            overlap_lines: 1,
        };
        let content = "line1\nline2\nline3";
        let result = chunk_file_content(content, &config);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line_start, 1);
        assert_eq!(result[0].line_end, 3);
        assert_eq!(result[0].chunk_index, 0);
    }

    #[test]
    fn test_multiple_chunks_with_overlap() {
        let config = ChunkConfig {
            chunk_lines: 3,
            overlap_lines: 1,
        };
        // 5 lines: chunk_lines=3, overlap=1, step=2
        // Chunk 0: lines 0..3 (lines 1-3)
        // Chunk 1: lines 2..5 (lines 3-5)
        let content = "a\nb\nc\nd\ne";
        let result = chunk_file_content(content, &config);
        assert_eq!(result.len(), 2);

        assert_eq!(result[0].content, "a\nb\nc");
        assert_eq!(result[0].line_start, 1);
        assert_eq!(result[0].line_end, 3);
        assert_eq!(result[0].chunk_index, 0);

        assert_eq!(result[1].content, "c\nd\ne");
        assert_eq!(result[1].line_start, 3);
        assert_eq!(result[1].line_end, 5);
        assert_eq!(result[1].chunk_index, 1);
    }

    #[test]
    fn test_chunk_indices_are_sequential() {
        let config = ChunkConfig {
            chunk_lines: 2,
            overlap_lines: 0,
        };
        let content = "a\nb\nc\nd\ne\nf";
        let result = chunk_file_content(content, &config);
        for (i, chunk) in result.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i);
        }
    }

    #[test]
    fn test_last_chunk_extends_to_end() {
        let config = ChunkConfig {
            chunk_lines: 3,
            overlap_lines: 1,
        };
        // 7 lines: step=2
        // Chunk 0: lines 0..3 (1-3)
        // Chunk 1: lines 2..5 (3-5)
        // Chunk 2: lines 4..7 (5-7) - extends to end
        let content = "a\nb\nc\nd\ne\nf\ng";
        let result = chunk_file_content(content, &config);
        let last = result.last().unwrap();
        assert_eq!(last.line_end, 7);
        assert_eq!(last.content, "e\nf\ng");
    }

    #[test]
    fn test_first_chunk_starts_at_line_1() {
        let config = ChunkConfig::default();
        let content = "first\nsecond\nthird";
        let result = chunk_file_content(content, &config);
        assert_eq!(result[0].line_start, 1);
    }
}
