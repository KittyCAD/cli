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
    #[clap(name = "input", required = false, multiple_values = true)]
    pub input: Vec<String>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdSay {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let kitty_speaking = self.input.len() > 0;
        let kitty_string = format_kitty(kitty_speaking);
        if self.input.len() > 0 {
            let text = self.input.join(" ");
            let mut border = String::from("--");
            let print_text = format!("|{}|", text);
            for _i in 0..text.len() {
                border.push('-');
            }
            writeln!(ctx.io.out, "{}", border).ok();
            writeln!(ctx.io.out, "{}", print_text).ok();
            writeln!(ctx.io.out, "{}", border).ok();
        }
        writeln!(ctx.io.out, "{}", kitty_string).ok();
        Ok(())
    }
}

fn format_kitty(is_speaking: bool) -> String {
    let speech_bar = if is_speaking { r#"\"# } else { " " };
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
",
        speech_bar = speech_bar
    )
}
