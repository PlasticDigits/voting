//! Captured INFO-level production tracing with real HTTP and PostgreSQL failure paths.
//! Every credential is an unfunded synthetic sentinel, never a deployment secret.
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::any,
    Router,
};
use http_body_util::BodyExt;
use operator_voting::{api::AppState, application::application, VotingConfig};
use std::{
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
use tower::ServiceExt;
use tracing::instrument::WithSubscriber;

const USER: &str = "SENTINEL_USER";
const PASSWORD: &str = "SENTINEL_BASIC_PASS";
const QUERY: &str = "SENTINEL_QUERY";
const SQL: &str = "SENTINEL_SQL_MESSAGE";

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}
impl Capture {
    fn dispatch(&self) -> tracing::Dispatch {
        // Both production binaries add this INFO directive to their EnvFilter.
        tracing::Dispatch::new(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_ansi(false)
                .without_time()
                .with_writer(self.clone())
                .finish(),
        )
    }
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}
struct Fixture(JoinHandle<()>);
impl Drop for Fixture {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn assert_no_credentials(text: &str) {
    use base64::Engine;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{PASSWORD}"));
    for needle in [
        USER,
        PASSWORD,
        QUERY,
        SQL,
        "private-provider.example",
        basic.as_str(),
    ] {
        assert!(
            !text.contains(needle),
            "credential marker escaped redaction"
        );
    }
}
fn record_checked_log(name: &str, log: &str) {
    assert_no_credentials(log);
    if let Ok(directory) = std::env::var("VOTING_REDACTION_CAPTURE_DIR") {
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(directory.join(name))
            .unwrap()
            .write_all(log.as_bytes())
            .unwrap();
    }
}

/// A real PostgreSQL ErrorResponse at connection startup, containing hostile details.
async fn database_error_fixture() -> (u16, Fixture) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let length = match stream.read_i32().await {
                Ok(length) => length,
                Err(_) => continue,
            };
            if !(8..=16 * 1024).contains(&length) {
                continue;
            }
            let mut startup = vec![0; length as usize - 4];
            if stream.read_exact(&mut startup).await.is_err() {
                continue;
            }
            let message = format!(
                "{SQL} https://{USER}:{PASSWORD}@private-provider.example/?api_key={QUERY}"
            );
            let mut fields = b"SFATAL\0VFATAL\0C28000\0M".to_vec();
            fields.extend_from_slice(message.as_bytes());
            fields.extend_from_slice(b"\0\0");
            stream.write_u8(b'E').await.unwrap();
            stream.write_i32(fields.len() as i32 + 4).await.unwrap();
            stream.write_all(&fields).await.unwrap();
        }
    });
    (port, Fixture(task))
}

#[tokio::test]
async fn production_api_logs_and_responses_redact_database_and_hostile_requests() {
    use base64::Engine;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
    let (port, _fixture) = database_error_fixture().await;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(1))
        .connect_lazy_with(
            PgConnectOptions::new()
                .host("127.0.0.1")
                .port(port)
                .username(USER)
                .password(PASSWORD)
                .database("isolated_sentinel_fixture")
                .ssl_mode(PgSslMode::Disable),
        );
    // Prove the fixture reaches the genuine database-error variant, not just pool timeout.
    let error = pool.acquire().await.unwrap_err();
    assert!(matches!(error, sqlx::Error::Database(_)));
    assert!(error.to_string().contains(SQL));
    let mut cfg = VotingConfig::for_tests("unused", "");
    cfg.run_mode = "production".into();
    cfg.apply_migrations = false;
    cfg.cors_origins = vec!["https://vote.cl8y.com".into()];
    let router = application(AppState { pool, cfg }).unwrap();
    let capture = Capture::default();
    let dispatch = capture.dispatch();
    let mut responses = String::new();
    async {
        let hostile =
            format!("https://{USER}:{PASSWORD}@private-provider.example/?api_key={QUERY}");
        let cases = [
            (
                "GET",
                format!("/v1/proposals?secret={QUERY}"),
                String::new(),
                500,
                "internal_error",
            ),
            (
                "POST",
                format!("/v1/register?secret={QUERY}"),
                format!("{{\"sentinel\":\"{hostile}\""),
                400,
                "bad_request",
            ),
            (
                "GET",
                format!("/v1/balances/0x1111111111111111111111111111111111111111?height={QUERY}"),
                String::new(),
                400,
                "bad_request",
            ),
            (
                "GET",
                format!("/v1/proposals/{QUERY}"),
                String::new(),
                400,
                "bad_request",
            ),
            (
                "GET",
                format!("/unknown?private_url={hostile}"),
                String::new(),
                404,
                "not_found",
            ),
        ];
        let authorization = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{PASSWORD}"))
        );
        for (method, path, body, status, code) in cases {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("origin", "https://vote.cl8y.com")
                        .header("authorization", &authorization)
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            let request_id = response.headers()["x-request-id"]
                .to_str()
                .unwrap()
                .to_owned();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let body = String::from_utf8(bytes.to_vec()).unwrap();
            assert_no_credentials(&body);
            let value: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(value["code"], code);
            assert_eq!(value["request_id"], request_id);
            responses.push_str(&body);
            responses.push('\n');
        }
    }
    .with_subscriber(dispatch)
    .await;
    let log = capture.text();
    assert_eq!(log.matches("API request rejected").count(), 5);
    for status in ["status=500", "status=400", "status=404"] {
        assert!(
            log.contains(status),
            "safe HTTP failure classification missing"
        );
    }
    assert!(log.contains("request_id="));
    assert!(log.contains("http_request"));
    record_checked_log("api-info.log", &log);
    record_checked_log("api-responses.jsonl", &responses);
}

#[derive(Clone, Copy)]
enum Failure {
    Http,
    Json,
    Timeout,
}
async fn provider_fixture(failure: Failure) -> (String, Fixture) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = Router::new().fallback(any(move || async move {
        let body =
            format!("{SQL} https://{USER}:{PASSWORD}@private-provider.example/?api_key={QUERY}");
        match failure {
            Failure::Http => (StatusCode::SERVICE_UNAVAILABLE, body),
            Failure::Json => (StatusCode::OK, body),
            Failure::Timeout => {
                tokio::time::sleep(Duration::from_secs(30)).await;
                (StatusCode::OK, body)
            }
        }
    }));
    let task = tokio::spawn(async move {
        axum::serve(listener, server).await.unwrap();
    });
    (
        format!("http://{USER}:{PASSWORD}@{address}/private?api_key={QUERY}"),
        Fixture(task),
    )
}

#[tokio::test]
async fn provider_http_json_and_timeouts_have_redacted_captured_error_logs() {
    use voting_ledger::{bsc::BscClient, lcd::LcdClient, LedgerError};
    let capture = Capture::default();
    async {
        for failure in [Failure::Http, Failure::Json, Failure::Timeout] {
            let (url, _fixture) = provider_fixture(failure).await;
            // Real adapter deadlines are eight seconds; both chains run concurrently.
            let lcd = LcdClient::new(vec![url.clone()]);
            let bsc = BscClient::new(vec![url]);
            let (terra, bsc) = tokio::time::timeout(Duration::from_secs(10), async {
                tokio::join!(lcd.latest_height(), bsc.block_number())
            }).await.expect("adapter transport deadline must be bounded");
            for (chain, error) in [("terra", terra.unwrap_err()), ("bsc", bsc.unwrap_err())] {
                let expected = match failure {
                    Failure::Http => "http_status",
                    Failure::Json => "invalid_json_schema",
                    Failure::Timeout if chain == "terra" => "lcd_transport",
                    Failure::Timeout => "bsc_transport",
                };
                assert!(matches!(error, LedgerError::Provider(class) if class == expected));
                assert_no_credentials(&format!("{error} {error:?}"));
                // Exercise both Display and Debug through the actual tracing formatter.
                tracing::warn!(chain, error = %error, error_debug = ?error, "provider request rejected");
            }
        }
        let database = LedgerError::Database(sqlx::Error::Protocol(format!("{SQL} {USER}:{PASSWORD} {QUERY}")));
        tracing::warn!(error = %database, error_debug = ?database, "ledger database rejected");
    }.with_subscriber(capture.dispatch()).await;
    let log = capture.text();
    assert_eq!(log.matches("provider request rejected").count(), 6);
    for class in [
        "http_status",
        "invalid_json_schema",
        "lcd_transport",
        "bsc_transport",
        "database error",
    ] {
        assert!(log.contains(class), "sanitized failure class missing");
    }
    record_checked_log("provider-info.log", &log);
}
