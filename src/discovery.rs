use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
    time::Duration,
};
use tokio::{net::UdpSocket, sync::watch};

const MULTICAST: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 1900);
const SERVER: &str = "Castrivo/0.1 UPnP/1.0";
pub struct Discovery {
    socket: Arc<UdpSocket>,
    location: String,
    targets: Vec<(String, String)>,
}
impl Discovery {
    pub fn new(ip: Ipv4Addr, port: u16, uuid: uuid::Uuid) -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 1900).into())?;
        socket.join_multicast_v4(MULTICAST.ip(), &ip)?;
        socket.set_multicast_if_v4(&ip)?;
        socket.set_multicast_ttl_v4(2)?;
        let udn = format!("uuid:{uuid}");
        let targets = [
            "upnp:rootdevice",
            "urn:schemas-upnp-org:device:MediaRenderer:1",
            "urn:schemas-upnp-org:service:AVTransport:1",
            "urn:schemas-upnp-org:service:RenderingControl:1",
            "urn:schemas-upnp-org:service:ConnectionManager:1",
        ]
        .into_iter()
        .map(|target| (target.to_string(), format!("{udn}::{target}")))
        .chain(std::iter::once((udn.clone(), udn.clone())))
        .collect();
        Ok(Self {
            socket: Arc::new(UdpSocket::from_std(socket.into())?),
            location: format!("http://{ip}:{port}/description.xml"),
            targets,
        })
    }
    async fn advertise(&self, subtype: &str) -> Result<()> {
        for (target, usn) in &self.targets {
            let message = format!(
                "NOTIFY * HTTP/1.1\r\nHOST: {MULTICAST}\r\nNT: {target}\r\nNTS: ssdp:{subtype}\r\nUSN: {usn}\r\nLOCATION: {}\r\nCACHE-CONTROL: max-age=1800\r\nSERVER: {SERVER}\r\n\r\n",
                self.location
            );
            self.socket.send_to(message.as_bytes(), MULTICAST).await?;
        }
        Ok(())
    }
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(900));
        let mut buffer = [0; 4096];
        loop {
            tokio::select! {
                _ = shutdown.changed() => { self.advertise("byebye").await?; return Ok(()); }
                _ = interval.tick() => self.advertise("alive").await?,
                result = self.socket.recv_from(&mut buffer) => {
                    let (length, remote) = result?;
                    if let Some((target, mx)) = search(&String::from_utf8_lossy(&buffer[..length])) {
                        let responses: Vec<_> = self.targets.iter().filter(|(st,_)| target == "ssdp:all" || st.eq_ignore_ascii_case(&target)).map(|(st,usn)| {
                            format!("HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nEXT:\r\nLOCATION: {}\r\nSERVER: {SERVER}\r\nST: {st}\r\nUSN: {usn}\r\n\r\n",self.location)
                        }).collect();
                        if !responses.is_empty() { tracing::info!(%remote, target, "Discovery request"); }
                        let socket = self.socket.clone();
                        tokio::spawn(async move {
                            // A randomized delay bounded by MX prevents synchronized replies.
                            let jitter = u64::from(uuid::Uuid::new_v4().as_bytes()[0]) * u64::from(mx) * 1000 / 256;
                            tokio::time::sleep(Duration::from_millis(jitter)).await;
                            for response in responses { let _ = socket.send_to(response.as_bytes(), remote).await; }
                        });
                    }
                }
            }
        }
    }
}
fn search(message: &str) -> Option<(String, u16)> {
    let mut lines = message.lines();
    if lines.next()? != "M-SEARCH * HTTP/1.1" {
        return None;
    }
    let mut st = None;
    let mut mx = None;
    let mut man = false;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            match name.to_ascii_lowercase().as_str() {
                "st" => st = Some(value.trim().to_owned()),
                "mx" => mx = value.trim().parse::<u16>().ok().map(|v| v.clamp(1, 5)),
                "man" => man = value.trim().eq_ignore_ascii_case("\"ssdp:discover\""),
                _ => {}
            }
        }
    }
    if man { Some((st?, mx?)) } else { None }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_search_requires_valid_headers() {
        assert_eq!(
            search(
                "M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\n\r\n"
            ),
            Some(("ssdp:all".into(), 2))
        );
        assert!(search("M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\n").is_none());
    }
}
