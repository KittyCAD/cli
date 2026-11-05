use std::fmt::Write;

use anyhow::Result;
use clap::Command;
use pulldown_cmark_to_cmark::cmark_with_options;

use crate::ml::copilot::state::slash_command_documentation;

struct MarkdownDocument<'a>(Vec<pulldown_cmark::Event<'a>>);

impl MarkdownDocument<'_> {
    fn header(&mut self, text: String, level: pulldown_cmark::HeadingLevel) {
        self.0.push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading {
            level,
            id: None,
            classes: vec![],
            attrs: vec![],
        }));
        self.0.push(pulldown_cmark::Event::Text(text.into()));
        self.0
            .push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Heading(level)));
    }

    fn paragraph(&mut self, text: String) {
        self.0
            .push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph));
        self.0.push(pulldown_cmark::Event::Text(text.into()));
        self.0
            .push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Paragraph));
    }

    fn link_in_list(&mut self, text: String, url: String) {
        let link = pulldown_cmark::Tag::Link {
            link_type: pulldown_cmark::LinkType::Inline,
            dest_url: url.into(),
            title: "".into(),
            id: "".into(),
        };

        self.0.push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::Item));
        self.0.push(pulldown_cmark::Event::Start(link));
        self.0.push(pulldown_cmark::Event::Text(text.into()));
        self.0.push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Link));
        self.0.push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Item));
    }
}

fn do_markdown(doc: &mut MarkdownDocument, app: &Command, title: &str) -> Result<()> {
    // We don't need the header since our renderer will do that for us.
    //doc.header(app.get_name().to_string(), pulldown_cmark::HeadingLevel::H2);

    if let Some(about) = app.get_about() {
        doc.paragraph(about.to_string());
    }

    let sub_commands = app
        .get_subcommands()
        .filter(|c| c.get_name() != "help")
        .collect::<Vec<&Command>>();
    if !sub_commands.is_empty() {
        doc.header("Subcommands".to_string(), pulldown_cmark::HeadingLevel::H3);

        doc.0
            .push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::List(None)));

        for cmd in sub_commands {
            doc.link_in_list(
                format!("{} {}", title, cmd.get_name()),
                format!("./{}_{}", title.replace(' ', "_"), cmd.get_name()),
            );
        }

        doc.0
            .push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::List(false)));
    }

    let args = app.get_arguments().collect::<Vec<&clap::Arg>>();
    if !args.is_empty() {
        doc.header("Options".to_string(), pulldown_cmark::HeadingLevel::H3);

        let mut html = "<dl class=\"flags\">\n".to_string();

        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                html.push('\n');
            }
            let mut def = String::new();

            if let Some(short) = arg.get_short() {
                def.push('-');
                def.push(short);
            }

            if let Some(long) = arg.get_long() {
                if arg.get_short().is_some() {
                    def.push('/');
                }
                def.push_str("--");
                def.push_str(long);
            }

            if arg.get_long().is_none() && arg.get_short().is_none() && !arg.get_id().as_str().is_empty() {
                def.push_str(arg.get_id().as_str());
            }

            let mut desc = arg
                .get_long_help()
                .unwrap_or_else(|| arg.get_help().unwrap_or_default())
                .to_string();
            desc = normalize_description_for_definition_list(&desc);

            // Check if the arg is an enum and if so, add the possible values.
            let possible_values = arg.get_possible_values();
            if !possible_values.is_empty() {
                desc.push_str("<br/>Possible values: <code>");
                for (i, value) in possible_values.iter().enumerate() {
                    if i > 0 {
                        desc.push_str(" | ");
                    }
                    desc.push_str(value.get_name());
                }
                desc.push_str("</code>");
            }

            let values = arg.get_default_values();
            if !values.is_empty() {
                desc.push_str("<br/>Default value: <code>");
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        desc.push_str(" | ");
                    }
                    let v = value.to_str().unwrap_or_default();
                    if !v.is_empty() {
                        desc.push_str(v);
                    }
                }
                desc.push_str("</code>");
            }

            write!(
                html,
                r#"   <dt><code>{def}</code></dt>
   <dd>{desc}</dd>
"#,
            )
            .unwrap_or_default();
        }

        html.push_str("</dl>\n\n");

        doc.0.push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::HtmlBlock));
        doc.0.push(pulldown_cmark::Event::Html(html.into()));
        doc.0
            .push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::HtmlBlock));
    }

    // TODO: add examples

    if let Some(about) = app.get_long_about() {
        doc.header("About".to_string(), pulldown_cmark::HeadingLevel::H3);
        let raw = about
            .to_string()
            .trim_start_matches(&app.get_about().map(|s| s.to_string()).unwrap_or_default())
            .trim_start_matches('.')
            .to_string();

        // We need to parse this as markdown so any code snippets denoted by 4 spaces
        // are rendered as code blocks. Which works better for our docs.
        let parser = pulldown_cmark::Parser::new(&raw);

        let mut result = String::new();
        cmark_with_options(parser, &mut result, get_cmark_options())?;

        doc.paragraph(result);
    }

    if app.get_name() == "copilot" && title.contains("ml kcl") {
        doc.header("Slash Commands".to_string(), pulldown_cmark::HeadingLevel::H3);
        doc.0
            .push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::List(None)));
        for entry in slash_command_documentation() {
            doc.0.push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::Item));
            doc.0.push(pulldown_cmark::Event::Code(entry.command.clone().into()));
            doc.0
                .push(pulldown_cmark::Event::Text(format!(" {}", entry.description).into()));
            doc.0.push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Item));
        }
        doc.0
            .push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::List(false)));
    }

    // Check if the command has a parent.
    let mut split = title.split(' ').collect::<Vec<&str>>();
    let first = format!("{} ", split.first().unwrap());
    if !(title == app.get_name() || title.trim_start_matches(&first) == app.get_name()) {
        doc.header("See also".to_string(), pulldown_cmark::HeadingLevel::H3);

        doc.0
            .push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::List(None)));

        // Get the parent command.
        // Iterate if more than one, thats why we have a list.
        if split.len() > 2 {
            // Remove the last element, since that is the command name.
            split.pop();

            for (i, _) in split.iter().enumerate() {
                if i < 1 {
                    // We don't care about the first command.
                    continue;
                }

                let mut p = split.clone();
                p.truncate(i + 1);
                let parent = p.join(" ");
                doc.link_in_list(parent.to_string(), format!("./{}", parent.replace(' ', "_")));
            }
        }

        doc.0
            .push(pulldown_cmark::Event::End(pulldown_cmark::TagEnd::List(false)));
    }

    Ok(())
}

fn normalize_description_for_definition_list(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn get_cmark_options() -> pulldown_cmark_to_cmark::Options<'static> {
    pulldown_cmark_to_cmark::Options {
        newlines_after_codeblock: 2,
        code_block_token_count: 3,
        ..Default::default()
    }
}

/// Convert rustdoc links to markdown links.
/// For example:
/// <https://example.com> -> [https://example.com](https://example.com)
/// <https://example.com/thing|Foo> -> [Foo](https://example.com/thing)
fn rustdoc_to_markdown_link(text: &str) -> Result<String> {
    let re = regex::Regex::new(r#"<(https?://[^>]+)>"#)?;
    Ok(re
        .replace_all(text, |caps: &regex::Captures| {
            let url = &caps[1];
            let text = url.split('|').nth(1).unwrap_or(url);
            format!("[{}]({})", text, url.split('|').next().unwrap_or(url))
        })
        .to_string())
}

#[derive(Debug, PartialEq, Eq)]
struct LanguageCodeBlock {
    language: String,
    code: Vec<String>,
}

fn language_and_code(text: &str) -> LanguageCodeBlock {
    let lines: Vec<&str> = text.split("\n").collect();

    let code: Vec<String> = text.split("\n").skip(1).map(String::from).collect();
    let could_be_language = lines.first();
    let language = match could_be_language {
        Some(l) => {
            if lines.len() == 1 {
                "".to_string()
            } else {
                l.to_string()
            }
        }
        None => "".to_string(),
    };
    LanguageCodeBlock { language, code }
}

/// Cleanup the code blocks in the markdown.
fn cleanup_code_blocks(text: &str) -> Result<String> {
    let regexes = vec![r#"(?s)```(.*?)```"#];
    // We need this replace since cmark seems to add a \` to ` its very weird.
    let mut text = text.replace("\\`", "`");
    for r in regexes {
        let re = regex::Regex::new(r)?;
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                let lang = &caps[1];
                let mut lc = language_and_code(lang);
                if lc.code.is_empty() && lc.language.is_empty() {
                    // ```cat file.txt``` is not a valid code block
                    // return the text back as is without it being cleaned up.
                    format!("```{}```", lang)
                } else {
                    // a\n parses into ["a",""]
                    if lc.code.last().is_some_and(|line| line.is_empty()) {
                        lc.code.pop();
                    }
                    if lc.code.is_empty() {
                        format!("```{}\n{}```", lc.language, lc.code.join("\n"))
                    } else {
                        format!("```{}\n{}\n```", lc.language, lc.code.join("\n"))
                    }
                }
            })
            .to_string();
    }

    Ok(text)
}

/// Convert a clap Command to markdown documentation.
pub fn app_to_markdown(app: &Command, title: &str) -> Result<String> {
    let mut document = MarkdownDocument(Vec::new());

    do_markdown(&mut document, app, title)?;

    let mut result = String::new();
    cmark_with_options(document.0.into_iter(), &mut result, get_cmark_options())?;

    // Fix the code blocks.
    result = cleanup_code_blocks(&result)?;

    // Fix the rustdoc links.
    result = rustdoc_to_markdown_link(&result)?;

    Ok(result)
}

#[cfg(test)]
mod test {
    use super::LanguageCodeBlock;
    use super::cleanup_code_blocks;
    use super::language_and_code;
    use pretty_assertions::assert_eq;

    fn definition_descriptions(markdown: &str) -> Vec<&str> {
        let mut descriptions = Vec::new();
        let mut remainder = markdown;

        while let Some(start) = remainder.find("<dd>") {
            let after_start = &remainder[start + "<dd>".len()..];
            let Some(end) = after_start.find("</dd>") else {
                break;
            };
            descriptions.push(&after_start[..end]);
            remainder = &after_start[end + "</dd>".len()..];
        }

        descriptions
    }

    #[test]
    fn test_rustdoc_to_markdown_link() {
        assert_eq!(
            super::rustdoc_to_markdown_link("<https://example.com>").unwrap(),
            "[https://example.com](https://example.com)"
        );
        assert_eq!(
            super::rustdoc_to_markdown_link("<https://example.com|Foo>").unwrap(),
            "[Foo](https://example.com)"
        );
        assert_eq!(
            super::rustdoc_to_markdown_link("<https://example.com/thing|Foo Bar Baz>").unwrap(),
            "[Foo Bar Baz](https://example.com/thing)"
        );
        assert_eq!(
            super::rustdoc_to_markdown_link(
                "Things are really cool. <https://example.com/thing|Foo Bar Baz> and <https://example.com|Foo>"
            )
            .unwrap(),
            "Things are really cool. [Foo Bar Baz](https://example.com/thing) and [Foo](https://example.com)"
        );
    }

    #[test]
    fn this_is_not_a_valid_markdown_code_block() {
        let code = "```some code```";
        let actual = cleanup_code_blocks(code).expect("to parse");
        let expected = "```some code```";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_cleanup_code_blocks() {
        assert_eq!(cleanup_code_blocks("```\nsome code```").unwrap(), "```\nsome code\n```");

        assert_eq!(
            cleanup_code_blocks("```\nsome code\n```").unwrap(),
            "```\nsome code\n```"
        );

        // the first some code is actually the language
        assert_eq!(
            cleanup_code_blocks("```some code\nsome other code```").unwrap(),
            "```some code\nsome other code\n```"
        );
    }

    mod language_and_code {
        use super::LanguageCodeBlock;
        use super::language_and_code;

        #[test]
        fn no_language() {
            let block = "\nhello\nworld\n";
            let actual = language_and_code(block);
            let expected = LanguageCodeBlock {
                language: "".to_string(),
                code: vec!["hello".to_string(), "world".to_string(), "".to_string()],
            };
            assert_eq!(actual, expected);
        }
        #[test]
        fn shell() {
            let block = "shell\nhello\nworld\n";
            let actual = language_and_code(block);
            let expected = LanguageCodeBlock {
                language: "shell".to_string(),
                code: vec!["hello".to_string(), "world".to_string(), "".to_string()],
            };
            assert_eq!(actual, expected);
        }
        #[test]
        fn shell_no_code() {
            let block = "shell\n";
            let actual = language_and_code(block);
            let expected = LanguageCodeBlock {
                language: "shell".to_string(),
                code: vec!["".to_string()],
            };
            assert_eq!(actual, expected);
        }
        #[test]
        fn huh() {
            let block = "huh";
            let actual = language_and_code(block);
            let expected = LanguageCodeBlock {
                language: "".to_string(),
                code: vec![],
            };
            assert_eq!(actual, expected);
        }
        #[test]
        fn no_language_no_code() {
            let block = "";
            let actual = language_and_code(block);
            let expected = LanguageCodeBlock {
                language: "".to_string(),
                code: vec![],
            };
            assert_eq!(actual, expected);
        }
    }

    mod code_blocks {
        use super::cleanup_code_blocks;
        #[test]
        fn basic_example() {
            let code = "```
hello
world
```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```\nhello\nworld\n```";
            assert_eq!(actual, expected);
        }
        #[test]
        fn hello_world_with_language() {
            let code = "```shell
hello
world
```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```shell\nhello\nworld\n```";
            assert_eq!(actual, expected);
        }
        #[test]
        fn only_language_many_lines() {
            let code = "```shell




```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```shell\n\n\n\n\n```";
            assert_eq!(actual, expected);
        }
        #[test]
        fn only_language_many_one_newline() {
            let code = "```shell
```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```shell\n```";
            assert_eq!(actual, expected);
        }
        #[test]
        fn not_a_code_block() {
            let code = "```shell```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```shell```";
            assert_eq!(actual, expected);
        }
        #[test]
        fn format_kept() {
            let code = "```
        hello
        world
```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```\n        hello\n        world\n```";
            assert_eq!(actual, expected);
        }
        #[test]
        fn one_with_new_lines() {
            let code = "```\nhello\n```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```\nhello\n```";
            assert_eq!(actual, expected);
        }
        #[test]
        fn python() {
            let code = "```python
    first()
    second()
```";

            let actual = cleanup_code_blocks(code).expect("to parse");
            let expected = "```python\n    first()\n    second()\n```";
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_multiline_arg_description_is_safe_for_mdx_definition_list() {
        let app = clap::Command::new("delete")
            .about("Delete one of your uploaded projects.")
            .arg(
                clap::Arg::new("id-or-path")
                    .help(
                        "The project id, or a local project directory, `.kcl` file, or `project.toml`.\n\n\
                         When a local path is provided, the persisted Zoo cloud project id will be removed from\n\
                         `project.toml` after the remote project is deleted.",
                    )
                    .required(true),
            );

        let markdown = super::app_to_markdown(&app, "zoo project delete").unwrap();

        assert!(markdown.contains(
            "<dd>The project id, or a local project directory, `.kcl` file, or `project.toml`. \
             When a local path is provided, the persisted Zoo cloud project id will be removed from \
             `project.toml` after the remote project is deleted.</dd>"
        ));

        for description in definition_descriptions(&markdown) {
            assert!(
                !description.contains('\n'),
                "definition descriptions must stay inline for MDX: {description:?}"
            );
        }
    }
}
