use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarkdownBlock {
    Heading {
        level: usize,
        text: String,
        raw: String,
    },
    Paragraph {
        text: String,
        raw: String,
    },
    List {
        ordered: bool,
        items: Vec<String>,
        item_markups: Vec<String>,
        raw: String,
    },
    Table {
        headers: Vec<String>,
        header_markups: Vec<String>,
        rows: Vec<Vec<String>>,
        row_markups: Vec<Vec<String>>,
        raw: String,
    },
    CodeFence {
        language: String,
        code: String,
        raw: String,
    },
    HorizontalRule,
}

pub(crate) fn parse_markdown_blocks(markup: &str) -> Vec<MarkdownBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    let mut parser = Parser::new_ext(markup, options)
        .into_offset_iter()
        .peekable();
    let mut blocks = Vec::new();

    while let Some((event, range)) = parser.next() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let (text, end_offset) =
                    collect_plain_text_until_with_end(&mut parser, TagEnd::Heading(level));
                blocks.push(MarkdownBlock::Heading {
                    level: heading_level(level),
                    text: collapse_ws(&text),
                    raw: markup
                        .get(range.start..end_offset)
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                });
            }
            Event::Start(Tag::Paragraph) => {
                let (text, end_offset) =
                    collect_plain_text_until_with_end(&mut parser, TagEnd::Paragraph);
                if !text.trim().is_empty() {
                    blocks.push(MarkdownBlock::Paragraph {
                        text: collapse_ws(&text),
                        raw: markup
                            .get(range.start..end_offset)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                    });
                }
            }
            Event::Start(Tag::List(first_number)) => {
                let ordered = first_number.is_some();
                let mut items = Vec::new();
                let mut item_markups = Vec::new();
                let mut end_offset = range.end;
                while let Some((next, next_range)) = parser.next() {
                    end_offset = next_range.end;
                    match next {
                        Event::Start(Tag::Item) => {
                            let (item, item_end_offset) =
                                collect_plain_text_until_with_end(&mut parser, TagEnd::Item);
                            let item = collapse_ws(&item);
                            if !item.is_empty() {
                                let item_raw = markup
                                    .get(next_range.start..item_end_offset)
                                    .unwrap_or("")
                                    .trim()
                                    .to_string();
                                items.push(item);
                                item_markups.push(if item_raw.is_empty() {
                                    items.last().cloned().unwrap_or_default()
                                } else {
                                    strip_list_item_marker(&item_raw, ordered)
                                });
                            }
                            end_offset = end_offset.max(item_end_offset);
                        }
                        Event::End(TagEnd::List(_)) => break,
                        _ => {}
                    }
                }
                if !items.is_empty() {
                    blocks.push(MarkdownBlock::List {
                        ordered,
                        items,
                        item_markups,
                        raw: markup
                            .get(range.start..end_offset)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                    });
                }
            }
            Event::Rule => blocks.push(MarkdownBlock::HorizontalRule),
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                };
                let mut code = String::new();
                let mut end_offset = range.end;
                for (next, next_range) in parser.by_ref() {
                    end_offset = next_range.end;
                    match next {
                        Event::Text(text) => code.push_str(&text),
                        Event::Code(text) => code.push_str(&text),
                        Event::SoftBreak | Event::HardBreak => code.push('\n'),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                blocks.push(MarkdownBlock::CodeFence {
                    language,
                    code,
                    raw: markup
                        .get(range.start..end_offset)
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                });
            }
            Event::Start(Tag::Table(_)) => {
                let mut headers = Vec::new();
                let mut header_markups = Vec::new();
                let mut rows = Vec::new();
                let mut row_markups = Vec::new();
                let mut current_row: Vec<(String, String)> = Vec::new();
                let mut end_offset = range.end;
                while let Some((next, next_range)) = parser.next() {
                    end_offset = next_range.end;
                    match next {
                        Event::Start(Tag::TableHead) => current_row.clear(),
                        Event::End(TagEnd::TableHead)
                            if headers.is_empty() && !current_row.is_empty() => {
                                headers =
                                    current_row.iter().map(|(text, _)| text.clone()).collect();
                                header_markups =
                                    current_row.iter().map(|(_, raw)| raw.clone()).collect();
                                current_row.clear();
                            }
                        Event::Start(Tag::TableRow) => current_row.clear(),
                        Event::End(TagEnd::TableRow) => {
                            if headers.is_empty() {
                                headers =
                                    current_row.iter().map(|(text, _)| text.clone()).collect();
                                header_markups =
                                    current_row.iter().map(|(_, raw)| raw.clone()).collect();
                            } else if !current_row.is_empty() {
                                rows.push(
                                    current_row.iter().map(|(text, _)| text.clone()).collect(),
                                );
                                row_markups
                                    .push(current_row.iter().map(|(_, raw)| raw.clone()).collect());
                            }
                        }
                        Event::Start(Tag::TableCell) => {
                            let (cell, cell_end_offset) =
                                collect_plain_text_until_with_end(&mut parser, TagEnd::TableCell);
                            let text = collapse_ws(&cell);
                            let raw = markup
                                .get(next_range.start..cell_end_offset)
                                .unwrap_or("")
                                .trim()
                                .to_string();
                            current_row.push((text, raw));
                            end_offset = end_offset.max(cell_end_offset);
                        }
                        Event::End(TagEnd::Table) => break,
                        _ => {}
                    }
                }
                if !headers.is_empty() || !rows.is_empty() {
                    blocks.push(MarkdownBlock::Table {
                        headers,
                        header_markups,
                        rows,
                        row_markups,
                        raw: markup
                            .get(range.start..end_offset)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                    });
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
    let mut parser = Parser::new_ext(markup, options)
        .into_offset_iter()
        .peekable();
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
    for (event, _) in parser.by_ref() {
        match event {
            Event::End(tag_end) if tag_end == end => break,
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

fn collect_plain_text_until_with_end(
    parser: &mut std::iter::Peekable<pulldown_cmark::OffsetIter<'_>>,
    end: TagEnd,
) -> (String, usize) {
    let mut out = String::new();
    let mut end_offset = 0usize;
    for (event, range) in parser.by_ref() {
        end_offset = range.end;
        match event {
            Event::End(tag_end) if tag_end == end => break,
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    (out, end_offset)
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

fn strip_list_item_marker(raw: &str, ordered: bool) -> String {
    let trimmed = raw.trim_start();
    if ordered {
        let mut digit_count = 0usize;
        for ch in trimmed.chars() {
            if ch.is_ascii_digit() {
                digit_count += 1;
            } else {
                break;
            }
        }
        if digit_count > 0 {
            let rest = &trimmed[digit_count..];
            if let Some(stripped) = rest.strip_prefix('.') {
                return stripped.trim_start().to_string();
            }
        }
        return trimmed.to_string();
    }
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        rest.to_string()
    } else {
        trimmed.to_string()
    }
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
            Some(MarkdownBlock::Heading { level: 1, text, .. }) if text == "Title"
        ));
        assert!(blocks.iter().any(
            |b| matches!(b, MarkdownBlock::List { ordered: false, items, item_markups, .. } if items.len() == 2 && item_markups.first().is_some_and(|raw| !raw.starts_with('-')))
        ));
        assert!(blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::Table { headers, rows, .. } if headers.len() == 2 && rows.len() == 1)));
        assert!(
            blocks.iter().any(
                |b| matches!(b, MarkdownBlock::CodeFence { language, .. } if language == "rust")
            )
        );
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

    #[test]
    fn parse_list_item_markup_strips_markers() {
        let blocks = parse_markdown_blocks("1. **bold**\n2. `code`\n");
        let list = blocks
            .iter()
            .find_map(|block| match block {
                MarkdownBlock::List {
                    ordered: true,
                    item_markups,
                    ..
                } => Some(item_markups.clone()),
                _ => None,
            })
            .expect("ordered list block should exist");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], "**bold**");
        assert_eq!(list[1], "`code`");
    }

    #[test]
    fn parse_table_markups_preserve_per_cell_inline_source() {
        let blocks = parse_markdown_blocks(
            "| Name | Description |\n| --- | --- |\n| `show_header` | Show the table header |\n| `fixed_rows` | Number of fixed rows |\n",
        );
        let table = blocks
            .iter()
            .find_map(|block| match block {
                MarkdownBlock::Table {
                    headers,
                    header_markups,
                    rows,
                    row_markups,
                    ..
                } => Some((headers, header_markups, rows, row_markups)),
                _ => None,
            })
            .expect("table block should exist");

        let (headers, header_markups, rows, row_markups) = table;
        assert_eq!(
            headers,
            &vec!["Name".to_string(), "Description".to_string()]
        );
        assert_eq!(
            header_markups,
            &vec!["Name".to_string(), "Description".to_string()]
        );
        assert_eq!(
            rows.first().expect("first row"),
            &vec![
                "show_header".to_string(),
                "Show the table header".to_string()
            ]
        );
        assert_eq!(
            row_markups.first().expect("first row markups"),
            &vec![
                "`show_header`".to_string(),
                "Show the table header".to_string()
            ]
        );
    }
}
