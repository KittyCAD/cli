use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;

/// Starts a modeling session
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdStartSession {
    /// What host/port to accept KCL programs on.
    #[clap(default_value = "0.0.0.0:3333")]
    pub listen_on: SocketAddr,
    /// How many engine connections to use in the connection pool.
    #[clap(default_value_t = 1)]
    pub num_engine_connections: u8,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdStartSession {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let args = kcl_test_server::ServerArgs {
            listen_on: self.listen_on,
            num_engine_conns: self.num_engine_connections,
            engine_address: None,
        };
        kcl_test_server::start_server(args).await?;
        writeln!(ctx.io.out, "Terminating").ok();
        Ok(())
    }
}
