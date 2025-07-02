use crate::network::{Event, NetworkState};
use futures::prelude::*;
use libp2p::{
    Multiaddr, PeerId, bytes::BufMut, identity::Keypair, kad, noise, swarm::SwarmEvent, tcp, yamux,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    ops::Add,
    str::FromStr,
    time::{Duration, Instant},
};
use tracing_subscriber::EnvFilter;

mod network;

const IPFS_BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

fn handle_network(event: Event) {
    println!("{event:?}");

    match event {
        Event::Identify(e) => match e {
            _ => (),
        },
        Event::Ping(_) => (),
        Event::Kademlia(e) => match e {
            kad::Event::OutboundQueryProgressed { result, .. } => match result {
                kad::QueryResult::GetClosestPeers(Ok(ok)) => println!("err 1"),
                kad::QueryResult::GetClosestPeers(Err(kad::GetClosestPeersError::Timeout {
                    ..
                })) => println!("err 2"),
                kad::QueryResult::PutRecord(Ok(_)) => println!("err 3"),
                kad::QueryResult::PutRecord(Err(err)) => {
                    println!("err 4");
                }
                _ => println!("kad: unknown event"),
            },
            _ => println!("kad: unknown event 2"),
        },
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
    println!("{:?}", id_keys.public().to_peer_id());

    println!("Putting PK record into the DHT");
    let mut pk_record_key = vec![];
    pk_record_key.put_slice("/pk/".as_bytes());
    pk_record_key.put_slice(swarm.local_peer_id().to_bytes().as_slice());

    let mut pk_record = kad::Record::new(pk_record_key, id_keys.public().encode_protobuf());
    pk_record.publisher = Some(*swarm.local_peer_id());
    pk_record.expires = Some(Instant::now().add(Duration::from_secs(60)));

    swarm
        .behaviour_mut()
        .kademlia
        .put_record(pk_record, kad::Quorum::N(NonZeroUsize::new(3).unwrap()))?;

    println!("Welcome!");

    // let poll = FuturesUnordered::<Box<dyn Future>>::new();

    // Dial the peer identified by the multi-address given as the second
    // command-line argument, if any.
    if let Some(addr) = std::env::args().nth(1) {
        // let remote: Multiaddr = addr.parse()?;
        // swarm.dial(remote)?;
        let peer_id: PeerId = addr.parse()?;
        swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
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
