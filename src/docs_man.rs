use std::{fmt::Write as _, io::Write};

use roff::{bold, escape, italic, list, paragraph, ManSection, Roff, Troffable};

/// Man page generator
pub struct Man {
    section: Option<ManSection>,
    manual: Option<String>,
    sections: Vec<(String, String)>,
}

impl Default for Man {
    fn default() -> Self {
        Self {
            section: Some(ManSection::Executable),
            manual: Some("General Commands Manual".to_string()),
            sections: Vec::new(),
        }
    }
}

/// Generate manpage for your application using the most common default values.
pub fn generate_manpage(app: &clap::Command, buf: &mut dyn Write, title: &str, root: &clap::Command) {
    let man = Man::default();
    man.render(app, buf, title, root);
}

impl Man {
    /// Write the manpage to a buffer.
    pub fn render(self, app: &clap::Command, buf: &mut dyn std::io::Write, title: &str, root: &clap::Command) {
        let mut page = Roff::new(root.get_name(), self.get_section())
            .source(&format!(
                "{} {}",
                root.get_name(),
                root.get_version().unwrap_or_default()
            ))
            .section("Name", [&about(app, title)])
            .section("Synopsis", [&synopsis(app, title)])
            .section("Description", &description(app));

        if let Some(manual) = &self.manual {
            page = page.manual(manual);
        }

        if app_has_arguments(app) {
            page = page.section("Options", &options(app));
        }

        if app_has_subcommands(app) {
            page = page.section(
                &subcommand_heading(app),
                &subcommands(app, self.get_section().value(), title),
            )
        }

        if app.get_after_long_help().is_some() || app.get_after_help().is_some() {
            page = page.section("Extra", &after_help(app))
        }

        for (title, section) in self.sections {
            page = page.section(&title, &[section]);
        }

        // Check if the command has a parent, for the see also section.
        let mut split = title.split(' ').collect::<Vec<&str>>();
        if title != root.get_name() {
            // Get the parent command.
            // Iterate if more than one, thats why we have a list.
            if split.len() > 1 {
                // Remove the last element, since that is the command name.
                split.pop();

                page = page.section("See also", &see_also(split));
            }
        }

        if app_has_version(root) {
            page = page.section("Version", &[version(root)]);
        }

        if root.get_author().is_some() {
            page = page.section("Author(s)", &[root.get_author().unwrap_or_default()]);
        }

        buf.write_all(page.render().as_bytes()).unwrap();
    }

    fn get_section(&self) -> ManSection {
        self.section.unwrap_or(ManSection::Executable)
    }
}

fn app_has_version(app: &clap::Command) -> bool {
    app.get_long_version().or_else(|| app.get_version()).is_some()
}

fn app_has_arguments(app: &clap::Command) -> bool {
    app.get_arguments().any(|i| !i.is_hide_set())
}

fn app_has_subcommands(app: &clap::Command) -> bool {
    app.get_subcommands().any(|i| !i.is_hide_set())
}

fn subcommand_heading(app: &clap::Command) -> String {
    match app.get_subcommand_help_heading() {
        Some(title) => title.to_string(),
        None => "Subcommands".to_string(),
    }
}

fn about(app: &clap::Command, title: &str) -> String {
    let t = title.replace(' ', "-");
    match app.get_about().or_else(|| app.get_long_about()) {
        Some(about) => format!("{} - {}", t, about),
        None => t,
    }
}

fn description(app: &clap::Command) -> Vec<String> {
    match app.get_long_about().or_else(|| app.get_about()) {
        Some(about) => about
            .lines()
            .filter_map(|l| (!l.trim().is_empty()).then(|| paragraph(l.trim())))
            .collect(),
        None => Vec::new(),
    }
}

fn synopsis(app: &clap::Command, title: &str) -> String {
    let mut res = String::new();

    res.push_str(&italic(title));
    res.push(' ');

    for opt in app.get_arguments() {
        let (lhs, rhs) = option_markers(opt);
        res.push_str(&match (opt.get_short(), opt.get_long()) {
            (Some(short), Some(long)) => format!("{}-{}|--{}{} ", lhs, short, long, rhs),
            (Some(short), None) => format!("{}-{}{} ", lhs, short, rhs),
            (None, Some(long)) => format!("{}--{}{} ", lhs, long, rhs),
            (None, None) => "".to_string(),
        });
    }

    for arg in app.get_positionals() {
        let (lhs, rhs) = option_markers(arg);
        write!(res, "{}{}{} ", lhs, arg.get_id(), rhs).unwrap_or_default();
    }

    if app.has_subcommands() {
        let (lhs, rhs) = subcommand_markers(app);
        write!(
            res,
            "{}{}{} ",
            lhs,
            escape(
                &app.get_subcommand_value_name()
                    .unwrap_or(&subcommand_heading(app))
                    .to_lowercase()
            ),
            rhs
        )
        .unwrap_or_default();
    }

    res.trim().to_string()
}

fn options(app: &clap::Command) -> Vec<String> {
    let mut res = Vec::new();
    let items: Vec<_> = app.get_arguments().filter(|i| !i.is_hide_set()).collect();

    for opt in items.iter().filter(|a| !a.is_positional()) {
        let mut body = Vec::new();

        let mut header = match (opt.get_short(), opt.get_long()) {
            (Some(short), Some(long)) => {
                vec![short_option(short), ", ".to_string(), long_option(long)]
            }
            (Some(short), None) => vec![short_option(short)],
            (None, Some(long)) => vec![long_option(long)],
            (None, None) => vec![],
        };

        if let Some(value) = &opt.get_value_names() {
            header.push(format!("={}", italic(&value.join(" "))));
        }

        if let Some(defs) = option_default_values(opt) {
            header.push(format!(" {}", defs));
        }

        if let Some(help) = opt.get_long_help().or_else(|| opt.get_help()) {
            body.push(help.to_string());
        }

        if let Some(env) = option_environment(opt) {
            body.push(env);
        }

        body.push("\n".to_string());

        res.push(list(&header, &body));
    }

    for pos in items.iter().filter(|a| a.is_positional()) {
        let (lhs, rhs) = option_markers(pos);
        let name = format!("{}{}{}", lhs, pos.get_id(), rhs);

        let mut header = vec![bold(&name)];

        let mut body = Vec::new();

        if let Some(defs) = option_default_values(pos) {
            header.push(format!(" {}", defs));
        }

        if let Some(help) = pos.get_long_help().or_else(|| pos.get_help()) {
            body.push(help.to_string());
        }

        if let Some(env) = option_environment(pos) {
            body.push(env);
        }

        res.push(list(&header, &body))
    }

    res
}

fn subcommands(app: &clap::Command, section: i8, title: &str) -> Vec<String> {
    app.get_subcommands()
        .filter(|s| !s.is_hide_set())
        .map(|command| {
            let name = format!("{}-{}({})", title.replace(' ', "-"), command.get_name(), section);

            let mut body = match command.get_about().or_else(|| command.get_long_about()) {
                Some(about) => about
                    .lines()
                    .filter_map(|l| (!l.trim().is_empty()).then(|| l.trim()))
                    .collect(),
                None => Vec::new(),
            };

            body.push("\n");

            list(&[bold(&name)], &body)
        })
        .collect()
}

fn version(app: &clap::Command) -> String {
    format!("v{}", app.get_long_version().or_else(|| app.get_version()).unwrap())
}

fn see_also(split: Vec<&str>) -> Vec<String> {
    let mut result: Vec<String> = vec![];
    for (i, _) in split.iter().enumerate() {
        let mut p = split.clone();
        p.truncate(i + 1);
        let parent = p.join("-");

        // TODO: we could print the description here as well, instead of empty.
        let empty: Vec<String> = vec![];

        result.push(list(&[bold(&format!("{}(1)", parent))], &empty));
    }

    result
}

fn after_help(app: &clap::Command) -> Vec<String> {
    match app.get_after_long_help().or_else(|| app.get_after_help()) {
        Some(about) => about
            .lines()
            .filter_map(|l| (!l.trim().is_empty()).then(|| paragraph(l.trim())))
            .collect(),
        None => Vec::new(),
    }
}

fn subcommand_markers(cmd: &clap::Command) -> (&'static str, &'static str) {
    markers(cmd.is_subcommand_required_set() || cmd.is_arg_required_else_help_set())
}

fn option_markers(opt: &clap::Arg) -> (&'static str, &'static str) {
    markers(opt.is_required_set())
}

fn markers(required: bool) -> (&'static str, &'static str) {
    if required {
        ("<", ">")
    } else {
        ("[", "]")
    }
}

fn short_option(opt: char) -> String {
    format!("-{}", bold(&opt.to_string()))
}

fn long_option(opt: &str) -> String {
    format!("--{}", bold(opt))
}

fn option_environment(opt: &clap::Arg) -> Option<String> {
    if opt.is_hide_env_set() {
        return None;
    } else if let Some(env) = opt.get_env() {
        return Some(paragraph(&format!(
            "May also be specified with the {} environment variable. ",
            bold(&env.to_string_lossy())
        )));
    }

    None
}

fn option_default_values(opt: &clap::Arg) -> Option<String> {
    if !opt.get_default_values().is_empty() {
        let values = opt
            .get_default_values()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(",");

        return Some(format!("[default: {}]", values));
    }

    None
}
