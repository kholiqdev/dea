//! Compares the time `dea-http` takes per request with curl's, against a
//! local server (F1-REQ-06 #6: at most 10 ms of overhead, by the median of
//! 100 requests).
//!
//! Dea uses a new `Client` per request, so every request opens a new
//! connection, as a curl process does. curl's own `time_total` is read, so
//! the cost of starting the process is not counted.

use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, Options, ProxyConfig, Request, SendContext,
    TlsConfig,
};
use dea_testkit::http::{TestServer, Tls};

const WARMUP: usize = 5;
const SAMPLES: usize = 100;
const LIMIT: Duration = Duration::from_millis(10);

#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort();
    let mid = samples.len() / 2;
    if samples.len().is_multiple_of(2) {
        (samples[mid - 1] + samples[mid]) / 2
    } else {
        samples[mid]
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

async fn dea_once(url: &str) -> Duration {
    let client = Client::new(ClientConfig {
        proxy: ProxyConfig::None,
        ..ClientConfig::default()
    })
    .expect("build the client");
    let request = Request {
        method: "GET".into(),
        url: url.parse().expect("parse the url"),
        headers: Vec::new(),
        credential_headers: Vec::new(),
        body: Body::Empty,
        options: Options::default(),
        tls: TlsConfig::default(),
    };
    let cancel = CancellationToken::new();
    let start = Instant::now();
    let response = client
        .send(request, SendContext::new(&cancel))
        .await
        .expect("send the request");
    let elapsed = start.elapsed();
    assert_eq!(response.status, 200);
    elapsed
}

/// curl's own total time, or `None` when curl cannot run.
fn curl_once(url: &str) -> Option<Duration> {
    let output = Command::new("curl")
        .args(["-s", "-o", NULL_DEVICE, "-w", "%{time_total}", url])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let seconds: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs_f64(seconds))
}

#[tokio::main]
async fn main() -> ExitCode {
    if Command::new("curl").arg("--version").output().is_err() {
        println!("curl not found, skipped");
        return ExitCode::SUCCESS;
    }

    let server = TestServer::start(Tls::None).await;
    let url = server.url("/status/200");

    for _ in 0..WARMUP {
        dea_once(&url).await;
        curl_once(&url).expect("curl warm-up request");
    }

    let mut dea = Vec::with_capacity(SAMPLES);
    let mut curl = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        dea.push(dea_once(&url).await);
        curl.push(curl_once(&url).expect("curl request"));
    }

    let dea = median(&mut dea);
    let curl = median(&mut curl);
    let overhead = ms(dea) - ms(curl);
    println!("dea median:  {:.3} ms", ms(dea));
    println!("curl median: {:.3} ms", ms(curl));
    println!(
        "overhead:    {:.3} ms (limit {:.0} ms)",
        overhead,
        ms(LIMIT)
    );

    if overhead > ms(LIMIT) {
        eprintln!("overhead is over the limit");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
