use anyhow::Result;

/*pub struct Example {
    pub description: String,
    pub args: Vec<String>,
    pub output: String,
}*/

/// This trait describes a command.
#[async_trait::async_trait]
pub trait Command {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()>;
}

/*pub trait CommandExamples {
    fn examples(&self) -> Vec<Example>;
}*/
