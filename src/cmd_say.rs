use anyhow::Result;
use clap::Parser;

/// Prints your text in a text bubble with KittyCAD as ASCII art
///
///     $ kittycad say
///     $ kittycad say hello!
///     $ kittycad say Hello World!
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdSay {
    /// What kitty says
    #[clap(name = "input", required = false, num_args(1..))]
    pub input: Vec<String>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdSay {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let kitty_speaking = !self.input.is_empty();
        let kitty_string = format_kitty(kitty_speaking);
        if kitty_speaking {
            let text = self.input.join(" ");
            let border = "-".repeat(text.len() + 2);
            let print_text = format!("|{text}|");
            writeln!(ctx.io.out, "{border}").ok();
            writeln!(ctx.io.out, "{print_text}").ok();
            writeln!(ctx.io.out, "{border}").ok();
        }
        writeln!(ctx.io.out, "{kitty_string}").ok();
        Ok(())
    }
}

fn format_kitty(is_speaking: bool) -> String {
    let speech_bar = if is_speaking { r"\" } else { " " };
    format!(
        "  {speech_bar}
   {speech_bar}                .....
    {speech_bar}              .::-:...            .....
     {speech_bar}            ..:---..:...        .::::...
      {speech_bar}          ..------:.::::::::::.:----......
       {speech_bar}      .::::------:::::::::::..------:..::::::::-.
        {speech_bar}   .::::..........::::::::::::::----:::::::::---.
         {speech_bar}  ::::::::::::::::::::::::...........::::::::---.
          {speech_bar} :--:::::::::::::::::::::::::::::::::::::::----.
            :--::=#@@@%%%###***+++===---::::::::--::-=----.
            :--::#@@@@@@@@@@@@@@@@@@@@@@@@@#-:::---:=-=---.
            :--::#@@@@@@@@@@@@@@@@@@@@@@@@@@@:::----++=---.
            :--::#@@@@%***#@@@@@@@@@*+*@@@@@@:::----=+----.
            :--::#@@@**%%%#+@@@@@@@@=-=@@@@@@:::----------
            :---:#@@@@@@@@@@@%%%%@@@=-=@@@@@@:::---------=
            -----#@@@@@@@**@@#+-+#@@#%%@@@@@@:::--------==
            -----#@@@@@@@@%+#%#-%@%+*#@@@@@@@::--------===
            -----*%@@@@@@@@@***+++*%@@@@@@@@@----------===.
            ------=+***####%%%@@@@@@@@@@@@@@@---------====.
            ----------::::::::::::--===+++*+:--------=====
            --==---===---::::::::::::::::-----------=====+
            -------+**+----------------------------====***
            ---------------::::::::::------------======#**
            -----=+++++-----------------=-=--=---======*+=
            -----=+++++--#@@@%%%%###+---=-=--+---======:.
            .......::----+####%%%%@@*---++++++---===:.
                  .*########*:.......:==--------.
                :*#%%%%%%%%%%+       -%######*#+.
                =#########%%%+     =#%%%%%%%%%##-
                -++***#####=.      *############:
                                   -==++***##+:
"
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::cmd::Command;

    pub struct TestItem {
        name: String,
        cmd: crate::cmd_say::CmdSay,
        want_out: String,
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[serial_test::serial]
    async fn test_cmd_say() {
        let tests: Vec<TestItem> = vec![
            TestItem {
                name: "no input string".to_string(),
                cmd: crate::cmd_say::CmdSay {
                    input: vec!["Hello".to_string(), "World!".to_string()],
                },
                want_out: "--------------\n|Hello World!|\n--------------\n".to_owned()
                    + &crate::cmd_say::format_kitty(true),
            },
            TestItem {
                name: "given input string".to_string(),
                cmd: crate::cmd_say::CmdSay { input: vec![] },
                want_out: crate::cmd_say::format_kitty(false),
            },
        ];

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        for t in tests {
            let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
            // We need to also turn off the fancy terminal colors.
            // This ensures it also works in GitHub actions/any CI.
            io.set_color_enabled(false);
            // TODO: we should figure out how to test the prompts.
            io.set_never_prompt(true);
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
            };

            let cmd_say = crate::cmd_say::CmdSay { input: t.cmd.input };
            match cmd_say.run(&mut ctx).await {
                Ok(()) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert!(stderr.is_empty(), "test {}: {}", t.name, stderr);
                    if !stdout.contains(&t.want_out) {
                        assert_eq!(stdout, t.want_out, "test {}: stdout mismatch", t.name);
                    }
                }
                Err(_err) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert_eq!(stdout, t.want_out, "test {}", t.name);
                    assert!(stderr.is_empty(), "test {}: {}", t.name, stderr);
                }
            }
        }
    }
}
