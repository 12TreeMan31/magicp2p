//! Basic client for testing EVERYTHING

use clap::Parser;
use futures::StreamExt;
use libp2p::swarm::{
    ConnectionId, NetworkBehaviour, Swarm, SwarmEvent, behaviour::toggle::Toggle,
    dial_opts::DialOpts,
};
use libp2p::{
    Multiaddr, PeerId, SwarmBuilder, identify, identity::Keypair, mdns, noise, ping, relay::client,
    rendezvous, tcp, yamux,
};
use magicp2p::socket;
use std::error::Error;
use tokio::{
    select,
    time::{self, Duration},
};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

const PROGRAM_PROTOCOL: &str = "CONNECTION_TEST";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Opt {
    #[arg(short, long)]
    mdns: bool,
    #[arg(short, long)]
    relay: Option<String>,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    mdns: Toggle<mdns::tokio::Behaviour>,
    identify: identify::Behaviour,
    relay: client::Behaviour,
    ping: ping::Behaviour,
    rendezvous: rendezvous::client::Behaviour,
}

fn event_handle(
    swarm: &mut Swarm<Behaviour>,
    event: SwarmEvent<BehaviourEvent>,
    server_id: &mut Option<PeerId>,
    server_connection: &mut Option<ConnectionId>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {}", address),
        SwarmEvent::ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint,
            num_established,
            concurrent_dial_errors,
            established_in,
        } => {
            if Some(connection_id) == *server_connection {
                *server_connection = None;
                *server_id = Some(peer_id);
            }

            info!(
                "{} Connected to <{}> as a {:?} in {:?}",
                connection_id, peer_id, endpoint, established_in
            );
        }
        SwarmEvent::Behaviour(BehaviourEvent::Mdns(e)) => match e {
            mdns::Event::Discovered(list) => {
                for (peer_id, addr) in list {
                    let dail = DialOpts::peer_id(peer_id)
                        .addresses(vec![addr])
                        .extend_addresses_through_behaviour()
                        .build();
                    let _ = swarm.dial(dail);
                }
            }
            _ => {}
        },
        SwarmEvent::Behaviour(BehaviourEvent::Identify(e)) => match e {
            identify::Event::Received { peer_id, info, .. } => {
                info!("<{}> supports {:#?}", peer_id, info.protocols);
                swarm.add_external_address(info.observed_addr);
            }
            identify::Event::Error { peer_id, error, .. } => {
                error!("Error with <{}> getting peer info: {}", peer_id, error);
            }
            _ => {}
        },
        SwarmEvent::Behaviour(BehaviourEvent::Rendezvous(e)) => match e {
            rendezvous::client::Event::Discovered {
                rendezvous_node,
                registrations,
                cookie,
            } => {
                for reg in registrations {
                    let address = reg.record.addresses().get(0).unwrap();

                    let dail = DialOpts::peer_id(reg.record.peer_id())
                        .addresses([address.clone()].to_vec())
                        .extend_addresses_through_behaviour()
                        .build();
                    info!(
                        "Dialing {} on {:#?}",
                        reg.record.peer_id(),
                        reg.record.addresses()
                    );
                    let _ = swarm.dial(dail);
                }
            }
            _ => warn!(?e),
        },
        SwarmEvent::Behaviour(BehaviourEvent::Ping(_)) => info!("Hi!"),
        SwarmEvent::Behaviour(BehaviourEvent::Relay(e)) => info!(target: "relay", ?e),
        _ => {}
    }
}

fn discover_handle(swarm: &mut Swarm<Behaviour>, server_id: Option<PeerId>) {
    let peer_id = match server_id {
        Some(x) => {
            info!("Scanning <{}>", x);
            x
        }
        None => {
            warn!("No server!");
            return;
        }
    };

    swarm
        .behaviour_mut()
        .rendezvous
        .discover(None, None, None, peer_id);
}

fn regester_handle(swarm: &mut Swarm<Behaviour>, rendezvous_node: &mut Option<PeerId>) {
    let peer_id = match *rendezvous_node {
        Some(x) => x,
        None => return,
    };

    *rendezvous_node = None;

    if let Err(e) = swarm.behaviour_mut().rendezvous.register(
        rendezvous::Namespace::from_static("test"),
        peer_id,
        Some(10000),
    ) {
        error!(?e);
        return;
    };
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args = Opt::parse();

    let keys = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(keys)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|keys, relay| {
            let peer_id = keys.public().to_peer_id();

            let mdns = if args.mdns {
                let mdns_cfg = mdns::Config::default();
                let mdns = mdns::tokio::Behaviour::new(mdns_cfg, peer_id).unwrap();
                Toggle::from(Some(mdns))
            } else {
                Toggle::from(None)
            };

            let identify_cfg =
                identify::Config::new_with_signed_peer_record(PROGRAM_PROTOCOL.to_string(), keys);
            let identify = identify::Behaviour::new(identify_cfg);

            let rendezvous = rendezvous::client::Behaviour::new(keys.clone());
            let ping = ping::Behaviour::default();

            Behaviour {
                mdns,
                identify,
                relay,
                rendezvous,
                ping,
            }
        })?
        .build();
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;

    let mut relay_server: Option<ConnectionId> = None;
    let mut relay_id: Option<PeerId> = None;

    if let Some(addr) = args.relay {
        let addr: Multiaddr = addr.parse()?;
        let request = DialOpts::unknown_peer_id().address(addr.clone()).build();
        relay_server = Some(request.connection_id());
        let _ = swarm.dial(request).unwrap();
        swarm.add_external_address(addr.clone());
    }

    let mut discover = time::interval(Duration::from_secs(5));
    let mut regester = time::interval(Duration::from_secs(5));

    loop {
        select! {
            _ = discover.tick() => discover_handle(&mut swarm, relay_id),
            _ = regester.tick() => regester_handle(&mut swarm, &mut relay_id),
            event = swarm.select_next_some() => event_handle(&mut swarm, event, &mut relay_id, &mut relay_server)
        }
    }
}
