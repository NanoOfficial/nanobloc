use actix_cors::Cors;
use actix_web::{
    web::{self, JsonConfig},
    App, HttpServer,
};
use futures::{
    channel::mpsc,
    future::{join_all, try_join_all},
    prelude::*,
};
use tokio::time::sleep;

use std::{
    collections::HashMap,
    io,
    net::{SocketAddr, TcpListener},
    time::Duration,
};

use crate::{end::actix::error_handlers, AllowOrigin, ApiAccess, ApiAggregator, ApiBuilder};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WebServerConfig {
    pub listen_address: SocketAddr,
    pub allow_origin: Option<AllowOrigin>,
    pub json_payload_size: Option<usize>,
}

impl WebServerConfig {
    pub fn new(listen_address: SocketAddr) -> Self {
        Self {
            listen_address,
            allow_origin: None,
            json_payload_size: None,
        }
    }

    fn json_config(&self) -> JsonConfig {
        let config = JsonConfig::default();

        if let Some(limit) = self.json_payload_size {
            config.limit(limit)
        } else {
            config
        }
    }

    fn cors_factory(&self) -> Cors {
        self.allow_origin
            .clone()
            .map_or_else(Cors::default, Cors::from)
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ApiManagerConfig {
    pub servers: HashMap<ApiAccess, WebServerConfig>,
    pub api_aggregator: ApiAggregator,
    pub server_restart_retry_timeout: u64,
    pub server_restart_max_retries: u16,
    pub disable_signals: bool,
}

impl ApiManagerConfig {
    pub fn new(
        servers: HashMap<ApiAccess, WebServerConfig>,
        api_aggregator: ApiAggregator,
    ) -> Self {
        Self {
            servers,
            api_aggregator,
            ..Default::default()
        }
    }

    pub fn with_retries(mut self, timeout: u64, max_retries: u16) -> Self {
        self.server_restart_retry_timeout = timeout;
        self.server_restart_max_retries = max_retries;
        self
    }

    pub fn disable_signals(mut self) -> Self {
        self.disable_signals = true;
        self
    }
}

impl Default for ApiManagerConfig {
    fn default() -> Self {
        Self {
            servers: HashMap::new(),
            api_aggregator: ApiAggregator::default(),
            server_restart_retry_timeout: 500,
            server_restart_max_retries: 20,
            disable_signals: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateEndpoints {
    endpoints: Vec<(String, ApiBuilder)>,
}

impl UpdateEndpoints {
    pub fn new(endpoints: Vec<(String, ApiBuilder)>) -> Self {
        Self { endpoints }
    }

    pub fn updated_paths(&self) -> impl Iterator<Item = &str> {
        self.endpoints.iter().map(|(path, _)| path.as_str())
    }

    #[doc(hidden)]
    pub fn into_endpoints(self) -> Vec<(String, ApiBuilder)> {
        self.endpoints
    }
}

async fn with_retries<T>(
    mut action: impl FnMut() -> io::Result<T>,
    description: String,
    attempts: u16,
    timeout: u64,
) -> io::Result<T> {
    let timeout = Duration::from_millis(timeout);

    for attempt in 1..=attempts {
        log::trace!("{} (attempt #{})", description, attempt);
        match action() {
            Ok(value) => return Ok(value),
            Err(e) => {
                log::warn!("{} (attempt #{}) failed: {}", description, attempt, e);
                sleep(timeout).await;
            }
        }
    }

    let msg = format!(
        "Cannot complete {} after {} attempts",
        description, attempts
    );
    Err(io::Error::new(io::ErrorKind::Other, msg))
}

#[derive(Debug)]
struct ServerHandle {
    handle: actix_server::ServerHandle,
}

impl ServerHandle {
    async fn stop(self) {
        self.handle.stop(false).await;
    }
}

#[derive(Debug)]
pub struct ApiManager {
    config: ApiManagerConfig,
    servers: Vec<ServerHandle>,
    endpoints: Vec<(String, ApiBuilder)>,
}

impl ApiManager {
    pub fn new(config: ApiManagerConfig) -> Self {
        Self {
            config,
            servers: Vec::new(),
            endpoints: Vec::new(),
        }
    }

    async fn start_servers(
        &mut self,
        server_finished_tx: mpsc::Sender<io::Result<()>>,
    ) -> io::Result<()> {
        log::trace!("Servers start requested.");

        let disable_signals = self.config.disable_signals;
        let start_servers = self.config.servers.iter().map(|(&access, server_config)| {
            let mut aggregator = self.config.api_aggregator.clone();
            aggregator.extend(self.endpoints.clone());
            let server_config = server_config.clone();
            let action_description = format!(
                "starting {} api on {}",
                access, server_config.listen_address
            );

            with_retries(
                move || {
                    Self::start_server(
                        aggregator.clone(),
                        access,
                        server_config.clone(),
                        disable_signals,
                    )
                },
                action_description,
                self.config.server_restart_max_retries,
                self.config.server_restart_retry_timeout,
            )
        });
        let servers = try_join_all(start_servers).await?;

        self.servers = servers
            .into_iter()
            .zip(&self.config.servers)
            .map(|(server, (&access, server_config))| {
                let listen_addr = server_config.listen_address;
                let mut server_finished = server_finished_tx.clone();
                let handle = server.handle();

                tokio::spawn(async move {
                    let res = server.await;
                    if let Err(ref e) = res {
                        log::error!("{} server on {} failed: {}", access, listen_addr, e);
                    } else if !server_finished.is_closed() {
                        log::info!(
                            "{} server on {} terminated in response to a signal",
                            access,
                            listen_addr
                        );
                    }

                    server_finished.send(res).await.ok();
                });

                ServerHandle { handle }
            })
            .collect();

        Ok(())
    }

    async fn stop_servers(&mut self) {
        log::trace!("Servers stop requested.");

        join_all(self.servers.drain(..).map(ServerHandle::stop)).await;
    }

    pub async fn run<S>(mut self, endpoints_rx: S) -> io::Result<()>
    where
        S: Stream<Item = UpdateEndpoints> + Unpin,
    {
        let res = self.run_inner(endpoints_rx).await;
        self.stop_servers().await;
        log::info!("HTTP servers shut down");
        res
    }

    async fn run_inner<S>(&mut self, endpoints_rx: S) -> io::Result<()>
    where
        S: Stream<Item = UpdateEndpoints> + Unpin,
    {
        let mut endpoints_rx = endpoints_rx.fuse();
        let mut server_finished_channel = mpsc::channel(self.config.servers.len());

        loop {
            futures::select! {
                res = server_finished_channel.1.next() => {
                    return res.unwrap_or(Ok(()));
                }

                maybe_request = endpoints_rx.next() => {
                    if let Some(request) = maybe_request {
                        log::info!("Server restart requested");
                        server_finished_channel = mpsc::channel(self.config.servers.len());

                        self.stop_servers().await;
                        self.endpoints = request.endpoints;
                        self.start_servers(server_finished_channel.0.clone()).await?;
                    } else {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn start_server(
        aggregator: ApiAggregator,
        access: ApiAccess,
        server_config: WebServerConfig,
        disable_signals: bool,
    ) -> io::Result<actix_server::Server> {
        let listen_address = server_config.listen_address;
        log::info!("Starting {} web api on {}", access, listen_address);

        let listener = TcpListener::bind(listen_address)?;
        let mut server_builder = HttpServer::new(move || {
            App::new()
                .app_data(server_config.json_config())
                .wrap(server_config.cors_factory())
                .wrap(error_handlers())
                .service(aggregator.extend_backend(access, web::scope("api")))
        })
        .listen(listener)?;

        if disable_signals {
            server_builder = server_builder.disable_signals();
        }

        Ok(server_builder.run())
    }
}
