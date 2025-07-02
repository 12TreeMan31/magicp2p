use crate::network::{Event, NetworkState};
use futures::prelude::*;
use libp2p::{Multiaddr, identity::Keypair, kad, noise, swarm::SwarmEvent, tcp, yamux};
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
        _ => (),
    }
}
// /ip6/2604:1380:1000:6000::1/tcp/4001/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    /*let _ = tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .try_init();*/
    console_subscriber::init();
    let id_keys: Keypair = Keypair::generate_ed25519();
    let behavor = NetworkState::new(&id_keys.public());

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(id_keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_dns()?
        .with_behaviour(|_| behavor)?
        .build();

    // Tell the swarm to listen on all interfaces and a random, OS-assigned
    // port.
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
    println!("Welcome!");

    // let poll = FuturesUnordered::<Box<dyn Future>>::new();

    // Dial the peer identified by the multi-address given as the second
    // command-line argument, if any.
    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
        println!("Dialed {addr}")
    }

    // Get tokio monitor working!!
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),
            SwarmEvent::Behaviour(event) => handle_network(event),
            _ => {}
        }
    }
}
