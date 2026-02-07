use std::fs;
use tonic::transport::{Certificate, ClientTlsConfig, Identity};

pub fn client_tls_config(
    cert_path: &str,
    key_path: &str,
    ca_path: &str,
) -> Result<ClientTlsConfig, std::io::Error> {
    let cert = fs::read(cert_path)?;
    let key = fs::read(key_path)?;
    let ca = fs::read(ca_path)?;

    let identity = Identity::from_pem(cert, key);
    let ca_cert = Certificate::from_pem(ca);

    Ok(ClientTlsConfig::new()
        .identity(identity)
        .ca_certificate(ca_cert))
}
