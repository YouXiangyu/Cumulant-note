use crate::services::vault::{ensure_markdown_path, resolve_existing_file, resolve_writable_file};
use crate::services::ServiceResult;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownDocument {
    pub relative_path: String,
    pub raw: String,
    pub content: String,
    pub preview_markdown: String,
    pub frontmatter: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownExport {
    pub relative_path: String,
    pub markdown: String,
}

pub struct MarkdownService;

impl MarkdownService {
    pub fn read(vault_path: &str, relative_path: &str) -> ServiceResult<MarkdownDocument> {
        ensure_markdown_path(relative_path)?;
        let path = resolve_existing_file(vault_path, relative_path)?;
        let raw = fs::read_to_string(path)?;
        let parsed = parse_markdown(&raw)?;
        Ok(MarkdownDocument {
            relative_path: relative_path.replace('\\', "/"),
            preview_markdown: parsed.content.clone(),
            raw,
            content: parsed.content,
            frontmatter: parsed.frontmatter,
        })
    }

    pub fn save(
        vault_path: &str,
        relative_path: &str,
        content: &str,
        frontmatter: Option<Value>,
    ) -> ServiceResult<MarkdownDocument> {
        ensure_markdown_path(relative_path)?;
        let path = resolve_writable_file(vault_path, relative_path)?;
        let sanitized = sanitize_file_frontmatter(frontmatter.unwrap_or(Value::Object(Map::new())));
        let raw = compose_markdown(&sanitized, content)?;
        fs::write(path, raw)?;
        Self::read(vault_path, relative_path)
    }

    pub fn export(vault_path: &str, relative_path: &str) -> ServiceResult<MarkdownExport> {
        let document = Self::read(vault_path, relative_path)?;
        let frontmatter = sanitize_export_frontmatter(document.frontmatter);
        let markdown = compose_markdown(&frontmatter, &document.content)?;
        Ok(MarkdownExport {
            relative_path: relative_path.replace('\\', "/"),
            markdown,
        })
    }
}

#[derive(Debug)]
struct ParsedMarkdown {
    frontmatter: Value,
    content: String,
}

pub fn parse_markdown(raw: &str) -> ServiceResult<MarkdownDocumentParts> {
    let parsed = parse_parts(raw)?;
    Ok(MarkdownDocumentParts {
        frontmatter: parsed.frontmatter,
        content: parsed.content,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownDocumentParts {
    pub frontmatter: Value,
    pub content: String,
}

fn parse_parts(raw: &str) -> ServiceResult<ParsedMarkdown> {
    if !raw.starts_with("---\n") && !raw.starts_with("---\r\n") {
        return Ok(ParsedMarkdown {
            frontmatter: Value::Object(Map::new()),
            content: raw.to_string(),
        });
    }

    let newline = if raw.starts_with("---\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let marker = format!("{newline}---{newline}");
    if let Some(end) = raw[3..].find(&marker) {
        let yaml_start = 3 + newline.len();
        let yaml_end = 3 + end;
        let yaml = &raw[yaml_start..yaml_end];
        let content_start = yaml_end + marker.len();
        let frontmatter = if yaml.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml)?;
            serde_json::to_value(yaml_value).unwrap_or(Value::Object(Map::new()))
        };
        Ok(ParsedMarkdown {
            frontmatter,
            content: raw[content_start..].to_string(),
        })
    } else {
        Ok(ParsedMarkdown {
            frontmatter: Value::Object(Map::new()),
            content: raw.to_string(),
        })
    }
}

pub fn compose_markdown(frontmatter: &Value, content: &str) -> ServiceResult<String> {
    let Some(map) = frontmatter.as_object() else {
        return Ok(content.to_string());
    };
    if map.is_empty() {
        return Ok(content.to_string());
    }

    let yaml = serde_yaml::to_string(frontmatter)?;
    Ok(format!("---\n{}\n---\n{}", yaml.trim_end(), content))
}

pub fn sanitize_file_frontmatter(frontmatter: Value) -> Value {
    let allowed = allowed_yaml_fields();
    let mut output = Map::new();
    if let Some(map) = frontmatter.as_object() {
        for (key, value) in map {
            if allowed.contains(key.as_str()) {
                output.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(output)
}

pub fn sanitize_export_frontmatter(frontmatter: Value) -> Value {
    let mut output = Map::new();
    if let Some(map) = sanitize_file_frontmatter(frontmatter).as_object() {
        for (key, value) in map {
            if key != "thebrain_id" && !key.starts_with("thebrain_") {
                output.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(output)
}

fn allowed_yaml_fields() -> BTreeSet<&'static str> {
    [
        "title",
        "tags",
        "aliases",
        "created",
        "updated",
        "source",
        "source_type",
        "status",
        "classification",
        "thebrain_id",
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vault::VaultService;
    use serde_json::json;

    #[test]
    fn preview_hides_frontmatter_and_export_filters_internal_fields() {
        let temp = tempfile::tempdir().unwrap();
        VaultService::init(temp.path().to_str().unwrap()).unwrap();
        let folder = temp.path().join("100-School");
        std::fs::create_dir_all(&folder).unwrap();

        let saved = MarkdownService::save(
            temp.path().to_str().unwrap(),
            "100-School/笔记.md",
            "# 标题\n正文",
            Some(json!({
                "title": "笔记",
                "tags": ["school"],
                "thebrain_id": "tb-1",
                "token_usage": 999
            })),
        )
        .unwrap();

        assert_eq!(saved.preview_markdown, "# 标题\n正文");
        assert!(saved.raw.contains("thebrain_id"));
        assert!(!saved.raw.contains("token_usage"));

        let exported =
            MarkdownService::export(temp.path().to_str().unwrap(), "100-School/笔记.md").unwrap();
        assert!(exported.markdown.contains("title:"));
        assert!(!exported.markdown.contains("thebrain_id"));
        assert!(!exported.markdown.contains("token_usage"));
        assert!(exported.markdown.contains("# 标题"));
    }
}
