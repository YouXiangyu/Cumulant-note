use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextChunk {
    pub chunk_index: usize,
    pub heading_path: Vec<String>,
    pub content: String,
    pub snippet: String,
    pub start_line: usize,
    pub end_line: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub char_count: usize,
    pub token_estimate: usize,
}

#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    pub max_chars: usize,
    pub min_chars: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            max_chars: 1_600,
            min_chars: 280,
        }
    }
}

pub struct StructureAwareChunker {
    config: ChunkerConfig,
}

impl Default for StructureAwareChunker {
    fn default() -> Self {
        Self {
            config: ChunkerConfig::default(),
        }
    }
}

impl StructureAwareChunker {
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    pub fn chunk(&self, content: &str) -> Vec<TextChunk> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_heading_path = Vec::new();
        let mut active_heading_path = Vec::new();
        let mut start_line = 1usize;
        let mut end_line = 1usize;
        let mut char_start = 0usize;
        let mut char_end = 0usize;
        let mut line_char_offset = 0usize;
        let mut in_code_fence = false;

        for (line_index, line) in content.lines().enumerate() {
            let line_number = line_index + 1;
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code_fence = !in_code_fence;
            }

            if !in_code_fence {
                if let Some((level, title)) = parse_heading(line) {
                    if !current.trim().is_empty() {
                        push_chunk(
                            &mut chunks,
                            &mut current,
                            &current_heading_path,
                            start_line,
                            end_line,
                            char_start,
                            char_end,
                        );
                    }
                    update_heading_path(&mut active_heading_path, level, title);
                }
            }

            if current.is_empty() {
                start_line = line_number;
                char_start = line_char_offset;
                current_heading_path = active_heading_path.clone();
            }
            current.push_str(line);
            current.push('\n');
            end_line = line_number;
            char_end = line_char_offset + line.chars().count();

            if !in_code_fence
                && current.chars().count() >= self.config.max_chars
                && current.chars().count() >= self.config.min_chars
            {
                push_chunk(
                    &mut chunks,
                    &mut current,
                    &current_heading_path,
                    start_line,
                    end_line,
                    char_start,
                    char_end,
                );
            }

            line_char_offset += line.chars().count() + 1;
        }

        if !current.trim().is_empty() {
            push_chunk(
                &mut chunks,
                &mut current,
                &current_heading_path,
                start_line,
                end_line,
                char_start,
                char_end,
            );
        }

        if chunks.is_empty() && !content.trim().is_empty() {
            let trimmed = content.trim().to_string();
            chunks.push(TextChunk {
                chunk_index: 0,
                heading_path: Vec::new(),
                snippet: snippet(&trimmed),
                token_estimate: estimate_tokens(&trimmed),
                char_count: trimmed.chars().count(),
                char_end: trimmed.chars().count(),
                content: trimmed,
                start_line: 1,
                end_line: content.lines().count().max(1),
                char_start: 0,
            });
        }

        for (index, chunk) in chunks.iter_mut().enumerate() {
            chunk.chunk_index = index;
        }
        chunks
    }
}

pub fn chunk_markdown(content: &str) -> Vec<TextChunk> {
    StructureAwareChunker::default().chunk(content)
}

fn push_chunk(
    chunks: &mut Vec<TextChunk>,
    current: &mut String,
    heading_path: &[String],
    start_line: usize,
    end_line: usize,
    char_start: usize,
    char_end: usize,
) {
    let content = current.trim().to_string();
    current.clear();
    if content.is_empty() {
        return;
    }
    chunks.push(TextChunk {
        chunk_index: chunks.len(),
        heading_path: heading_path.to_vec(),
        snippet: snippet(&content),
        start_line,
        end_line,
        char_start,
        char_end,
        char_count: content.chars().count(),
        token_estimate: estimate_tokens(&content),
        content,
    });
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|char| *char == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = trimmed[level..].trim();
    if rest.is_empty() || !trimmed[level..].starts_with(' ') {
        return None;
    }
    Some((level, rest.trim_matches('#').trim().to_string()))
}

fn update_heading_path(path: &mut Vec<String>, level: usize, title: String) {
    if path.len() >= level {
        path.truncate(level - 1);
    }
    path.push(title);
}

fn snippet(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(220).collect()
}

fn estimate_tokens(content: &str) -> usize {
    (content.chars().count() / 4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_preserve_markdown_heading_path() {
        let chunks = chunk_markdown("# 课程\n\n正文\n\n## 线性代数\n\n矩阵和特征值\n");

        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].heading_path, vec!["课程"]);
        assert_eq!(chunks[1].heading_path, vec!["课程", "线性代数"]);
        assert!(chunks[1].content.contains("特征值"));
    }

    #[test]
    fn code_fence_heading_does_not_split_structure() {
        let chunks = chunk_markdown("# Root\n\n```md\n# Not heading\n```\n\ntext");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path, vec!["Root"]);
        assert!(chunks[0].content.contains("# Not heading"));
    }
}
