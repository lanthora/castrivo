mod discovery;
mod logging;
mod player;
mod protocol;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
};
use tokio::sync::{mpsc, watch};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Options {
    #[arg(long, default_value = "Castrivo")]
    name: String,
    /// IPv4 address of the interface to advertise on.
    #[arg(long)]
    ip: Option<Ipv4Addr>,
    #[arg(long, default_value_t = 5200)]
    port: u16,
    /// Stable device UUID override (useful for multiple receivers).
    #[arg(long)]
    uuid: Option<uuid::Uuid>,
    /// Play a media URL or local file at startup for diagnostics.
    #[arg(long)]
    media: Option<String>,
    /// Optional detailed libmpv log; may contain media URLs or credentials.
    #[arg(long)]
    mpv_log: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    logging::init()?;
    let options = Options::parse();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        pid = std::process::id(),
        "Starting receiver"
    );
    let result = run(options);
    match &result {
        Ok(()) => tracing::info!("Receiver shut down normally"),
        Err(error) => {
            tracing::error!(error = %format!("{error:#}"), "Receiver exited with an error")
        }
    }
    result
}

fn run(options: Options) -> Result<()> {
    let ip = match options.ip {
        Some(ip) => ip,
        None => {
            let routed = local_ip_address::local_ip().ok();
            let private = |address: IpAddr| match address {
                IpAddr::V4(ip) if ip.is_private() && !ip.is_loopback() => Some(ip),
                _ => None,
            };
            routed
                .and_then(private)
                .or_else(|| {
                    local_ip_address::list_afinet_netifas()
                        .ok()?
                        .into_iter()
                        .find_map(|(_, address)| private(address))
                })
                .context("No private IPv4 LAN interface found; use --ip")?
        }
    };
    anyhow::ensure!(
        !ip.is_unspecified() && !ip.is_multicast(),
        "--ip must identify a local interface"
    );
    let host = hostname::get().context("Could not read device hostname")?;
    let id = options.uuid.unwrap_or_else(|| {
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_DNS,
            format!("castrivo:{}", host.to_string_lossy()).as_bytes(),
        )
    });
    let runtime = tokio::runtime::Runtime::new()?;
    let (commands, receiver) = mpsc::channel(64);
    let (snapshot, states) = watch::channel(player::Snapshot::default());
    let service = Arc::new(protocol::Receiver::new(
        commands,
        states,
        options.name.clone(),
        id,
    ));
    // Bind before opening a window so startup errors are visible and actionable.
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind((ip, options.port)))
        .context("Could not bind HTTP receiver port")?;
    let port = listener.local_addr()?.port();
    let ssdp = runtime.block_on(async { discovery::Discovery::new(ip, port, id) })?;
    let (shutdown, shutdown_rx) = watch::channel(false);
    let http_shutdown = shutdown.clone();
    let discovery_shutdown = shutdown.clone();
    let discovery_rx = shutdown_rx.clone();
    let network = runtime.spawn(async move {
        let (http, discovery) = tokio::join!(
            async {
                let result = protocol::serve(listener, service, shutdown_rx).await;
                let _ = http_shutdown.send(true);
                result
            },
            async {
                let result = ssdp.run(discovery_rx).await;
                let _ = discovery_shutdown.send(true);
                result
            },
        );
        http?;
        discovery
    });
    let signal_shutdown = shutdown.clone();
    runtime.spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = signal_shutdown.send(true);
        }
    });
    tracing::info!(%ip, port, uuid = %id, "Receiver ready; use the phone app casting menu");
    let native_log = options
        .mpv_log
        .map(|path| path.to_string_lossy().into_owned());
    let result = player::run(
        receiver,
        snapshot,
        shutdown.subscribe(),
        &options.name,
        options.media,
        native_log.as_deref(),
    );
    let _ = shutdown.send(true);
    runtime
        .block_on(network)
        .context("Receiver service task failed")??;
    result
}
