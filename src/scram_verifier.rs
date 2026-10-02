//! SCRAM-SHA-256 authentication against a stored verifier.
//!
//! pgwire's `ScramAuth` needs the SaltedPassword. PostgreSQL never keeps
//! that: `pg_authid.rolpassword` holds
//! `SCRAM-SHA-256$<iterations>:<salt>$<StoredKey>:<ServerKey>`, and the
//! server checks the client proof with StoredKey alone (RFC 5802 §3):
//!
//! ```text
//! ClientSignature := HMAC(StoredKey, AuthMessage)
//! ClientKey       := ClientProof XOR ClientSignature
//! verify            H(ClientKey) == StoredKey        (constant time)
//! ServerSignature := HMAC(ServerKey, AuthMessage)
//! ```
//!
//! `ScramVerifierStartupHandler` is that server side. A Python
//! `ScramVerifierSource.get_scram_verifier(login)` returns a
//! `ScramVerifier` or `None`; `None` runs the exchange against a mock
//! verifier derived from a per-server secret (as PostgreSQL's
//! `scram_mock_salt` does), so an unknown user fails at the same step and
//! with the same error as a wrong password.
//!
//! Adapted from pgwire 0.41.0 (`src/api/auth/sasl.rs`,
//! `src/api/auth/sasl/scram.rs`, `src/api/auth/mod.rs`:
//! `register_connection`, `compute_cert_signature`, SCRAM message
//! grammar). pgwire is Copyright (c) 2018 Ning Sun and contributors,
//! licensed MIT OR Apache-2.0.

use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bytes::Bytes;
use futures::sink::{Sink, SinkExt};
use pgwire::api::auth::sasl::{SCRAM_SHA_256_METHOD, SCRAM_SHA_256_PLUS_METHOD};
use pgwire::api::auth::{
    finish_authentication, protocol_negotiation, save_startup_parameters_to_metadata,
    DefaultServerParameterProvider, LoginInfo, StartupHandler,
};
use pgwire::api::{
    ClientInfo, ConnectionGuard, ConnectionHandle, ConnectionManager, PgWireConnectionState,
    PidSecretKeyGenerator, RandomPidSecretKeyGenerator,
};
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::startup::Authentication;
use pgwire::messages::{PgWireBackendMessage, PgWireFrontendMessage};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_async_runtimes::tokio as pyo3_tokio;
use ring::{digest, hmac};
use subtle::ConstantTimeEq;
use x509_certificate::certificate::CapturedX509Certificate;
use x509_certificate::SignatureAlgorithm;

use crate::auth::PyLoginInfo;
use crate::errors::py_err_to_pywire;

// ---------- ScramVerifier ---------------------------------------------

/// A stored SCRAM-SHA-256 verifier: what PostgreSQL keeps in
/// `pg_authid.rolpassword` instead of the password.
#[pyclass(name = "ScramVerifier", module = "pywire.auth", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyScramVerifier {
    iterations: u32,
    salt: Vec<u8>,
    stored_key: Vec<u8>,
    server_key: Vec<u8>,
}

impl PyScramVerifier {
    fn new(
        iterations: u32,
        salt: Vec<u8>,
        stored_key: Vec<u8>,
        server_key: Vec<u8>,
    ) -> Result<Self, &'static str> {
        if iterations == 0 {
            return Err("iterations must be positive");
        }
        if salt.is_empty() {
            return Err("salt must not be empty");
        }
        if stored_key.len() != 32 || server_key.len() != 32 {
            return Err("stored_key and server_key must be 32-byte SHA-256 values");
        }
        Ok(Self {
            iterations,
            salt,
            stored_key,
            server_key,
        })
    }

    /// Parse `SCRAM-SHA-256$<iterations>:<salt>$<StoredKey>:<ServerKey>`.
    /// The error never echoes the input: it is credential material.
    fn parse(text: &str) -> Result<Self, &'static str> {
        const INVALID: &str =
            "not a SCRAM-SHA-256$<iterations>:<salt>$<StoredKey>:<ServerKey> verifier";
        let fields = text
            .strip_prefix("SCRAM-SHA-256$")
            .and_then(|rest| rest.split_once('$'))
            .and_then(|(params, keys)| Some((params.split_once(':')?, keys.split_once(':')?)));
        let Some(((iterations, salt), (stored_key, server_key))) = fields else {
            return Err(INVALID);
        };
        let b64 = |value: &str| STANDARD.decode(value).map_err(|_| INVALID);
        Self::new(
            iterations.parse().map_err(|_| INVALID)?,
            b64(salt)?,
            b64(stored_key)?,
            b64(server_key)?,
        )
    }
}

#[pymethods]
impl PyScramVerifier {
    #[new]
    fn py_new(
        iterations: u32,
        salt: &[u8],
        stored_key: &[u8],
        server_key: &[u8],
    ) -> PyResult<Self> {
        Self::new(
            iterations,
            salt.to_vec(),
            stored_key.to_vec(),
            server_key.to_vec(),
        )
        .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Parse PostgreSQL's `rolpassword` text form.
    #[staticmethod]
    #[pyo3(name = "parse")]
    fn py_parse(text: &str) -> PyResult<Self> {
        Self::parse(text).map_err(pyo3::exceptions::PyValueError::new_err)
    }

    #[getter]
    fn iterations(&self) -> u32 {
        self.iterations
    }

    #[getter]
    fn salt<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.salt)
    }

    #[getter]
    fn stored_key<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.stored_key)
    }

    #[getter]
    fn server_key<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.server_key)
    }

    fn __repr__(&self) -> String {
        format!(
            "ScramVerifier(iterations={}, <{}-byte salt>)",
            self.iterations,
            self.salt.len()
        )
    }
}

// ---------- SCRAM exchange (pure; unit tested) ------------------------

fn invalid(message: &str) -> PgWireError {
    PgWireError::InvalidScramMessage(message.to_owned())
}

fn utf8(data: &[u8]) -> PgWireResult<&str> {
    std::str::from_utf8(data).map_err(|_| invalid("SCRAM message is not UTF-8"))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), message)
        .as_ref()
        .to_vec()
}

/// `client-first-message`, split along the RFC 5802 grammar. The raw
/// gs2-header and bare message are kept verbatim: the client signs the
/// exact bytes it sent.
struct ClientFirst<'a> {
    cbind_flag: &'a str,
    gs2_header: &'a str,
    bare: &'a str,
    nonce: &'a str,
}

impl<'a> ClientFirst<'a> {
    fn parse(message: &'a str) -> PgWireResult<Self> {
        let malformed = || invalid("malformed SCRAM client-first-message");
        let (cbind_flag, rest) = message.split_once(',').ok_or_else(malformed)?;
        let (authzid, bare) = rest.split_once(',').ok_or_else(malformed)?;
        if !(authzid.is_empty() || authzid.starts_with("a=")) {
            return Err(malformed());
        }
        let mut parts = bare.split(',');
        // A leading `m=` is a mandatory extension; RFC 5802 §5.1 says the
        // server must fail the exchange, which the `n=` check does.
        let username = parts.next().and_then(|part| part.strip_prefix("n="));
        let nonce = parts.next().and_then(|part| part.strip_prefix("r="));
        match (username, nonce) {
            (Some(_), Some(nonce)) if !nonce.is_empty() => Ok(Self {
                cbind_flag,
                gs2_header: &message[..message.len() - bare.len()],
                bare,
                nonce,
            }),
            _ => Err(malformed()),
        }
    }
}

/// Channel-binding negotiation, following PostgreSQL's
/// `read_client_first_message`. `certificate` is the tls-server-end-point
/// hash when this connection is TLS and a certificate is configured
/// (SCRAM-SHA-256-PLUS was advertised). Returns the channel-binding data
/// the client must echo in `c=`.
fn negotiate<'a>(
    mechanism: &str,
    cbind_flag: &str,
    certificate: Option<&'a [u8]>,
) -> PgWireResult<&'a [u8]> {
    match (mechanism, cbind_flag, certificate) {
        (SCRAM_SHA_256_PLUS_METHOD, "p=tls-server-end-point", Some(signature)) => Ok(signature),
        (SCRAM_SHA_256_METHOD, "n", _) | (SCRAM_SHA_256_METHOD, "y", None) => Ok(&[]),
        // `y` with PLUS on offer is a downgrade; PLUS without `p=` (or `p=`
        // without PLUS) is a protocol violation.
        (SCRAM_SHA_256_METHOD | SCRAM_SHA_256_PLUS_METHOD, _, _) => {
            Err(invalid("SCRAM channel binding negotiation error"))
        }
        (other, _, _) => Err(PgWireError::UnsupportedSASLAuthMethod(other.to_owned())),
    }
}

/// Server state between server-first-message and client-final-message.
struct Exchange {
    user: String,
    verifier: PyScramVerifier,
    /// `true` when the verifier is the unknown-user mock: the exchange
    /// runs identically and then fails.
    mock: bool,
    nonce: String,
    channel_binding: String,
    server_first: String,
    /// `client-first-message-bare "," server-first-message`
    auth_prefix: String,
}

impl Exchange {
    fn new(
        first: &ClientFirst<'_>,
        cbind_data: &[u8],
        server_nonce: &str,
        user: String,
        verifier: PyScramVerifier,
        mock: bool,
    ) -> Self {
        let nonce = format!("{}{server_nonce}", first.nonce);
        let server_first = format!(
            "r={nonce},s={},i={}",
            STANDARD.encode(&verifier.salt),
            verifier.iterations
        );
        let channel_binding = STANDARD.encode([first.gs2_header.as_bytes(), cbind_data].concat());
        Self {
            user,
            verifier,
            mock,
            auth_prefix: format!("{},{server_first}", first.bare),
            nonce,
            channel_binding,
            server_first,
        }
    }

    /// Verify `client-final-message`; return `server-final-message`.
    fn verify(&self, message: &[u8]) -> PgWireResult<String> {
        let malformed = || invalid("malformed SCRAM client-final-message");
        let (without_proof, proof) = utf8(message)?.rsplit_once(",p=").ok_or_else(malformed)?;
        let mut parts = without_proof.split(',');
        let channel_binding = parts.next().and_then(|part| part.strip_prefix("c="));
        let nonce = parts.next().and_then(|part| part.strip_prefix("r="));
        let (Some(channel_binding), Some(nonce)) = (channel_binding, nonce) else {
            return Err(malformed());
        };
        if channel_binding != self.channel_binding {
            return Err(invalid("SCRAM channel binding check failed"));
        }
        if nonce != self.nonce {
            return Err(invalid("SCRAM nonce mismatch"));
        }
        let proof = STANDARD.decode(proof).map_err(|_| malformed())?;
        if proof.len() != digest::SHA256_OUTPUT_LEN {
            return Err(malformed());
        }

        let auth_message = format!("{},{without_proof}", self.auth_prefix);
        let client_signature = hmac_sha256(&self.verifier.stored_key, auth_message.as_bytes());
        let client_key: Vec<u8> = proof
            .iter()
            .zip(&client_signature)
            .map(|(p, s)| p ^ s)
            .collect();
        let computed = digest::digest(&digest::SHA256, &client_key);
        let proof_ok: bool = computed.as_ref().ct_eq(&self.verifier.stored_key).into();
        if !proof_ok || self.mock {
            return Err(PgWireError::InvalidPassword(self.user.clone()));
        }
        let server_signature = hmac_sha256(&self.verifier.server_key, auth_message.as_bytes());
        Ok(format!("v={}", STANDARD.encode(server_signature)))
    }
}

/// The verifier an unknown user is checked against: stable per user for
/// the life of the server (so repeated probes see the same salt), random
/// across servers, and never accepted.
fn mock_verifier(secret: &[u8], user: &str, iterations: u32) -> PyScramVerifier {
    let salt = hmac_sha256(secret, format!("salt\0{user}").as_bytes());
    let key = hmac_sha256(secret, format!("key\0{user}").as_bytes());
    PyScramVerifier {
        iterations,
        salt: salt[..16].to_vec(),
        stored_key: key.clone(),
        server_key: key,
    }
}

/// tls-server-end-point channel-binding data (RFC 5929 §4.1): the hash of
/// the DER certificate, SHA-256 for MD5/SHA-1 signatures.
pub(crate) fn compute_cert_signature(pem: &[u8]) -> PgWireResult<Vec<u8>> {
    let certificates = CapturedX509Certificate::from_pem_multiple(pem)
        .map_err(|error| PgWireError::ApiError(Box::new(error)))?;
    let certificate = certificates
        .first()
        .ok_or(PgWireError::UnsupportedCertificateSignatureAlgorithm)?;
    let algorithm = match certificate.signature_algorithm() {
        Some(
            SignatureAlgorithm::RsaSha1
            | SignatureAlgorithm::RsaSha256
            | SignatureAlgorithm::EcdsaSha256,
        ) => &digest::SHA256,
        Some(SignatureAlgorithm::RsaSha384 | SignatureAlgorithm::EcdsaSha384) => &digest::SHA384,
        Some(SignatureAlgorithm::RsaSha512) => &digest::SHA512,
        _ => return Err(PgWireError::UnsupportedCertificateSignatureAlgorithm),
    };
    Ok(digest::digest(algorithm, certificate.constructed_data())
        .as_ref()
        .to_vec())
}

// ---------- StartupHandler --------------------------------------------

/// Shared, per-server configuration for the verifier handler.
pub(crate) struct ScramVerifierConfig {
    pub(crate) source: Arc<Py<PyAny>>,
    /// tls-server-end-point hash of the configured certificate.
    pub(crate) certificate: Option<Arc<Vec<u8>>>,
    pub(crate) mock_secret: [u8; 32],
    pub(crate) mock_iterations: u32,
}

enum State {
    Initial,
    ServerFirstSent(Box<Exchange>),
    Done,
}

/// Per-connection SCRAM-SHA-256(-PLUS) startup handler.
pub(crate) struct ScramVerifierStartupHandler {
    config: Arc<ScramVerifierConfig>,
    manager: Arc<ConnectionManager>,
    parameters: DefaultServerParameterProvider,
    pids: RandomPidSecretKeyGenerator,
    state: Mutex<State>,
}

impl ScramVerifierStartupHandler {
    pub(crate) fn new(config: Arc<ScramVerifierConfig>, manager: Arc<ConnectionManager>) -> Self {
        Self {
            config,
            manager,
            parameters: DefaultServerParameterProvider::default(),
            pids: RandomPidSecretKeyGenerator::default(),
            state: Mutex::new(State::Initial),
        }
    }

    fn certificate<C: ClientInfo>(&self, client: &C) -> Option<&[u8]> {
        self.config
            .certificate
            .as_deref()
            .map(Vec::as_slice)
            .filter(|_| client.is_secure())
    }

    fn take_state(&self) -> State {
        std::mem::replace(&mut *self.state.lock().expect("state lock"), State::Done)
    }

    async fn lookup(&self, login: PyLoginInfo) -> PgWireResult<Option<PyScramVerifier>> {
        let to_pgwire = |error: PyErr| Python::attach(|py| py_err_to_pywire(py, error));
        let future = Python::attach(|py| {
            let coroutine = self
                .config
                .source
                .bind(py)
                .call_method1("get_scram_verifier", (login,))?;
            pyo3_tokio::into_future(coroutine)
        })
        .map_err(to_pgwire)?;
        let result = future.await.map_err(to_pgwire)?;
        Python::attach(|py| {
            result
                .bind(py)
                .extract::<Option<PyScramVerifier>>()
                .map_err(PyErr::from)
        })
        .map_err(to_pgwire)
    }

    async fn start<C: ClientInfo + Sync>(
        &self,
        client: &C,
        mechanism: &str,
        data: Option<Bytes>,
    ) -> PgWireResult<Exchange> {
        let data = data.ok_or_else(|| invalid("empty SCRAM client-first-message"))?;
        let first = ClientFirst::parse(utf8(&data)?)?;
        let cbind_data = negotiate(mechanism, first.cbind_flag, self.certificate(client))?;
        // PostgreSQL clients send an empty SCRAM username; the startup
        // `user` is authoritative.
        let login = PyLoginInfo::from_pg(&LoginInfo::from_client_info(client));
        let user = login.user.clone().unwrap_or_default();
        let (verifier, mock) = match self.lookup(login).await? {
            Some(verifier) => (verifier, false),
            None => (
                mock_verifier(&self.config.mock_secret, &user, self.config.mock_iterations),
                true,
            ),
        };
        let server_nonce = STANDARD.encode(rand::random::<[u8; 18]>());
        Ok(Exchange::new(
            &first,
            cbind_data,
            &server_nonce,
            user,
            verifier,
            mock,
        ))
    }
}

/// Copy of pgwire's crate-private `register_connection` (auth/mod.rs).
fn register_connection<C: ClientInfo>(client: &C, manager: &Arc<ConnectionManager>) {
    let (pid, secret_key) = client.pid_and_secret_key();
    let (handle, guard) = manager.register(pid, secret_key);
    client
        .session_extensions()
        .insert::<Arc<ConnectionHandle>>(handle);
    client.session_extensions().insert::<ConnectionGuard>(guard);
}

#[async_trait]
impl StartupHandler for ScramVerifierStartupHandler {
    async fn on_startup<C>(
        &self,
        client: &mut C,
        message: PgWireFrontendMessage,
    ) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        match message {
            PgWireFrontendMessage::Startup(ref startup) => {
                protocol_negotiation(client, startup).await?;
                save_startup_parameters_to_metadata(client, startup);
                client.set_state(PgWireConnectionState::AuthenticationInProgress);
                let mut mechanisms = vec![SCRAM_SHA_256_METHOD.to_owned()];
                if self.certificate(client).is_some() {
                    mechanisms.push(SCRAM_SHA_256_PLUS_METHOD.to_owned());
                }
                client
                    .send(PgWireBackendMessage::Authentication(Authentication::SASL(
                        mechanisms,
                    )))
                    .await?;
            }
            PgWireFrontendMessage::PasswordMessageFamily(message) => match self.take_state() {
                State::Initial => {
                    let initial = message.into_sasl_initial_response()?;
                    let exchange = self
                        .start(client, &initial.auth_method, initial.data)
                        .await?;
                    let server_first = Bytes::from(exchange.server_first.clone());
                    *self.state.lock().expect("state lock") =
                        State::ServerFirstSent(Box::new(exchange));
                    client
                        .send(PgWireBackendMessage::Authentication(
                            Authentication::SASLContinue(server_first),
                        ))
                        .await?;
                }
                State::ServerFirstSent(exchange) => {
                    let response = message.into_sasl_response()?;
                    let server_final = exchange.verify(&response.data)?;
                    client
                        .send(PgWireBackendMessage::Authentication(
                            Authentication::SASLFinal(Bytes::from(server_final)),
                        ))
                        .await?;
                    let (pid, secret_key) = self.pids.generate(client);
                    client.set_pid_and_secret_key(pid, secret_key);
                    register_connection(client, &self.manager);
                    finish_authentication(client, &self.parameters).await?;
                }
                State::Done => return Err(PgWireError::InvalidSASLState),
            },
            _ => {}
        }
        Ok(())
    }
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyScramVerifier>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    // RFC 7677 §3 test vector.
    const PASSWORD: &[u8] = b"pencil";
    const SALT_B64: &str = "W22ZaJ0SNY7soEsUEjb6gQ==";
    const CLIENT_FIRST: &str = "n,,n=user,r=rOprNGfwEbeRWgbNEkqO";
    const SERVER_NONCE: &str = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";
    const SERVER_FIRST: &str =
        "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
    const CLIENT_FINAL: &str = "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=";
    const SERVER_FINAL: &str = "v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=";

    fn verifier_for(password: &[u8], salt: &[u8], iterations: u32) -> PyScramVerifier {
        let mut salted = [0u8; 32];
        ring::pbkdf2::derive(
            ring::pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(iterations).unwrap(),
            salt,
            password,
            &mut salted,
        );
        let client_key = hmac_sha256(&salted, b"Client Key");
        PyScramVerifier::new(
            iterations,
            salt.to_vec(),
            digest::digest(&digest::SHA256, &client_key)
                .as_ref()
                .to_vec(),
            hmac_sha256(&salted, b"Server Key"),
        )
        .unwrap()
    }

    fn rfc_verifier() -> PyScramVerifier {
        verifier_for(PASSWORD, &STANDARD.decode(SALT_B64).unwrap(), 4096)
    }

    fn rfc_exchange(verifier: PyScramVerifier, mock: bool) -> Exchange {
        let first = ClientFirst::parse(CLIENT_FIRST).unwrap();
        Exchange::new(&first, &[], SERVER_NONCE, "user".into(), verifier, mock)
    }

    fn assert_invalid_password(result: PgWireResult<String>) {
        assert!(matches!(result, Err(PgWireError::InvalidPassword(user)) if user == "user"));
    }

    fn assert_invalid_message(result: PgWireResult<String>) {
        assert!(matches!(result, Err(PgWireError::InvalidScramMessage(_))));
    }

    #[test]
    fn rfc7677_vector_verifies() {
        let exchange = rfc_exchange(rfc_verifier(), false);
        assert_eq!(exchange.server_first, SERVER_FIRST);
        assert_eq!(exchange.channel_binding, "biws");
        assert_eq!(
            exchange.verify(CLIENT_FINAL.as_bytes()).unwrap(),
            SERVER_FINAL
        );
    }

    #[test]
    fn wrong_password_is_rejected() {
        let salt = STANDARD.decode(SALT_B64).unwrap();
        let exchange = rfc_exchange(verifier_for(b"pencil!", &salt, 4096), false);
        assert_invalid_password(exchange.verify(CLIENT_FINAL.as_bytes()));
    }

    #[test]
    fn every_proof_bit_matters() {
        // The proof check is a constant-time comparison of H(ClientKey)
        // against StoredKey; flipping any byte must fail it.
        let exchange = rfc_exchange(rfc_verifier(), false);
        let (prefix, proof) = CLIENT_FINAL.rsplit_once(",p=").unwrap();
        let proof = STANDARD.decode(proof).unwrap();
        for index in [0, 15, 31] {
            let mut tampered = proof.clone();
            tampered[index] ^= 1;
            let message = format!("{prefix},p={}", STANDARD.encode(tampered));
            assert_invalid_password(exchange.verify(message.as_bytes()));
        }
    }

    #[test]
    fn mock_verifier_never_succeeds() {
        // Even a proof that checks out against the mock verifier fails.
        let exchange = rfc_exchange(rfc_verifier(), true);
        assert_invalid_password(exchange.verify(CLIENT_FINAL.as_bytes()));

        let secret = [7u8; 32];
        let first = mock_verifier(&secret, "ghost", 4096);
        let again = mock_verifier(&secret, "ghost", 4096);
        assert_eq!(first.salt, again.salt);
        assert_eq!(first.salt.len(), 16);
        assert_ne!(first.salt, mock_verifier(&secret, "other", 4096).salt);
        assert_ne!(first.salt, mock_verifier(&[8; 32], "ghost", 4096).salt);
    }

    #[test]
    fn client_final_checks() {
        let exchange = rfc_exchange(rfc_verifier(), false);
        let (prefix, proof) = CLIENT_FINAL.rsplit_once(",p=").unwrap();
        let nonce = prefix.strip_prefix("c=biws,").unwrap();
        for message in [
            "c=biws".to_owned(),
            format!("{nonce},p={proof}"),
            format!("c=biws,p={proof}"),
            format!("c=eSws,{nonce},p={proof}"),
            format!("c=biws,r=other,p={proof}"),
            format!("{prefix},p=!!"),
            format!("{prefix},p=AAAA"),
        ] {
            assert_invalid_message(exchange.verify(message.as_bytes()));
        }
        assert_invalid_message(exchange.verify(&[0xff]));
    }

    #[test]
    fn client_final_extensions_are_signed() {
        // An extension before the proof is part of AuthMessage, so a proof
        // computed without it no longer verifies.
        let exchange = rfc_exchange(rfc_verifier(), false);
        let (prefix, proof) = CLIENT_FINAL.rsplit_once(",p=").unwrap();
        let message = format!("{prefix},x=ext,p={proof}");
        assert_invalid_password(exchange.verify(message.as_bytes()));
    }

    #[test]
    fn client_first_parsing() {
        let first = ClientFirst::parse("p=tls-server-end-point,a=bob,n=,r=abc,x=1").unwrap();
        assert_eq!(first.cbind_flag, "p=tls-server-end-point");
        assert_eq!(first.gs2_header, "p=tls-server-end-point,a=bob,");
        assert_eq!(first.bare, "n=,r=abc,x=1");
        assert_eq!(first.nonce, "abc");
        for message in [
            "n",
            "n,",
            "n,b=bob,n=user,r=abc",
            "n,,m=ext,n=user,r=abc",
            "n,,n=user",
            "n,,n=user,r=",
            "n,,r=abc",
        ] {
            assert!(ClientFirst::parse(message).is_err(), "{message}");
        }
    }

    #[test]
    fn channel_binding_negotiation() {
        let signature: &[u8] = &[1, 2, 3];
        let plus = "p=tls-server-end-point";
        assert_eq!(
            negotiate(SCRAM_SHA_256_PLUS_METHOD, plus, Some(signature)).unwrap(),
            signature
        );
        assert!(negotiate(SCRAM_SHA_256_METHOD, "n", Some(signature))
            .unwrap()
            .is_empty());
        assert!(negotiate(SCRAM_SHA_256_METHOD, "y", None)
            .unwrap()
            .is_empty());
        for (mechanism, flag, certificate) in [
            (SCRAM_SHA_256_METHOD, "y", Some(signature)),
            (SCRAM_SHA_256_METHOD, plus, Some(signature)),
            (SCRAM_SHA_256_PLUS_METHOD, "n", Some(signature)),
            (SCRAM_SHA_256_PLUS_METHOD, plus, None),
            (SCRAM_SHA_256_PLUS_METHOD, "p=tls-unique", Some(signature)),
        ] {
            assert!(matches!(
                negotiate(mechanism, flag, certificate),
                Err(PgWireError::InvalidScramMessage(_))
            ));
        }
        assert!(matches!(
            negotiate("SCRAM-SHA-1", "n", None),
            Err(PgWireError::UnsupportedSASLAuthMethod(_))
        ));
    }

    #[test]
    fn channel_binding_includes_certificate_hash() {
        let first = ClientFirst::parse("p=tls-server-end-point,,n=,r=abc").unwrap();
        let exchange = Exchange::new(&first, &[9, 9], "xyz", "u".into(), rfc_verifier(), false);
        let expected = STANDARD.encode(b"p=tls-server-end-point,,\x09\x09");
        assert_eq!(exchange.channel_binding, expected);
    }

    #[test]
    fn verifier_parse_round_trips_postgres_format() {
        let verifier = rfc_verifier();
        let text = format!(
            "SCRAM-SHA-256${}:{}${}:{}",
            verifier.iterations,
            STANDARD.encode(&verifier.salt),
            STANDARD.encode(&verifier.stored_key),
            STANDARD.encode(&verifier.server_key)
        );
        let parsed = PyScramVerifier::parse(&text).unwrap();
        assert_eq!(parsed.iterations, 4096);
        assert_eq!(parsed.salt, verifier.salt);
        assert_eq!(parsed.stored_key, verifier.stored_key);
        assert_eq!(parsed.server_key, verifier.server_key);

        let key = STANDARD.encode([0u8; 32]);
        for bad in [
            "md5abc".to_owned(),
            "SCRAM-SHA-256$4096:c2FsdA==".to_owned(),
            format!("SCRAM-SHA-256$4096$c2FsdA==${key}:{key}"),
            format!("SCRAM-SHA-256$4096:c2FsdA==${key}"),
            format!("SCRAM-SHA-256$x:c2FsdA==${key}:{key}"),
            format!("SCRAM-SHA-256$4096:!!${key}:{key}"),
            format!("SCRAM-SHA-256$4096:c2FsdA==$!!:{key}"),
            format!("SCRAM-SHA-256$4096:c2FsdA==${key}:!!"),
            format!("SCRAM-SHA-256$0:c2FsdA==${key}:{key}"),
            format!("SCRAM-SHA-256$4096:${key}:{key}"),
            format!("SCRAM-SHA-256$4096:c2FsdA==$AAAA:{key}"),
        ] {
            let error = PyScramVerifier::parse(&bad).err().unwrap();
            assert!(!error.contains(&key), "error must not echo the input");
        }
    }

    #[test]
    fn certificate_signature() {
        let rsa_sha256 = include_bytes!("../tests/fixtures/server.crt");
        assert_eq!(compute_cert_signature(rsa_sha256).unwrap().len(), 32);
        let ecdsa_sha384 = include_bytes!("../tests/fixtures/ecdsa-sha384.crt");
        assert_eq!(compute_cert_signature(ecdsa_sha384).unwrap().len(), 48);
        let rsa_sha512 = include_bytes!("../tests/fixtures/rsa-sha512.crt");
        assert_eq!(compute_cert_signature(rsa_sha512).unwrap().len(), 64);
        assert!(matches!(
            compute_cert_signature(include_bytes!("../tests/fixtures/ed25519.crt")),
            Err(PgWireError::UnsupportedCertificateSignatureAlgorithm)
        ));
        assert!(compute_cert_signature(b"not a certificate").is_err());
        assert!(compute_cert_signature(
            b"-----BEGIN CERTIFICATE-----\n!!\n-----END CERTIFICATE-----\n"
        )
        .is_err());
    }
}
