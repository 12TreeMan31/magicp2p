use crate::network::{Event, NetworkState};
use futures::prelude::*;
use libp2p::{Multiaddr, identity::Keypair, noise, quic, swarm::SwarmEvent, yamux};
use std::{error::Error, time::Duration};
use tracing_subscriber::EnvFilter;

mod network;

fn handle_network(event: Event) {
    println!("{event:?}");

    match event {
        Event::Identify(e) => match e {
            _ => (),
        },
        Event::Ping(_) => (),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let id_keys: Keypair = Keypair::generate_ed25519();
    let behavor = NetworkState::new(&id_keys);

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_quic()
        .with_behaviour(|_| behavor)?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX)))
        .build();

    // Tell the swarm to listen on all interfaces and a random, OS-assigned
    // port.
    swarm.listen_on("/ip6/::/udp/0/quic-v1".parse()?)?;

    // let poll = FuturesUnordered::<Box<dyn Future>>::new();

    // Dial the peer identified by the multi-address given as the second
    // command-line argument, if any.
    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
        println!("Dialed {addr}")
    }

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),
            SwarmEvent::Behaviour(event) => handle_network(event),
            _ => {}
        }
    }
}

/*use futures::prelude::*;
use libp2p::{Multiaddr, PeerId, identity, noise, ping, swarm::SwarmEvent, tcp, yamux};
use std::{error::Error, time::Duration};
use text_io::read;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());

    let behavor = ping::Behaviour::new(ping::Config::new().with_interval(Duration::new(1, 0)));
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| behavor)?
        .build();

    // Tell the swarm to listen on all interfaces and a random, OS-assigned
    // port.
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Dial the peer identified by the multi-address given as the second
    // command-line argument, if any.
    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
        println!("Dialed {addr}")
    }

    loop {
        // println!("{:?}", swarm.network_info());
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),
            SwarmEvent::Behaviour(event) => println!("{event:?}"),
            _ => println!("Nothing!"),
        }
    }
}
*/
