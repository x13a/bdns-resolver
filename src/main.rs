use std::env;
use std::sync::{Arc, Mutex};

use rand::rngs::SmallRng;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use tide::{Request, Response, StatusCode};
use tide::prelude::*;
use tide::utils::After;
use tide_rustls::TlsListener;
use trust_dns_resolver::config::{NameServerConfigGroup, ResolverConfig, ResolverOpts};
use trust_dns_resolver::TokioAsyncResolver;

const ENV_DNS: &'static str = "DNS";
const ENV_ADDR: &'static str = "ADDR";
const ENV_CERT_FILE: &'static str = "CERT_FILE";
const ENV_KEY_FILE: &'static str = "KEY_FILE";

const DEFAULT_DNS: &'static str = "127.0.0.1:5353";
const DEFAULT_ADDR: &'static str = "127.0.0.1:8000";

const NOT_FOUND: &'static str = "nx";
const EXISTS: &'static str = "xx";

#[derive(Deserialize)]
#[serde(default)]
struct ResolveQuery {
    n: u8,
    r: u8,
}

impl Default for ResolveQuery {
    fn default() -> Self {
        Self { n: 8, r: 1 }
    }
}

#[derive(Clone)]
pub struct State {
    resolver: Arc<TokioAsyncResolver>,
    rng: Arc<Mutex<SmallRng>>,
}

async fn resolve(req: Request<State>) -> tide::Result {
    let host = req.param("host")?;
    if !validate_host(host) {
        return Ok(Response::builder(StatusCode::BadRequest).build());
    }
    let query: ResolveQuery = req.query()?;
    let state = req.state();
    let addrs = state.resolver.lookup_ip(host).await?;
    let mut results = addrs
        .iter()
        .take(query.n.into())
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
    if results.is_empty() {
        return Ok(Response::builder(StatusCode::NotFound)
            .body(NOT_FOUND)
            .build());
    }
    if query.r != 0 {
        let mut rng = state.rng.lock().unwrap();
        results.shuffle(&mut *rng);
    }
    Ok(results.join("\n").into())
}

async fn exists(req: Request<State>) -> tide::Result {
    // let host = req.param("host")?;
    // if !validate_host(host) {
    //     return Ok(Response::builder(StatusCode::BadRequest).build());
    // }
    // if is_exists(host).await {
    //     return Ok(EXISTS.into());
    // }
    // Ok(Response::builder(StatusCode::NotFound).body(NOT_FOUND).build())
    Ok(Response::builder(StatusCode::InternalServerError).build())
}

async fn is_exists(host: &str) -> bool {
    unimplemented!()
}

fn validate_host(s: &str) -> bool {
    if s.len() < 3 || s.len() > 255 {
        return false;
    }
    let mut prev_c = '.';
    let mut has_dot = false;
    for c in s.chars() {
        if c == '.' {
            if prev_c == '.' || prev_c == '-' {
                return false;
            }
            has_dot = true;
        } else if c == '-' {
            if prev_c == '.' {
                return false;
            }
        } else if !c.is_ascii_alphanumeric() {
            return false;
        }
        prev_c = c;
    }
    if prev_c == '.' || prev_c == '-' || !has_dot {
        return false;
    }
    true
}

struct Opts {
    dns: Vec<String>,
    addr: String,
    cert_file: Option<String>,
    key_file: Option<String>,
}

fn get_opts() -> Opts {
    let dns = env::var(ENV_DNS)
        .unwrap_or_else(|_| DEFAULT_DNS.into())
        .split(",")
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    return Opts {
        dns,
        addr: env::var(ENV_ADDR).unwrap_or_else(|_| DEFAULT_ADDR.into()),
        cert_file: env::var(ENV_CERT_FILE).ok(),
        key_file: env::var(ENV_KEY_FILE).ok(),
    };
}

#[tokio::main]
async fn main() -> tide::Result<()> {
    let opts = get_opts();
    let mut name_servers = NameServerConfigGroup::new();
    for dns in &opts.dns {
        let (ip, port) = match dns.rsplit_once(':') {
            Some((s1, s2)) => (s1, s2),
            None => continue,
        };
        name_servers.merge(NameServerConfigGroup::from_ips_clear(
            &[ip.parse()?],
            port.parse()?,
            true,
        ));
    }
    let resolver = TokioAsyncResolver::tokio(
        ResolverConfig::from_parts(None, vec![], name_servers),
        ResolverOpts::default(),
    )
    .expect("failed to connect resolver");
    let rng = SmallRng::from_entropy();
    let mut app = tide::with_state(State {
        resolver: Arc::new(resolver),
        rng: Arc::new(Mutex::new(rng)),
    });
    app.with(After(|mut res: Response| async {
        res.append_header("Access-Control-Allow-Origin", "*");
        Ok(res)
    }));
    app.at("/ping").get(|_| async { Ok("OK") });
    app.at("/r/:host").get(resolve);
    app.at("/x/:host").get(exists);
    if opts.cert_file.is_some() && opts.key_file.is_some() {
        app.listen(
            TlsListener::build()
                .addrs(opts.addr)
                .cert(opts.cert_file.unwrap())
                .key(opts.key_file.unwrap()),
        )
        .await?;
    } else {
        app.listen(opts.addr).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::validate_host;

    #[test]
    fn vhost() {
        assert_eq!(validate_host(".o"), false);
        assert_eq!(validate_host("..o"), false);
        assert_eq!(validate_host("-asd.o"), false);
        assert_eq!(validate_host("--asd.o"), false);
        assert_eq!(validate_host("asd."), false);
        assert_eq!(validate_host("asd.o."), false);
        assert_eq!(validate_host("asd-"), false);
        assert_eq!(validate_host("asd.o-"), false);
        assert_eq!(validate_host("asd"), false);
        assert_eq!(validate_host("asd-.o"), false);
        assert_eq!(validate_host("asd.-o"), false);
        assert_eq!(validate_host("asd..o"), false);
        assert_eq!(validate_host("asd--o"), false);
        assert_eq!(validate_host("-.o"), false);
        assert_eq!(validate_host(".-o"), false);
        assert_eq!(validate_host("asd--asd.o"), true);
    }
}
