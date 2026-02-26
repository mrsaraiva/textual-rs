use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarkdownBlock {
    Heading { level: usize, text: String },
    Paragraph { text: String },
    List { ordered: bool, items: Vec<String> },
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    CodeFence { language: String, code: String },
    HorizontalRule,
}

pub(crate) fn parse_markdown_blocks(markup: &str) -> Vec<MarkdownBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    let mut parser = Parser::new_ext(markup, options).into_offset_iter().peekable();
    let mut blocks = Vec::new();

    while let Some((event, _)) = parser.next() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let text = collect_plain_text_until(&mut parser, TagEnd::Heading(level));
                blocks.push(MarkdownBlock::Heading {
                    level: heading_level(level),
                    text: collapse_ws(&text),
                });
            }
            Event::Start(Tag::Paragraph) => {
                let text = collect_plain_text_until(&mut parser, TagEnd::Paragraph);
                if !text.trim().is_empty() {
                    blocks.push(MarkdownBlock::Paragraph {
                        text: collapse_ws(&text),
                    });
                }
            }
            Event::Start(Tag::List(first_number)) => {
                let ordered = first_number.is_some();
                let mut items = Vec::new();
                while let Some((next, _)) = parser.next() {
                    match next {
                        Event::Start(Tag::Item) => {
                            let item = collect_plain_text_until(&mut parser, TagEnd::Item);
                            let item = collapse_ws(&item);
                            if !item.is_empty() {
                                items.push(item);
                            }
                        }
                        Event::End(TagEnd::List(_)) => break,
                        _ => {}
                    }
                }
                if !items.is_empty() {
                    blocks.push(MarkdownBlock::List { ordered, items });
                }
            }
            Event::Rule => blocks.push(MarkdownBlock::HorizontalRule),
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                };
                let mut code = String::new();
                while let Some((next, _)) = parser.next() {
                    match next {
                        Event::Text(text) => code.push_str(&text),
                        Event::Code(text) => code.push_str(&text),
                        Event::SoftBreak | Event::HardBreak => code.push('\n'),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                blocks.push(MarkdownBlock::CodeFence { language, code });
            }
            Event::Start(Tag::Table(_)) => {
                let mut headers = Vec::new();
                let mut rows = Vec::new();
                let mut current_row: Vec<String> = Vec::new();
                while let Some((next, _)) = parser.next() {
                    match next {
                        Event::Start(Tag::TableHead) => current_row.clear(),
                        Event::End(TagEnd::TableHead) => {
                            if headers.is_empty() && !current_row.is_empty() {
                                headers = current_row.clone();
                                current_row.clear();
                            }
                        }
                        Event::Start(Tag::TableRow) => current_row.clear(),
                        Event::End(TagEnd::TableRow) => {
                            if headers.is_empty() {
                                headers = current_row.clone();
                            } else if !current_row.is_empty() {
                                rows.push(current_row.clone());
                            }
                        }
                        Event::Start(Tag::TableCell) => {
                            let cell = collect_plain_text_until(&mut parser, TagEnd::TableCell);
                            current_row.push(collapse_ws(&cell));
                        }
                        Event::End(TagEnd::Table) => break,
                        _ => {}
                    }
                }
                if !headers.is_empty() || !rows.is_empty() {
                    blocks.push(MarkdownBlock::Table { headers, rows });
                }
            }
            _ => {}
        }
    }

    blocks
}

pub(crate) fn parse_markdown_headings(markup: &str) -> Vec<(usize, String)> {
    parse_markdown_headings_with_lines(markup)
        .into_iter()
        .map(|(level, text, _line)| (level, text))
        .collect()
}

pub(crate) fn parse_markdown_headings_with_lines(markup: &str) -> Vec<(usize, String, usize)> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    let mut parser = Parser::new_ext(markup, options).into_offset_iter().peekable();
    let mut headings = Vec::new();

    while let Some((event, range)) = parser.next() {
        if let Event::Start(Tag::Heading { level, .. }) = event {
            let text = collect_plain_text_until(&mut parser, TagEnd::Heading(level));
            let heading = collapse_ws(&text);
            if !heading.is_empty() {
                headings.push((
                    heading_level(level),
                    heading,
                    line_index_for_offset(markup, range.start),
                ));
            }
        }
    }

    headings
}

fn line_index_for_offset(markup: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(markup.len());
    markup[..clamped].bytes().filter(|b| *b == b'\n').count()
}

fn collect_plain_text_until(
    parser: &mut std::iter::Peekable<pulldown_cmark::OffsetIter<'_>>,
    end: TagEnd,
) -> String {
    let mut out = String::new();
    while let Some((event, _)) = parser.next() {
        match event {
            Event::End(tag_end) if tag_end == end => break,
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

fn heading_level(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn collapse_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{MarkdownBlock, parse_markdown_blocks, parse_markdown_headings_with_lines};

    #[test]
    fn parse_headings_lists_and_table() {
        let blocks = parse_markdown_blocks(
            r#"# Title

Some text here.

- one
- two

| Name | Value |
| --- | --- |
| a | 1 |

```rust
fn x() {}
```
"#,
        );
        assert!(matches!(
            blocks.first(),
            Some(MarkdownBlock::Heading { level: 1, text }) if text == "Title"
        ));
        assert!(blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::List { ordered: false, items } if items.len() == 2)));
        assert!(blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::Table { headers, rows } if headers.len() == 2 && rows.len() == 1)));
        assert!(blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::CodeFence { language, .. } if language == "rust")));
    }

    #[test]
    fn parse_headings_reports_line_indices() {
        let headings = parse_markdown_headings_with_lines("# A\nx\n## B\n\n### C\n");
        assert_eq!(
            headings,
            vec![
                (1, "A".to_string(), 0),
                (2, "B".to_string(), 2),
                (3, "C".to_string(), 4)
            ]
        );
    }
}
