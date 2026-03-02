use crate::cluster::ClusterRegistry;
use crate::config::{ServerConfig, ServerServiceConfig, ServiceType, TransportType};
use crate::config_watcher::{ConfigChange, ServerServiceChange};
use crate::constants::{listen_backoff, UDP_BUFFER_SIZE};
use crate::helper::{retry_notify_with_deadline, write_and_flush};
use crate::multi_map::MultiMap;
use crate::protocol::Hello::{ControlChannelHello, DataChannelHello, NodeRegistrationHello};
use crate::protocol::{
    self, read_auth, read_control_cmd, read_hello, write_msg, Ack, ControlChannelCmd,
    DataChannelCmd, Hello, UdpTraffic, HASH_WIDTH_IN_BYTES,
};
use crate::transport::{SocketOpts, TcpTransport, Transport};
use anyhow::{anyhow, bail, Context, Result};
use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;

use nebula_core::identity::node_id::{ClusterId, NodeId};
use nebula_core::identity::roles::NodeRole;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{self, copy_bidirectional, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time;
use tracing::{debug, error, info, info_span, instrument, warn, Instrument, Span};

type ServiceDigest = protocol::Digest; // SHA256 of a service name
type Nonce = protocol::Digest; // Also called `session_key`

const TCP_POOL_SIZE: usize = 8; // The number of cached connections for TCP services
const UDP_POOL_SIZE: usize = 2; // The number of cached connections for UDP services
const CHAN_SIZE: usize = 2048; // The capacity of various chans
const HANDSHAKE_TIMEOUT: u64 = 5; // Timeout for transport handshake

// The entrypoint of running a server
pub async fn run_server(
    config: ServerConfig,
    shutdown_rx: broadcast::Receiver<bool>,
    update_rx: mpsc::Receiver<ConfigChange>,
    cluster_registry: Arc<RwLock<ClusterRegistry>>,
) -> Result<()> {
    match config.transport.transport_type {
        TransportType::Tcp => {
            let mut server =
                Server::<TcpTransport>::from(config, cluster_registry).await?;
            server.run(shutdown_rx, update_rx).await?;
        }
        TransportType::Tls => {
            crate::helper::feature_neither_compile("native-tls", "rustls")
        }
        TransportType::Noise => {
            crate::helper::feature_not_compile("noise")
        }
        TransportType::Websocket => {
            crate::helper::feature_neither_compile("websocket-native-tls", "websocket-rustls")
        }
    }

    Ok(())
}

// A hash map of ControlChannelHandles, indexed by ServiceDigest or Nonce
// See also MultiMap
type ControlChannelMap<T> = MultiMap<ServiceDigest, Nonce, ControlChannelHandle<T>>;

// Server holds all states of running a server
struct Server<T: Transport> {
    // `[server]` config
    config: Arc<ServerConfig>,

    // `[server.services]` config, indexed by ServiceDigest
    services: Arc<RwLock<HashMap<ServiceDigest, ServerServiceConfig>>>,
    // Collection of control channels
    control_channels: Arc<RwLock<ControlChannelMap<T>>>,
    // Wrapper around the transport layer
    transport: Arc<T>,
    // NEBULA: cluster registry for node management
    cluster_registry: Arc<RwLock<ClusterRegistry>>,
}

// Generate a hash map of services which is indexed by ServiceDigest
fn generate_service_hashmap(
    server_config: &ServerConfig,
) -> HashMap<ServiceDigest, ServerServiceConfig> {
    let mut ret = HashMap::new();
    for u in &server_config.services {
        ret.insert(protocol::digest(u.0.as_bytes()), (*u.1).clone());
    }
    ret
}

impl<T: 'static + Transport> Server<T> {
    // Create a server from `[server]`
    pub async fn from(
        config: ServerConfig,
        cluster_registry: Arc<RwLock<ClusterRegistry>>,
    ) -> Result<Server<T>> {
        let config = Arc::new(config);
        let services = Arc::new(RwLock::new(generate_service_hashmap(&config)));
        let control_channels = Arc::new(RwLock::new(ControlChannelMap::new()));
        let transport = Arc::new(T::new(&config.transport)?);
        Ok(Server {
            config,
            services,
            control_channels,
            transport,
            cluster_registry,
        })
    }

    // The entry point of Server
    pub async fn run(
        &mut self,
        mut shutdown_rx: broadcast::Receiver<bool>,
        mut update_rx: mpsc::Receiver<ConfigChange>,
    ) -> Result<()> {
        // Listen at `server.bind_addr`
        let l = self
            .transport
            .bind(&self.config.bind_addr)
            .await
            .with_context(|| "Failed to listen at `server.bind_addr`")?;
        info!("Listening at {}", self.config.bind_addr);

        // Retry at least every 100ms
        let mut backoff = ExponentialBackoff {
            max_interval: Duration::from_millis(100),
            max_elapsed_time: None,
            ..Default::default()
        };

        // Wait for connections and shutdown signals
        loop {
            tokio::select! {
                // Wait for incoming control and data channels
                ret = self.transport.accept(&l) => {
                    match ret {
                        Err(err) => {
                            // Detects whether it's an IO error
                            if let Some(err) = err.downcast_ref::<io::Error>() {
                                if let Some(d) = backoff.next_backoff() {
                                    error!("Failed to accept: {:#}. Retry in {:?}...", err, d);
                                    time::sleep(d).await;
                                } else {
                                    error!("Too many retries. Aborting...");
                                    break;
                                }
                            }
                        }
                        Ok((conn, addr)) => {
                            backoff.reset();

                            // Do transport handshake with a timeout
                            match time::timeout(Duration::from_secs(HANDSHAKE_TIMEOUT), self.transport.handshake(conn)).await {
                                Ok(conn) => {
                                    match conn.with_context(|| "Failed to do transport handshake") {
                                        Ok(conn) => {
                                            let services = self.services.clone();
                                            let control_channels = self.control_channels.clone();
                                            let server_config = self.config.clone();
                                            let cluster_registry = self.cluster_registry.clone();
                                            tokio::spawn(async move {
                                                if let Err(err) = handle_connection(
                                                    conn,
                                                    services,
                                                    control_channels,
                                                    server_config,
                                                    cluster_registry,
                                                ).await {
                                                    error!("{:#}", err);
                                                }
                                            }.instrument(info_span!("connection", %addr)));
                                        }, Err(e) => {
                                            error!("{:#}", e);
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("Transport handshake timeout: {}", e);
                                }
                            }
                        }
                    }
                },
                // Wait for the shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Shutting down gracefully...");
                    break;
                },
                e = update_rx.recv() => {
                    if let Some(e) = e {
                        self.handle_hot_reload(e).await;
                    }
                }
            }
        }

        info!("Shutdown");

        Ok(())
    }

    async fn handle_hot_reload(&mut self, e: ConfigChange) {
        match e {
            ConfigChange::ServerChange(server_change) => match server_change {
                ServerServiceChange::Add(cfg) => {
                    let hash = protocol::digest(cfg.name.as_bytes());
                    let mut wg = self.services.write().await;
                    let _ = wg.insert(hash, cfg);

                    let mut wg = self.control_channels.write().await;
                    let _ = wg.remove1(&hash);
                }
                ServerServiceChange::Delete(s) => {
                    let hash = protocol::digest(s.as_bytes());
                    let _ = self.services.write().await.remove(&hash);

                    let mut wg = self.control_channels.write().await;
                    let _ = wg.remove1(&hash);
                }
            },
            ignored => warn!("Ignored {:?} since running as a server", ignored),
        }
    }
}

// Handle connections to `server.bind_addr`
async fn handle_connection<T: 'static + Transport>(
    mut conn: T::Stream,
    services: Arc<RwLock<HashMap<ServiceDigest, ServerServiceConfig>>>,
    control_channels: Arc<RwLock<ControlChannelMap<T>>>,
    server_config: Arc<ServerConfig>,
    cluster_registry: Arc<RwLock<ClusterRegistry>>,
) -> Result<()> {
    // Read hello
    let hello = read_hello(&mut conn).await?;
    match hello {
        ControlChannelHello(_, service_digest) => {
            do_control_channel_handshake(
                conn,
                services,
                control_channels,
                service_digest,
                server_config,
            )
            .await?;
        }
        DataChannelHello(_, nonce) => {
            do_data_channel_handshake(conn, control_channels, nonce).await?;
        }
        NodeRegistrationHello(_version, node_id, cluster_id) => {
            do_node_registration::<T>(conn, &cluster_registry, node_id, cluster_id).await?;
        }
    }
    Ok(())
}

// ── NEBULA: Node registration handshake ─────────────────────────────────────

async fn do_node_registration<T: 'static + Transport>(
    mut conn: T::Stream,
    cluster_registry: &Arc<RwLock<ClusterRegistry>>,
    node_id: NodeId,
    cluster_id: ClusterId,
) -> Result<()> {
    info!(
        node = %node_id,
        cluster = %cluster_id,
        "Node registration request"
    );

    // Register the node (or accept dual-tunnel during rotation)
    let assigned_role = {
        let mut registry = cluster_registry.write().await;
        match registry.register_node(&cluster_id, node_id) {
            Ok(assigned_role) => {
                info!(
                    node = %node_id,
                    cluster = %cluster_id,
                    role = %assigned_role,
                    "Node registered successfully"
                );
                write_msg(&mut conn, &Ack::RegistrationAccepted { assigned_role }).await?;
                assigned_role
            }
            Err(crate::cluster::registry::RegistrationError::NodeAlreadyRegistered) => {
                warn!(
                    node = %node_id,
                    cluster = %cluster_id,
                    "Node already registered"
                );
                write_msg(&mut conn, &Ack::NodeAlreadyRegistered).await?;
                return Ok(());
            }
            Err(crate::cluster::registry::RegistrationError::ClusterFull) => {
                warn!(
                    node = %node_id,
                    cluster = %cluster_id,
                    "Cluster is full"
                );
                write_msg(&mut conn, &Ack::ClusterNotFound).await?;
                return Ok(());
            }
            Err(e) => {
                warn!(
                    node = %node_id,
                    cluster = %cluster_id,
                    error = %e,
                    "Registration failed"
                );
                write_msg(&mut conn, &Ack::ClusterNotFound).await?;
                return Ok(());
            }
        }
    };

    // After successful registration, enter a control loop to handle
    // heartbeats and rotation commands from the node.
    run_node_control_loop::<T>(conn, cluster_registry, node_id, cluster_id, assigned_role).await
}

/// Long-lived control loop for a registered node.
///
/// Reads `ControlChannelCmd` messages from the node and handles:
/// - `NodeHeartBeat`: updates heartbeat timestamp
/// - `RotationPrepare`: begins a rotation (only from current master)
/// - `RotationReady`: promotes the new master and starts routing to it
/// - `RotationComplete`: finalizes the rotation and cleans up
/// - `HeartBeat`: simple keepalive (no-op acknowledgement)
///
/// The loop exits when the connection is closed or an unrecoverable error occurs.
async fn run_node_control_loop<T: 'static + Transport>(
    mut conn: T::Stream,
    cluster_registry: &Arc<RwLock<ClusterRegistry>>,
    node_id: NodeId,
    cluster_id: ClusterId,
    _assigned_role: NodeRole,
) -> Result<()> {
    loop {
        let cmd = match read_control_cmd(&mut conn).await {
            Ok(cmd) => cmd,
            Err(e) => {
                // Connection closed or read error — node disconnected
                debug!(
                    node = %node_id,
                    cluster = %cluster_id,
                    "Node control channel closed: {:#}",
                    e
                );
                break;
            }
        };

        match cmd {
            ControlChannelCmd::HeartBeat => {
                debug!(node = %node_id, "Heartbeat received");
            }

            ControlChannelCmd::NodeHeartBeat(payload) => {
                let mut registry = cluster_registry.write().await;
                registry.update_heartbeat(&cluster_id, &payload.node_id);
                debug!(
                    node = %payload.node_id,
                    battery = payload.battery_level,
                    "Node heartbeat processed"
                );
            }

            ControlChannelCmd::RotationPrepare { new_master } => {
                info!(
                    node = %node_id,
                    cluster = %cluster_id,
                    new_master = %new_master,
                    "Rotation prepare received"
                );

                let mut registry = cluster_registry.write().await;
                match registry.begin_rotation(&cluster_id, &new_master) {
                    Ok(()) => {
                        info!(
                            cluster = %cluster_id,
                            new_master = %new_master,
                            "Rotation started"
                        );
                        write_msg(&mut conn, &Ack::RotationInProgress).await?;
                    }
                    Err(e) => {
                        warn!(
                            cluster = %cluster_id,
                            error = %e,
                            "Failed to begin rotation"
                        );
                        write_msg(&mut conn, &Ack::ClusterNotFound).await?;
                    }
                }
            }

            ControlChannelCmd::RotationReady { new_master } => {
                info!(
                    node = %node_id,
                    cluster = %cluster_id,
                    new_master = %new_master,
                    "Rotation ready received — promoting new master"
                );

                let mut registry = cluster_registry.write().await;
                match registry.promote_node(&cluster_id, &new_master) {
                    Ok(()) => {
                        info!(
                            cluster = %cluster_id,
                            new_master = %new_master,
                            "New master promoted, routing new requests to it"
                        );
                        write_msg(&mut conn, &Ack::RotationInProgress).await?;
                    }
                    Err(e) => {
                        warn!(
                            cluster = %cluster_id,
                            error = %e,
                            "Failed to promote node"
                        );
                        write_msg(&mut conn, &Ack::ClusterNotFound).await?;
                    }
                }
            }

            ControlChannelCmd::RotationComplete {
                old_master,
                new_master,
            } => {
                info!(
                    node = %node_id,
                    cluster = %cluster_id,
                    old_master = %old_master,
                    new_master = %new_master,
                    "Rotation complete received — finalizing"
                );

                let mut registry = cluster_registry.write().await;
                match registry.complete_rotation(&cluster_id) {
                    Ok(()) => {
                        info!(
                            cluster = %cluster_id,
                            "Rotation finalized successfully"
                        );
                        write_msg(&mut conn, &Ack::Ok).await?;
                    }
                    Err(e) => {
                        warn!(
                            cluster = %cluster_id,
                            error = %e,
                            "Failed to complete rotation"
                        );
                        write_msg(&mut conn, &Ack::ClusterNotFound).await?;
                    }
                }
            }

            other => {
                debug!(
                    node = %node_id,
                    "Ignoring unhandled control command: {:?}",
                    other
                );
            }
        }
    }

    Ok(())
}

// ── Rathole-compatible control channel handshake ────────────────────────────

async fn do_control_channel_handshake<T: 'static + Transport>(
    mut conn: T::Stream,
    services: Arc<RwLock<HashMap<ServiceDigest, ServerServiceConfig>>>,
    control_channels: Arc<RwLock<ControlChannelMap<T>>>,
    service_digest: ServiceDigest,
    server_config: Arc<ServerConfig>,
) -> Result<()> {
    info!("Try to handshake a control channel");

    T::hint(&conn, SocketOpts::for_control_channel());

    // Generate a nonce
    let mut nonce = vec![0u8; HASH_WIDTH_IN_BYTES];
    rand::thread_rng().fill_bytes(&mut nonce);

    // Send hello back with the nonce
    let hello_send = Hello::ControlChannelHello(
        protocol::CURRENT_PROTO_VERSION,
        nonce.clone().try_into().unwrap(),
    );
    write_msg(&mut conn, &hello_send).await?;

    // Lookup the service
    let service_config = match services.read().await.get(&service_digest) {
        Some(v) => v,
        None => {
            write_msg(&mut conn, &Ack::ServiceNotExist).await?;
            bail!("No such a service {}", hex::encode(service_digest));
        }
    }
    .to_owned();

    let service_name = &service_config.name;

    // Calculate the checksum
    let mut concat = Vec::from(service_config.token.as_ref().unwrap().as_bytes());
    concat.append(&mut nonce);

    // Read auth
    let protocol::Auth(d) = read_auth(&mut conn).await?;

    // Validate
    let session_key = protocol::digest(&concat);
    if session_key != d {
        write_msg(&mut conn, &Ack::AuthFailed).await?;
        debug!(
            "Expect {}, but got {}",
            hex::encode(session_key),
            hex::encode(d)
        );
        bail!("Service {} failed the authentication", service_name);
    } else {
        let mut h = control_channels.write().await;

        // If there's already a control channel for the service, then drop the old one.
        if h.remove1(&service_digest).is_some() {
            warn!(
                "Dropping previous control channel for service {}",
                service_name
            );
        }

        // Send ack
        write_msg(&mut conn, &Ack::Ok).await?;

        info!(service = %service_config.name, "Control channel established");
        let handle =
            ControlChannelHandle::new(conn, service_config, server_config.heartbeat_interval);

        // Insert the new handle
        let _ = h.insert(service_digest, session_key, handle);
    }

    Ok(())
}

async fn do_data_channel_handshake<T: 'static + Transport>(
    conn: T::Stream,
    control_channels: Arc<RwLock<ControlChannelMap<T>>>,
    nonce: Nonce,
) -> Result<()> {
    debug!("Try to handshake a data channel");

    // Validate
    let control_channels_guard = control_channels.read().await;
    match control_channels_guard.get2(&nonce) {
        Some(handle) => {
            T::hint(&conn, SocketOpts::from_server_cfg(&handle.service));

            // Send the data channel to the corresponding control channel
            handle
                .data_ch_tx
                .send(conn)
                .await
                .with_context(|| "Data channel for a stale control channel")?;
        }
        None => {
            warn!("Data channel has incorrect nonce");
        }
    }
    Ok(())
}

pub struct ControlChannelHandle<T: Transport> {
    // Shutdown the control channel by dropping it
    _shutdown_tx: broadcast::Sender<bool>,
    data_ch_tx: mpsc::Sender<T::Stream>,
    service: ServerServiceConfig,
}

impl<T> ControlChannelHandle<T>
where
    T: 'static + Transport,
{
    #[instrument(name = "handle", skip_all, fields(service = %service.name))]
    fn new(
        conn: T::Stream,
        service: ServerServiceConfig,
        heartbeat_interval: u64,
    ) -> ControlChannelHandle<T> {
        // Create a shutdown channel
        let (shutdown_tx, shutdown_rx) = broadcast::channel::<bool>(1);

        // Store data channels
        let (data_ch_tx, data_ch_rx) = mpsc::channel(CHAN_SIZE * 2);

        // Store data channel creation requests
        let (data_ch_req_tx, data_ch_req_rx) = mpsc::unbounded_channel();

        // Cache some data channels for later use
        let pool_size = match service.service_type {
            ServiceType::Tcp => TCP_POOL_SIZE,
            ServiceType::Udp => UDP_POOL_SIZE,
        };

        for _i in 0..pool_size {
            if let Err(e) = data_ch_req_tx.send(true) {
                error!("Failed to request data channel {}", e);
            };
        }

        let shutdown_rx_clone = shutdown_tx.subscribe();
        let bind_addr = service.bind_addr.clone();
        match service.service_type {
            ServiceType::Tcp => tokio::spawn(
                async move {
                    if let Err(e) = run_tcp_connection_pool::<T>(
                        bind_addr,
                        data_ch_rx,
                        data_ch_req_tx,
                        shutdown_rx_clone,
                    )
                    .await
                    .with_context(|| "Failed to run TCP connection pool")
                    {
                        error!("{:#}", e);
                    }
                }
                .instrument(Span::current()),
            ),
            ServiceType::Udp => tokio::spawn(
                async move {
                    if let Err(e) = run_udp_connection_pool::<T>(
                        bind_addr,
                        data_ch_rx,
                        data_ch_req_tx,
                        shutdown_rx_clone,
                    )
                    .await
                    .with_context(|| "Failed to run UDP connection pool")
                    {
                        error!("{:#}", e);
                    }
                }
                .instrument(Span::current()),
            ),
        };

        // Create the control channel
        let ch = ControlChannel::<T> {
            conn,
            shutdown_rx,
            data_ch_req_rx,
            heartbeat_interval,
        };

        // Run the control channel
        tokio::spawn(
            async move {
                if let Err(err) = ch.run().await {
                    error!("{:#}", err);
                }
            }
            .instrument(Span::current()),
        );

        ControlChannelHandle {
            _shutdown_tx: shutdown_tx,
            data_ch_tx,
            service,
        }
    }
}

// Control channel, using T as the transport layer
struct ControlChannel<T: Transport> {
    conn: T::Stream,
    shutdown_rx: broadcast::Receiver<bool>,
    data_ch_req_rx: mpsc::UnboundedReceiver<bool>,
    heartbeat_interval: u64,
}

impl<T: Transport> ControlChannel<T> {
    async fn write_and_flush(&mut self, data: &[u8]) -> Result<()> {
        write_and_flush(&mut self.conn, data)
            .await
            .with_context(|| "Failed to write control cmds")?;
        Ok(())
    }

    #[instrument(skip_all)]
    async fn run(mut self) -> Result<()> {
        let create_ch_cmd = bincode::serialize(&ControlChannelCmd::CreateDataChannel).unwrap();
        let heartbeat = bincode::serialize(&ControlChannelCmd::HeartBeat).unwrap();

        // Wait for data channel requests and the shutdown signal
        loop {
            tokio::select! {
                val = self.data_ch_req_rx.recv() => {
                    match val {
                        Some(_) => {
                            if let Err(e) = self.write_and_flush(&create_ch_cmd).await {
                                error!("{:#}", e);
                                break;
                            }
                        }
                        None => {
                            break;
                        }
                    }
                },
                _ = time::sleep(Duration::from_secs(self.heartbeat_interval)), if self.heartbeat_interval != 0 => {
                            if let Err(e) = self.write_and_flush(&heartbeat).await {
                                error!("{:#}", e);
                                break;
                            }
                }
                // Wait for the shutdown signal
                _ = self.shutdown_rx.recv() => {
                    break;
                }
            }
        }

        info!("Control channel shutdown");

        Ok(())
    }
}

fn tcp_listen_and_send(
    addr: String,
    data_ch_req_tx: mpsc::UnboundedSender<bool>,
    mut shutdown_rx: broadcast::Receiver<bool>,
) -> mpsc::Receiver<TcpStream> {
    let (tx, rx) = mpsc::channel(CHAN_SIZE);

    tokio::spawn(
        async move {
            let l = retry_notify_with_deadline(
                listen_backoff(),
                || async { Ok(TcpListener::bind(&addr).await?) },
                |e, duration| {
                    error!("{:#}. Retry in {:?}", e, duration);
                },
                &mut shutdown_rx,
            )
            .await
            .with_context(|| "Failed to listen for the service");

            let l: TcpListener = match l {
                Ok(v) => v,
                Err(e) => {
                    error!("{:#}", e);
                    return;
                }
            };

            info!("Listening at {}", &addr);

            // Retry at least every 1s
            let mut backoff = ExponentialBackoff {
                max_interval: Duration::from_secs(1),
                max_elapsed_time: None,
                ..Default::default()
            };

            // Wait for visitors and the shutdown signal
            loop {
                tokio::select! {
                    val = l.accept() => {
                        match val {
                            Err(e) => {
                                error!("{}. Sleep for a while", e);
                                if let Some(d) = backoff.next_backoff() {
                                    time::sleep(d).await;
                                } else {
                                    error!("Too many retries. Aborting...");
                                    break;
                                }
                            }
                            Ok((incoming, addr)) => {
                                if data_ch_req_tx
                                    .send(true)
                                    .with_context(|| "Failed to send data chan create request")
                                    .is_err()
                                {
                                    break;
                                }

                                backoff.reset();

                                debug!("New visitor from {}", addr);

                                let _ = tx.send(incoming).await;
                            }
                        }
                    },
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }

            info!("TCPListener shutdown");
        }
        .instrument(Span::current()),
    );

    rx
}

#[instrument(skip_all)]
async fn run_tcp_connection_pool<T: Transport>(
    bind_addr: String,
    mut data_ch_rx: mpsc::Receiver<T::Stream>,
    data_ch_req_tx: mpsc::UnboundedSender<bool>,
    shutdown_rx: broadcast::Receiver<bool>,
) -> Result<()> {
    let mut visitor_rx = tcp_listen_and_send(bind_addr, data_ch_req_tx.clone(), shutdown_rx);
    let cmd = bincode::serialize(&DataChannelCmd::StartForwardTcp).unwrap();

    'pool: while let Some(mut visitor) = visitor_rx.recv().await {
        loop {
            if let Some(mut ch) = data_ch_rx.recv().await {
                if write_and_flush(&mut ch, &cmd).await.is_ok() {
                    tokio::spawn(async move {
                        let _ = copy_bidirectional(&mut ch, &mut visitor).await;
                    });
                    break;
                } else {
                    // Current data channel is broken. Request for a new one
                    if data_ch_req_tx.send(true).is_err() {
                        break 'pool;
                    }
                }
            } else {
                break 'pool;
            }
        }
    }

    info!("Shutdown");
    Ok(())
}

#[instrument(skip_all)]
async fn run_udp_connection_pool<T: Transport>(
    bind_addr: String,
    mut data_ch_rx: mpsc::Receiver<T::Stream>,
    _data_ch_req_tx: mpsc::UnboundedSender<bool>,
    mut shutdown_rx: broadcast::Receiver<bool>,
) -> Result<()> {
    let l = retry_notify_with_deadline(
        listen_backoff(),
        || async { Ok(UdpSocket::bind(&bind_addr).await?) },
        |e, duration| {
            warn!("{:#}. Retry in {:?}", e, duration);
        },
        &mut shutdown_rx,
    )
    .await
    .with_context(|| "Failed to listen for the service")?;

    info!("Listening at {}", &bind_addr);

    let cmd = bincode::serialize(&DataChannelCmd::StartForwardUdp).unwrap();

    // Receive one data channel
    let mut conn = data_ch_rx
        .recv()
        .await
        .ok_or_else(|| anyhow!("No available data channels"))?;
    write_and_flush(&mut conn, &cmd).await?;

    let mut buf = [0u8; UDP_BUFFER_SIZE];
    loop {
        tokio::select! {
            // Forward inbound traffic to the client
            val = l.recv_from(&mut buf) => {
                let (n, from) = val?;
                UdpTraffic::write_slice(&mut conn, from, &buf[..n]).await?;
            },

            // Forward outbound traffic from the client to the visitor
            hdr_len = conn.read_u8() => {
                let t = UdpTraffic::read(&mut conn, hdr_len?).await?;
                l.send_to(&t.data, t.from).await?;
            }

            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }

    debug!("UDP pool dropped");

    Ok(())
}
