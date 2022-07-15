use std::fmt::Write;

use anyhow::Result;
use clap::Command;
use pulldown_cmark_to_cmark::cmark_with_options;

struct MarkdownDocument<'a>(Vec<pulldown_cmark::Event<'a>>);

impl MarkdownDocument<'_> {
    fn header(&mut self, text: String, level: pulldown_cmark::HeadingLevel) {
        self.0.push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading(
            level,
            None,
            vec![],
        )));
        self.0.push(pulldown_cmark::Event::Text(text.into()));
        self.0.push(pulldown_cmark::Event::End(pulldown_cmark::Tag::Heading(
            level,
            None,
            vec![],
        )));
    }

    fn paragraph(&mut self, text: String) {
        self.0
            .push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::Paragraph));
        self.0.push(pulldown_cmark::Event::Text(text.into()));
        self.0.push(pulldown_cmark::Event::End(pulldown_cmark::Tag::Paragraph));
    }

    fn link_in_list(&mut self, text: String, url: String) {
        let link = pulldown_cmark::Tag::Link(pulldown_cmark::LinkType::Inline, url.into(), "".into());

        self.0.push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::Item));
        self.0.push(pulldown_cmark::Event::Start(link.clone()));
        self.0.push(pulldown_cmark::Event::Text(text.into()));
        self.0.push(pulldown_cmark::Event::End(link));
        self.0.push(pulldown_cmark::Event::End(pulldown_cmark::Tag::Item));
    }
}

fn do_markdown(doc: &mut MarkdownDocument, app: &Command, title: &str) -> Result<()> {
    // We don't need the header since our renderer will do that for us.
    //doc.header(app.get_name().to_string(), pulldown_cmark::HeadingLevel::H2);

    if let Some(about) = app.get_about() {
        doc.paragraph(about.to_string());
    }

    if app.has_subcommands() {
        doc.header("Subcommands".to_string(), pulldown_cmark::HeadingLevel::H3);

        doc.0
            .push(pulldown_cmark::Event::Start(pulldown_cmark::Tag::List(None)));

        for cmd in app.get_subcommands() {
            doc.link_in_list(
                format!("{} {}", title, cmd.get_name()),
                format!("./{}_{}", title.replace(' ', "_"), cmd.get_name()),
            );
        }

        doc.0.push(pulldown_cmark::Event::End(pulldown_cmark::Tag::List(None)));
    }

    let args = app.get_arguments().collect::<Vec<&clap::Arg>>();
    if !args.is_empty() {
        doc.header("Options".to_string(), pulldown_cmark::HeadingLevel::H3);

        let mut html = "<p>\n<dl class=\"flags\">\n".to_string();

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

            if arg.get_long().is_none() && arg.get_short().is_none() && !arg.get_name().is_empty() {
                def.push_str(arg.get_name());
            }

            write!(
                html,
                r#"   <dt><code>{}</code></dt>
   <dd>{}</dd>
"#,
                def,
                arg.get_help().unwrap_or_default()
            )
            .unwrap_or_default();
        }

        html.push_str("</dl>\n</p>\n\n");

        doc.0.push(pulldown_cmark::Event::Html(html.into()));
    }

    // TODO: add examples

    if let Some(about) = app.get_long_about() {
        doc.header("About".to_string(), pulldown_cmark::HeadingLevel::H3);
        let raw = about
            .to_string()
            .trim_start_matches(app.get_about().unwrap_or_default())
            .trim_start_matches('.')
            .to_string();

        // We need to parse this as markdown so any code snippets denoted by 4 spaces
        // are rendered as code blocks. Which works better for our docs.
        let parser = pulldown_cmark::Parser::new(&raw);

        let mut result = String::new();
        cmark_with_options(parser, &mut result, get_cmark_options())?;

        doc.paragraph(result);
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

        doc.0.push(pulldown_cmark::Event::End(pulldown_cmark::Tag::List(None)));
    }

    Ok(())
}

fn get_cmark_options() -> pulldown_cmark_to_cmark::Options<'static> {
    pulldown_cmark_to_cmark::Options {
        newlines_after_codeblock: 2,
        code_block_token_count: 3,
        ..Default::default()
    }
}

/// Convert a clap Command to markdown documentation.
pub fn app_to_markdown(app: &Command, title: &str) -> Result<String> {
    let mut document = MarkdownDocument(Vec::new());

    do_markdown(&mut document, app, title)?;

    let mut result = String::new();
    cmark_with_options(document.0.iter(), &mut result, get_cmark_options())?;

    // Fix the code blocks.
    result = result.replace("\\`", "`").replace("```", "\n```");

    Ok(result)
}
