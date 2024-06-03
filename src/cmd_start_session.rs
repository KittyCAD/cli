use std::{convert::Infallible, net::SocketAddr};

use anyhow::Result;
use clap::Parser;
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response,
};

/// Start a session with the KittyCAD Modeling API.
/// This will open a socket at the given address, and you can
/// send KCL programs to that socket with the normal Zoo CLI KCL commands.
/// Those KCL programs will be run via this command's session.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdStartSession {
    /// What address should the Zoo server listen for KCL programs on?
    // E.g. `0.0.0.0:8888`.
    pub listen_on: SocketAddr,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdStartSession {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let addr = self.listen_on;
        let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handler)) });

        let server = hyper::Server::bind(&addr).serve(make_svc);

        writeln!(ctx.io.out, "Listening for KCL programs on {addr}").ok();
        // Run this server for... forever!
        if let Err(e) = server.await {
            writeln!(ctx.io.out, "{e}").ok();
        }

        Ok(())
    }
}

async fn handler(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new("Hello, World".into()))
}
