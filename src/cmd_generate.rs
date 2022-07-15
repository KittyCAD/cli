use std::{fs, io::Write};

use anyhow::{Context, Result};
use clap::{Command, CommandFactory, Parser};

/// Generate various documentation files for the `kittycad` command line.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdGenerate {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Markdown(CmdGenerateMarkdown),
    ManPages(CmdGenerateManPages),
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdGenerate {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Markdown(cmd) => cmd.run(ctx).await,
            SubCommand::ManPages(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Generate markdown documentation.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdGenerateMarkdown {
    /// Path directory where you want to output the generated files.
    #[clap(short = 'D', long, default_value = "")]
    pub dir: String,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdGenerateMarkdown {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let mut app: Command = crate::Opts::command();
        app._build_all();

        // Make sure the output directory exists.
        if !self.dir.is_empty() {
            fs::create_dir_all(&self.dir).with_context(|| format!("failed to create directory {}", self.dir))?;
        }

        self.generate(ctx, &app, "")?;

        Ok(())
    }
}

impl CmdGenerateMarkdown {
    fn generate(&self, ctx: &mut crate::context::Context, app: &Command, parent: &str) -> Result<()> {
        let mut p = parent.to_string();
        if !p.is_empty() {
            p = format!("{}_{}", p, app.get_name());
        } else {
            p = app.get_name().to_string();
        }

        let filename = format!("{}.md", p);
        let title = p.replace('_', " ");
        writeln!(ctx.io.out, "Generating markdown for `{}` -> {}", title, filename)?;

        // Generate the markdown.
        let m = crate::docs_markdown::app_to_markdown(app, &title)?;

        // Add our header information.
        let markdown = format!(
            r#"---
title: "{}"
excerpt: "{}"
layout: manual
---

{}"#,
            title,
            app.get_about().unwrap_or_default(),
            m
        );
        if self.dir.is_empty() {
            // TODO: glamorize markdown to the shell.
            writeln!(ctx.io.out, "{}", markdown)?;
        } else {
            let p = std::path::Path::new(&self.dir).join(filename);
            let mut file = std::fs::File::create(p)?;
            file.write_all(markdown.as_bytes())?;
        }

        // Iterate over all the subcommands and generate the documentation.
        for subcmd in app.get_subcommands() {
            self.generate(ctx, subcmd, &p)?;
        }

        Ok(())
    }
}

/// Generate manual pages.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdGenerateManPages {
    /// Path directory where you want to output the generated files.
    #[clap(short = 'D', long, default_value = "")]
    pub dir: String,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdGenerateManPages {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let mut app: Command = crate::Opts::command();
        app._build_all();

        // Make sure the output directory exists.
        if !self.dir.is_empty() {
            fs::create_dir_all(&self.dir).with_context(|| format!("failed to create directory {}", self.dir))?;
        }

        self.generate(ctx, &app, "", &app)?;

        Ok(())
    }
}

impl CmdGenerateManPages {
    // TODO: having the root repeated like this sucks, clean this up.
    fn generate(
        &self,
        ctx: &mut crate::context::Context,
        app: &Command,
        parent: &str,
        root: &clap::Command,
    ) -> Result<()> {
        let mut p = parent.to_string();
        if !p.is_empty() {
            p = format!("{}-{}", p, app.get_name());
        } else {
            p = app.get_name().to_string();
        }

        let filename = format!("{}.1", p);
        let title = p.replace('-', " ");
        writeln!(ctx.io.out, "Generating man page for `{}` -> {}", title, filename)?;

        if self.dir.is_empty() {
            crate::docs_man::generate_manpage(app, &mut ctx.io.out, &title, root);
        } else {
            let p = std::path::Path::new(&self.dir).join(filename);
            let mut file = std::fs::File::create(p)?;
            crate::docs_man::generate_manpage(app, &mut file, &title, root);
        }

        // Iterate over all the subcommands and generate the documentation.
        for subcmd in app.get_subcommands() {
            // Make it recursive.
            self.generate(ctx, subcmd, &p, root)?;
        }

        Ok(())
    }
}

#[cfg(test)]
fn test_app() -> clap::Command<'static> {
    // Define our app.
    clap::Command::new("git")
        .about("A fictional versioning CLI")
        .subcommand_required(true)
        .allow_external_subcommands(true)
        .allow_invalid_utf8_for_external_subcommands(true)
        .subcommand(
            Command::new("clone")
                .about("Clones repos")
                .arg(clap::arg!(<REMOTE> "The remote to clone"))
                .arg_required_else_help(true),
        )
        .subcommand(
            clap::Command::new("push")
                .about("pushes things")
                .arg(clap::arg!(<REMOTE> "The remote to target"))
                .arg_required_else_help(true),
        )
        .subcommand(
            clap::Command::new("add")
                .about("adds things")
                .arg_required_else_help(true)
                .arg(clap::arg!(<PATH> ... "Stuff to add").allow_invalid_utf8(true))
                .subcommand(
                    clap::Command::new("new")
                        .about("subcommand for adding new stuff")
                        .long_about("See url: <https://example.com> and <https://example.com/thing|thing>.")
                        // Add an enum arg.
                        .arg(
                            clap::Arg::new("type")
                                .help("The type of thing to add.")
                                .long("type")
                                .possible_values(&["file", "dir"])
                                .default_value("file")
                                .required(true),
                        )
                        .subcommand(clap::Command::new("foo").about("sub subcommand")),
                ),
        )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::cmd::Command;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_generate_markdown() {
        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        let (io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
        let mut ctx = crate::context::Context {
            config: &mut c,
            io,
            debug: false,
        };

        let cmd = crate::cmd_generate::CmdGenerateMarkdown { dir: "".to_string() };

        cmd.run(&mut ctx).await.unwrap();

        let stdout = std::fs::read_to_string(stdout_path).unwrap();
        let stderr = std::fs::read_to_string(stderr_path).unwrap();

        assert!(stdout.contains("<dt><code>-H/--host</code></dt>"), "");
        assert!(stdout.contains("### About"), "");

        assert_eq!(stderr, "");
    }

    #[test]
    fn test_generate_markdown_sub_subcommands() {
        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        let (io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
        let mut ctx = crate::context::Context {
            config: &mut c,
            io,
            debug: false,
        };

        let cmd = crate::cmd_generate::CmdGenerateMarkdown { dir: "".to_string() };

        let app = crate::cmd_generate::test_app();

        cmd.generate(&mut ctx, &app, "").unwrap();

        let expected = r#"Generating markdown for `git` -> git.md
---
title: "git"
excerpt: "A fictional versioning CLI"
layout: manual
---

A fictional versioning CLI

### Subcommands

* [git clone](./git_clone)
* [git push](./git_push)
* [git add](./git_add)

### Options

<dl class="flags">
   <dt><code>--help</code></dt>
   <dd>Print help information</dd>

   <dt><code>--version</code></dt>
   <dd>Print version information</dd>
</dl>


Generating markdown for `git clone` -> git_clone.md
---
title: "git clone"
excerpt: "Clones repos"
layout: manual
---

Clones repos

### Options

<dl class="flags">
   <dt><code>--help</code></dt>
   <dd>Print help information</dd>

   <dt><code>--version</code></dt>
   <dd>Print version information</dd>

   <dt><code>REMOTE</code></dt>
   <dd>The remote to clone</dd>
</dl>


Generating markdown for `git push` -> git_push.md
---
title: "git push"
excerpt: "pushes things"
layout: manual
---

pushes things

### Options

<dl class="flags">
   <dt><code>--help</code></dt>
   <dd>Print help information</dd>

   <dt><code>--version</code></dt>
   <dd>Print version information</dd>

   <dt><code>REMOTE</code></dt>
   <dd>The remote to target</dd>
</dl>


Generating markdown for `git add` -> git_add.md
---
title: "git add"
excerpt: "adds things"
layout: manual
---

adds things

### Subcommands

* [git add new](./git_add_new)

### Options

<dl class="flags">
   <dt><code>--help</code></dt>
   <dd>Print help information</dd>

   <dt><code>--version</code></dt>
   <dd>Print version information</dd>

   <dt><code>PATH</code></dt>
   <dd>Stuff to add</dd>
</dl>


Generating markdown for `git add new` -> git_add_new.md
---
title: "git add new"
excerpt: "subcommand for adding new stuff"
layout: manual
---

subcommand for adding new stuff

### Subcommands

* [git add new foo](./git_add_new_foo)

### Options

<dl class="flags">
   <dt><code>--help</code></dt>
   <dd>Print help information</dd>

   <dt><code>--version</code></dt>
   <dd>Print version information</dd>

   <dt><code>--type</code></dt>
   <dd>The type of thing to add.<br/>Possible values: <code>file | dir</code><br/>Default value: <code>file</code></dd>
</dl>


### About

See url: [https://example.com](https://example.com) and [thing](https://example.com/thing).

### See also

* [git add](./git_add)
Generating markdown for `git add new foo` -> git_add_new_foo.md
---
title: "git add new foo"
excerpt: "sub subcommand"
layout: manual
---

sub subcommand

### Options

<dl class="flags">
   <dt><code>--help</code></dt>
   <dd>Print help information</dd>

   <dt><code>--version</code></dt>
   <dd>Print version information</dd>
</dl>


### See also

* [git add](./git_add)
* [git add new](./git_add_new)
"#;

        let stdout = std::fs::read_to_string(stdout_path).unwrap();
        let stderr = std::fs::read_to_string(stderr_path).unwrap();

        assert_eq!(stdout, expected);
        assert_eq!(stderr, "");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_generate_man_pages() {
        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        let (io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
        let mut ctx = crate::context::Context {
            config: &mut c,
            io,
            debug: true,
        };

        let cmd = crate::cmd_generate::CmdGenerateManPages { dir: "".to_string() };

        cmd.run(&mut ctx).await.unwrap();

        let stdout = std::fs::read_to_string(stdout_path).unwrap();
        let stderr = std::fs::read_to_string(stderr_path).unwrap();

        assert!(stdout.contains("kittycad(1)"), "");

        assert_eq!(stderr, "");
    }

    #[test]
    fn test_generate_man_pages_sub_subcommands() {
        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        let (io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
        let mut ctx = crate::context::Context {
            config: &mut c,
            io,
            debug: true,
        };

        let cmd = crate::cmd_generate::CmdGenerateManPages { dir: "".to_string() };

        // Define our app.
        let app = crate::cmd_generate::test_app();

        cmd.generate(&mut ctx, &app, "", &app).unwrap();

        let expected = r#"Generating man page for `git` -> git.1
.TH "GIT" "1" "" "git " "General Commands Manual"
.ss \n[.ss] 0
.nh
.ad l
.SH "NAME"
git \- A fictional versioning CLI
.SH "SYNOPSIS"
\fIgit\fP [\-\-help] [\-\-version] <subcommands>
.SH "DESCRIPTION"

.sp
A fictional versioning CLI
.SH "OPTIONS"
.TP
\-\-\fBhelp\fP
Print help information
.TP
\-\-\fBversion\fP
Print version information

.SH "SUBCOMMANDS"
.TP
\fBgit\-clone(1)\fP
Clones repos
.TP
\fBgit\-push(1)\fP
pushes things
.TP
\fBgit\-add(1)\fP
adds things

Generating man page for `git clone` -> git-clone.1
.TH "GIT" "1" "" "git " "General Commands Manual"
.ss \n[.ss] 0
.nh
.ad l
.SH "NAME"
git\-clone \- Clones repos
.SH "SYNOPSIS"
\fIgit clone\fP [\-\-help] [\-\-version] <REMOTE>
.SH "DESCRIPTION"

.sp
Clones repos
.SH "OPTIONS"
.TP
\-\-\fBhelp\fP
Print help information
.TP
\-\-\fBversion\fP
Print version information
.TP
\fB<REMOTE>\fP
The remote to clone
.SH "SEE ALSO"
.TP
\fBgit(1)\fP

Generating man page for `git push` -> git-push.1
.TH "GIT" "1" "" "git " "General Commands Manual"
.ss \n[.ss] 0
.nh
.ad l
.SH "NAME"
git\-push \- pushes things
.SH "SYNOPSIS"
\fIgit push\fP [\-\-help] [\-\-version] <REMOTE>
.SH "DESCRIPTION"

.sp
pushes things
.SH "OPTIONS"
.TP
\-\-\fBhelp\fP
Print help information
.TP
\-\-\fBversion\fP
Print version information
.TP
\fB<REMOTE>\fP
The remote to target
.SH "SEE ALSO"
.TP
\fBgit(1)\fP

Generating man page for `git add` -> git-add.1
.TH "GIT" "1" "" "git " "General Commands Manual"
.ss \n[.ss] 0
.nh
.ad l
.SH "NAME"
git\-add \- adds things
.SH "SYNOPSIS"
\fIgit add\fP [\-\-help] [\-\-version] <PATH> <subcommands>
.SH "DESCRIPTION"

.sp
adds things
.SH "OPTIONS"
.TP
\-\-\fBhelp\fP
Print help information
.TP
\-\-\fBversion\fP
Print version information
.TP
\fB<PATH>\fP
Stuff to add
.SH "SUBCOMMANDS"
.TP
\fBgit\-add\-new(1)\fP
subcommand for adding new stuff

.SH "SEE ALSO"
.TP
\fBgit(1)\fP

Generating man page for `git add new` -> git-add-new.1
.TH "GIT" "1" "" "git " "General Commands Manual"
.ss \n[.ss] 0
.nh
.ad l
.SH "NAME"
git\-add\-new \- subcommand for adding new stuff
.SH "SYNOPSIS"
\fIgit add new\fP [\-\-help] [\-\-version] <\-\-type> [subcommands]
.SH "DESCRIPTION"

.sp
See url: <https://example.com> and <https://example.com/thing|thing>.
.SH "OPTIONS"
.TP
\-\-\fBhelp\fP
Print help information
.TP
\-\-\fBversion\fP
Print version information
.TP
\-\-\fBtype\fP [possible values: file,dir] [default: file]
The type of thing to add.

.SH "SUBCOMMANDS"
.TP
\fBgit\-add\-new\-foo(1)\fP
sub subcommand

.SH "SEE ALSO"
.TP
\fBgit(1)\fP
.TP
\fBgit\-add(1)\fP

Generating man page for `git add new foo` -> git-add-new-foo.1
.TH "GIT" "1" "" "git " "General Commands Manual"
.ss \n[.ss] 0
.nh
.ad l
.SH "NAME"
git\-add\-new\-foo \- sub subcommand
.SH "SYNOPSIS"
\fIgit add new foo\fP [\-\-help] [\-\-version]
.SH "DESCRIPTION"

.sp
sub subcommand
.SH "OPTIONS"
.TP
\-\-\fBhelp\fP
Print help information
.TP
\-\-\fBversion\fP
Print version information

.SH "SEE ALSO"
.TP
\fBgit(1)\fP
.TP
\fBgit\-add(1)\fP
.TP
\fBgit\-add\-new(1)\fP

"#;

        let stdout = std::fs::read_to_string(stdout_path).unwrap();
        let stderr = std::fs::read_to_string(stderr_path).unwrap();

        assert_eq!(stdout, expected);
        assert_eq!(stderr, "");
    }
}
