//! 自定义 Smithy HTTP 客户端：支持 `--insecure`（跳过服务端证书校验）与可选 PEM CA。

use aws_smithy_runtime_api::client::http::{
    http_client_fn, HttpConnector, HttpConnectorFuture, HttpConnectorSettings, SharedHttpClient,
    SharedHttpConnector,
};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::body::SdkBody;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector as HyperHttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error, SignatureScheme};
use std::fmt;
use std::sync::Arc;
use tower::Service;

/// 跳过证书校验（仅用于内网 / 自签名 MinIO 压测）。
#[derive(Debug)]
struct SkipServerAuth;

impl ServerCertVerifier for SkipServerAuth {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
        ]
    }
}

fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn rustls_config(
    insecure: bool,
    ca_pem: Option<&[u8]>,
) -> Result<Arc<rustls::ClientConfig>, String> {
    install_crypto_provider();
    if insecure {
        let cfg = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerAuth))
            .with_no_client_auth();
        return Ok(Arc::new(cfg));
    }

    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    if let Some(pem) = ca_pem {
        let mut cursor = std::io::Cursor::new(pem);
        for item in rustls_pemfile::certs(&mut cursor).flatten() {
            let _ = roots.add(item);
        }
    }
    let cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

type HttpsConn = HttpsConnector<HyperHttpConnector>;

#[derive(Clone)]
struct HttpsAdapter {
    inner: Client<HttpsConn, SdkBody>,
}

impl fmt::Debug for HttpsAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HttpsAdapter")
    }
}

impl HttpConnector for HttpsAdapter {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let request = match request.try_into_http1x() {
            Ok(r) => r,
            Err(err) => {
                return HttpConnectorFuture::ready(Err(ConnectorError::user(err.into())));
            }
        };
        let mut client = self.inner.clone();
        let fut = Service::call(&mut client, request);
        HttpConnectorFuture::new(async move {
            let response = fut
                .await
                .map_err(|e| ConnectorError::other(e.into(), None))?
                .map(SdkBody::from_body_1_x);
            HttpResponse::try_from(response).map_err(|e| ConnectorError::other(e.into(), None))
        })
    }
}

/// 构建用于 HTTPS 的 Smithy HTTP 客户端（`insecure` 或额外 CA）。
pub fn shared_https_client(
    insecure: bool,
    ca_pem: Option<&[u8]>,
) -> Result<SharedHttpClient, String> {
    let tls = rustls_config(insecure, ca_pem)?;
    Ok(http_client_fn(
        move |settings: &HttpConnectorSettings, _rc: &RuntimeComponents| {
            let mut http = HyperHttpConnector::new();
            if let Some(d) = settings.connect_timeout() {
                http.set_connect_timeout(Some(d));
            }

            let tls_cfg = (*tls).clone();
            let https: HttpsConn = hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls_cfg)
                .https_or_http()
                .enable_http1()
                .wrap_connector(http);

            let mut hyper_builder = Client::builder(TokioExecutor::new());
            hyper_builder.pool_timer(TokioTimer::new());

            let inner = hyper_builder.build(https);
            SharedHttpConnector::new(HttpsAdapter { inner })
        },
    ))
}
