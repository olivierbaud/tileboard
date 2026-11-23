//! A small HTTP client for calling apps: GET only, plain `http://`, no redirects,
//! and a time limit and a size limit on every call.

use std::error::Error;
use std::fmt;
use std::time::Duration;

use axum::body::Bytes;
use axum::http::header::{ACCEPT, AUTHORIZATION};
use axum::http::{HeaderValue, Request, StatusCode};
use http_body_util::{BodyExt, Empty, LengthLimitError, Limited};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

/// No answer may be larger than this.
pub const MAX_BODY: usize = 1024 * 1024;

/// How much of an app's own error message is passed on.
const MAX_MESSAGE: usize = 200;

#[derive(Clone)]
pub struct HttpClient {
    inner: Client<HttpConnector, Empty<Bytes>>,
    timeout: Duration,
}

/// Why a call to an app gave no usable answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// No complete answer within the time limit.
    Timeout(Duration),
    /// No connection, or it broke.
    Unreachable(String),
    /// The app answered, but not with 200. `message` is the app's own `error` text, if it sent one.
    Status { code: u16, message: Option<String> },
    /// The answer is larger than [`MAX_BODY`].
    TooBig,
    /// The request could not even be built.
    Unusable(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Timeout(limit) => write!(f, "the app did not answer within {} s", limit.as_secs_f32()),
            FetchError::Unreachable(cause) => write!(f, "cannot reach the app ({cause})"),
            FetchError::Status { code: 401, .. } => write!(f, "the app answered 401: the token is missing or wrong"),
            FetchError::Status { code, message: Some(message) } => write!(f, "the app answered {code}: {message}"),
            FetchError::Status { code, message: None } => write!(f, "the app answered {code}"),
            FetchError::TooBig => write!(f, "the app's answer is larger than {} MB", MAX_BODY / (1024 * 1024)),
            FetchError::Unusable(why) => write!(f, "{why}"),
        }
    }
}

impl HttpClient {
    /// A client whose every call, from connecting to the last byte, gets at most `timeout`.
    pub fn new(timeout: Duration) -> HttpClient {
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(timeout));
        connector.set_nodelay(true);
        let inner = Client::builder(TokioExecutor::new()).pool_idle_timeout(Duration::from_secs(30)).build(connector);
        HttpClient { inner, timeout }
    }

    /// `GET url`, with the bearer `token` if there is one. Only a `200` is a success.
    pub async fn get(&self, url: &str, token: Option<&str>) -> Result<Bytes, FetchError> {
        let mut request = Request::get(url).header(ACCEPT, "application/json");
        if let Some(token) = token {
            let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| FetchError::Unusable("the token cannot be sent in a header".to_string()))?;
            value.set_sensitive(true);
            request = request.header(AUTHORIZATION, value);
        }
        let request = request
            .body(Empty::<Bytes>::new())
            .map_err(|_| FetchError::Unusable("the app's address is not valid".to_string()))?;

        let call = async {
            let response = self.inner.request(request).await.map_err(|e| FetchError::Unreachable(deepest_cause(&e)))?;
            let status = response.status();
            let body = Limited::new(response.into_body(), MAX_BODY)
                .collect()
                .await
                .map_err(|e| match e.downcast_ref::<LengthLimitError>() {
                    Some(_) => FetchError::TooBig,
                    None => FetchError::Unreachable(deepest_cause(e.as_ref())),
                })?
                .to_bytes();
            if status == StatusCode::OK {
                Ok(body)
            } else {
                Err(FetchError::Status { code: status.as_u16(), message: error_message(&body) })
            }
        };
        tokio::time::timeout(self.timeout, call).await.unwrap_or(Err(FetchError::Timeout(self.timeout)))
    }
}

/// The innermost reason of an error: "connection refused" rather than "client error (Connect)".
fn deepest_cause(error: &(dyn Error + 'static)) -> String {
    let mut cause = error;
    while let Some(next) = cause.source() {
        cause = next;
    }
    cause.to_string()
}

/// The `error` text of an app's error body, cut short and with control characters removed.
fn error_message(body: &[u8]) -> Option<String> {
    let doc: serde_json::Value = serde_json::from_slice(body).ok()?;
    let text: String = doc.get("error")?.as_str()?.chars().filter(|c| !c.is_control()).take(MAX_MESSAGE).collect();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_read_like_sentences() {
        let cases = [
            (FetchError::Timeout(Duration::from_secs(5)), "the app did not answer within 5 s"),
            (FetchError::Timeout(Duration::from_millis(500)), "the app did not answer within 0.5 s"),
            (FetchError::Unreachable("connection refused".into()), "cannot reach the app (connection refused)"),
            (FetchError::Status { code: 500, message: None }, "the app answered 500"),
            (
                FetchError::Status { code: 404, message: Some("no such source".into()) },
                "the app answered 404: no such source",
            ),
            (
                FetchError::Status { code: 401, message: Some("go away".into()) },
                "the app answered 401: the token is missing or wrong",
            ),
            (FetchError::TooBig, "the app's answer is larger than 1 MB"),
        ];
        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn an_apps_error_text_is_passed_on_cleaned_up() {
        assert_eq!(error_message(br#"{"error": "no such source"}"#), Some("no such source".to_string()));
        assert_eq!(error_message(br#"{"error": "line one\nline two\u0007"}"#), Some("line oneline two".to_string()));
        let long = format!(r#"{{"error": "{}"}}"#, "x".repeat(5000));
        assert_eq!(error_message(long.as_bytes()).unwrap().len(), MAX_MESSAGE);
        for nothing in [&b"not json"[..], br#"{"message": "x"}"#, br#"{"error": 5}"#, br#"{"error": ""}"#, b""] {
            assert_eq!(error_message(nothing), None);
        }
    }
}
