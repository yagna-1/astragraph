use std::env;
use std::fs;
use tonic::transport::{Certificate, Identity, ServerTlsConfig};

pub fn server_tls_config() -> Result<ServerTlsConfig, std::io::Error> {
    let cert_path = env::var("ASTRAGRAPH_GRAPH_TLS_CERT")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "TLS cert missing"))?;
    let key_path = env::var("ASTRAGRAPH_GRAPH_TLS_KEY")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "TLS key missing"))?;
    let ca_path = env::var("ASTRAGRAPH_GRAPH_TLS_CA")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "TLS CA missing"))?;

    let cert = fs::read(cert_path)?;
    let key = fs::read(key_path)?;
    let ca = fs::read(ca_path)?;

    let identity = Identity::from_pem(cert, key);
    let ca_cert = Certificate::from_pem(ca);

    Ok(ServerTlsConfig::new()
        .identity(identity)
        .client_ca_root(ca_cert))
}
