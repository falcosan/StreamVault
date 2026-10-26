use super::{android, between, tizen};
use crate::deploy::{Session, ask, say};
use futures::StreamExt;
use futures::future::join_all;
use futures::stream;
use if_addrs::IfAddr;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

const ROKU: u16 = 8060;
const WEBOS: [u16; 2] = [3000, 3001];
const VIDAA: u16 = 36669;
const NONE_FOUND: &str = "No supported TV found. Turn on the TV and connect it to the same network as this computer; on Android TV, Google TV or Fire TV enable debugging in Developer options first.";
const SCAN_PREFIX: u32 = 22;
const CONNECT: Duration = Duration::from_millis(400);
const ANSWER: Duration = Duration::from_secs(3);
const CONNECTIONS: usize = 200;

pub enum Target {
    Tizen(Ipv4Addr),
    Android(String),
}

pub struct Tv {
    pub target: Target,
    pub address: String,
    name: String,
}

enum Probe {
    Supported(Tv),
    Unsupported(String),
}

pub async fn choose(session: &Session) -> Result<Tv, String> {
    let wanted = std::env::var("TV").ok();
    if wanted.is_none() {
        say("Looking for TVs…");
    }
    let mut found = Vec::new();
    for probe in probes(session, wanted.as_deref()).await {
        match probe {
            Probe::Supported(tv) => found.push(tv),
            Probe::Unsupported(reason) => say(reason),
        }
    }
    if let Some(address) = wanted {
        return found
            .into_iter()
            .find(|tv| tv.address == address)
            .ok_or(format!("No supported TV found at {address}."));
    }
    match found.len() {
        0 => Err(NONE_FOUND.to_owned()),
        1 => {
            let tv = found.remove(0);
            say(format!("Found {} · {}", tv.name, tv.address));
            Ok(tv)
        }
        _ => pick(found),
    }
}

fn pick(mut tvs: Vec<Tv>) -> Result<Tv, String> {
    for (number, tv) in tvs.iter().enumerate() {
        say(format!("{}) {} · {}", number + 1, tv.name, tv.address));
    }
    loop {
        let answer =
            ask("TV number:").ok_or("Several TVs found; run it in a terminal to choose one.")?;
        if let Some(number) = answer
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=tvs.len()).contains(n))
        {
            return Ok(tvs.swap_remove(number - 1));
        }
    }
}

async fn probes(session: &Session, wanted: Option<&str>) -> Vec<Probe> {
    let attached = android::attached()
        .await
        .into_iter()
        .map(|(serial, model)| {
            Probe::Supported(Tv {
                target: Target::Android(serial.clone()),
                address: serial,
                name: model,
            })
        });
    let ports = [
        tizen::API,
        android::ADB,
        android::REMOTE,
        ROKU,
        WEBOS[0],
        WEBOS[1],
        VIDAA,
    ];
    let open = open_ports(&hosts(wanted), &ports).await;
    let probes = join_all(
        open.iter()
            .map(|(ip, ports)| probe(&session.http, *ip, ports)),
    )
    .await;
    attached.chain(probes.into_iter().flatten()).collect()
}

async fn probe(http: &reqwest::Client, ip: Ipv4Addr, ports: &[u16]) -> Option<Probe> {
    let open = |port| ports.contains(&port);
    let address = ip.to_string();
    if open(tizen::API)
        && let Some(device) = tizen::device(http, ip).await
    {
        let field = |key: &str| device[key].as_str().unwrap_or_default().to_owned();
        let name = format!("Samsung {} ({})", field("name"), field("modelName"));
        return Some(Probe::Supported(Tv {
            target: Target::Tizen(ip),
            address,
            name,
        }));
    }
    if open(android::ADB) || open(android::REMOTE) {
        let name = "Android TV / Google TV / Fire TV".to_owned();
        return Some(Probe::Supported(Tv {
            target: Target::Android(address.clone()),
            address,
            name,
        }));
    }
    if open(ROKU)
        && let Some(info) = get(http, ip, ROKU, "/query/device-info").await
    {
        let name = between(&info, "<user-device-name>", "</user-device-name>").unwrap_or_default();
        return Some(Probe::Unsupported(format!(
            "Skipping Roku {name} ({ip}): Roku runs only BrightScript channels, it has no web engine for StreamVault."
        )));
    }
    if WEBOS.into_iter().all(open) {
        return Some(Probe::Unsupported(format!(
            "Skipping LG webOS TV ({ip}): webOS blocks module scripts and cross-site requests in installed apps."
        )));
    }
    open(VIDAA).then(|| {
        Probe::Unsupported(format!(
            "Skipping Hisense VIDAA TV ({ip}): VIDAA does not allow installing apps outside its store."
        ))
    })
}

fn hosts(wanted: Option<&str>) -> Vec<Ipv4Addr> {
    if let Some(address) = wanted {
        return address.parse().into_iter().collect();
    }
    let mut hosts: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.addr {
            IfAddr::V4(address) => Some(address),
            IfAddr::V6(_) => None,
        })
        .filter(|address| !address.ip.is_loopback() && !address.ip.is_link_local())
        .flat_map(|address| subnet(address.ip, address.netmask))
        .collect();
    hosts.sort_unstable();
    hosts.dedup();
    hosts
}

fn subnet(ip: Ipv4Addr, netmask: Ipv4Addr) -> impl Iterator<Item = Ipv4Addr> {
    let mask = u32::from(netmask).max(u32::MAX << (32 - SCAN_PREFIX));
    let network = u32::from(ip) & mask;
    (network + 1..network | !mask).map(Ipv4Addr::from)
}

async fn open_ports(hosts: &[Ipv4Addr], ports: &[u16]) -> BTreeMap<Ipv4Addr, Vec<u16>> {
    let targets = hosts
        .iter()
        .flat_map(|host| ports.iter().map(|port| SocketAddr::from((*host, *port))));
    let open: Vec<SocketAddr> = stream::iter(targets)
        .map(|target| async move {
            let connected = timeout(CONNECT, TcpStream::connect(target)).await;
            connected
                .is_ok_and(|stream| stream.is_ok())
                .then_some(target)
        })
        .buffer_unordered(CONNECTIONS)
        .filter_map(|target| async move { target })
        .collect()
        .await;
    let mut found = BTreeMap::<Ipv4Addr, Vec<u16>>::new();
    for target in open {
        if let IpAddr::V4(ip) = target.ip() {
            found.entry(ip).or_default().push(target.port());
        }
    }
    found
}

pub async fn get(http: &reqwest::Client, ip: Ipv4Addr, port: u16, path: &str) -> Option<String> {
    let response = http
        .get(format!("http://{ip}:{port}{path}"))
        .timeout(ANSWER)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    response.text().await.ok()
}

pub fn local_ip(remote: Ipv4Addr) -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((remote, 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) => Some(ip),
        IpAddr::V6(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::serve;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn devices_are_told_apart_by_their_open_ports() {
        let (http, ip) = (reqwest::Client::new(), Ipv4Addr::new(192, 0, 2, 1));
        let probed = |ports: &'static [u16]| probe(&http, ip, ports);
        assert!(
            matches!(probed(&[3000, 3001]).await, Some(Probe::Unsupported(reason)) if reason.contains("webOS"))
        );
        assert!(
            matches!(probed(&[VIDAA]).await, Some(Probe::Unsupported(reason)) if reason.contains("VIDAA"))
        );
        assert!(matches!(
            probed(&[android::REMOTE]).await,
            Some(Probe::Supported(Tv { target: Target::Android(address), .. })) if address == "192.0.2.1"
        ));
        assert!(probed(&[3000]).await.is_none());
    }

    #[test]
    fn a_chosen_address_is_the_only_host_scanned() {
        assert_eq!(hosts(Some("192.0.2.7")), [Ipv4Addr::new(192, 0, 2, 7)]);
        assert!(hosts(Some("emulator-5554")).is_empty());
    }

    #[test]
    fn a_subnet_is_scanned_up_to_a_slash_22() {
        let hosts: Vec<_> = subnet(
            Ipv4Addr::new(192, 168, 1, 7),
            Ipv4Addr::new(255, 255, 255, 0),
        )
        .collect();
        assert_eq!(
            (hosts.len(), hosts[0], hosts[253]),
            (
                254,
                Ipv4Addr::new(192, 168, 1, 1),
                Ipv4Addr::new(192, 168, 1, 254)
            )
        );
        let wide: Vec<_> =
            subnet(Ipv4Addr::new(10, 1, 2, 3), Ipv4Addr::new(255, 0, 0, 0)).collect();
        assert_eq!(
            (wide.len(), wide[0], wide[1021]),
            (
                1022,
                Ipv4Addr::new(10, 1, 0, 1),
                Ipv4Addr::new(10, 1, 3, 254)
            )
        );
        assert_eq!(
            subnet(Ipv4Addr::new(10, 8, 0, 2), Ipv4Addr::BROADCAST).count(),
            0
        );
    }

    #[tokio::test]
    async fn only_listening_ports_are_reported_open() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let open = listener.local_addr().unwrap().port();
        let closed = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let found = open_ports(&[Ipv4Addr::LOCALHOST], &[open, closed]).await;
        assert_eq!(found.get(&Ipv4Addr::LOCALHOST), Some(&vec![open]));
    }

    #[tokio::test]
    async fn get_returns_the_body_of_a_successful_answer() {
        let (url, requests) = serve(&[(200, "{\"ok\":1}"), (404, "no")]).await;
        let port = url.rsplit_once(':').unwrap().1.parse().unwrap();
        let http = reqwest::Client::new();
        assert_eq!(
            get(&http, Ipv4Addr::LOCALHOST, port, "/api/v2/")
                .await
                .as_deref(),
            Some("{\"ok\":1}")
        );
        assert_eq!(
            get(&http, Ipv4Addr::LOCALHOST, port, "/api/v2/").await,
            None
        );
        assert!(requests.lock().unwrap()[0].starts_with("GET /api/v2/ HTTP/1.1"));
    }

    #[test]
    fn the_local_address_is_the_one_facing_the_remote() {
        assert_eq!(local_ip(Ipv4Addr::LOCALHOST), Some(Ipv4Addr::LOCALHOST));
    }
}
