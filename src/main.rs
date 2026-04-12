mod analyzer;
mod config;
mod intellisense;
mod parser;
mod server;
mod workspace;

use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| server::PawnProServer::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
