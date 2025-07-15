use crate::behaviour::Event;
use futures::prelude::*;
use libp2p::{
    Multiaddr, PeerId, gossipsub, identity::Keypair, kad, kad::PeerInfo, noise, swarm::SwarmEvent,
    tcp, yamux,
};
use std::str;
use std::time::{Duration, Instant};
use std::{error::Error, num::NonZeroUsize};
use tokio::{net::UdpSocket, select, sync::mpsc};

// use tracing_subscriber::EnvFilter;

mod behaviour;
mod commands;
mod dht;

fn mess(event: gossipsub::Event) {
    match event {
        gossipsub::Event::Message {
            propagation_source,
            message_id,
            message,
        } => {
            println!(
                "[{}:{}]: {:?}",
                propagation_source,
                message_id,
                message.data.to_ascii_lowercase()
            );
        }
        gossipsub::Event::GossipsubNotSupported { .. } => {}
        gossipsub::Event::Subscribed { ref topic, .. } => {
            if *topic == gossipsub::IdentTopic::new("magicp2p-test").hash() {
                println!("{:?}", event);
            }
        }
        _ => println!("{:?}", event),
    }
}

fn handle_network(event: Event, tx: &mpsc::UnboundedSender<PeerInfo>) {
    // println!("[SYSTEM] {event:?}");

    match event {
        Event::Identify(e) => {}
        Event::Kademlia(e) => dht::event_handler(e, &tx),
        Event::Gossipsub(e) => mess(e),
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    /*let _ = tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .try_init();*/
    // console_subscriber::init();
    let id_keys: Keypair = Keypair::generate_ed25519();

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(id_keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_dns()?
        .with_behaviour(|key| behaviour::Behaviour::new(&key))?
        .build();

    // swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
    println!("{:?}", id_keys.public().to_peer_id());

    // println!("Putting PK record into the DHT");
    let pk_record = dht::new_pk_record(&id_keys.public());

    swarm
        .behaviour_mut()
        .kademlia
        .put_record(pk_record, kad::Quorum::N(NonZeroUsize::new(3).unwrap()))?;

    swarm.behaviour_mut().kademlia.bootstrap()?;

    // println!("Welcome!");

    let (tx, mut rx) = mpsc::unbounded_channel::<PeerInfo>();

    // Dial the peer identified by the multi-address given as the second
    // command-line argument, if any.
    if let Some(addr) = std::env::args().nth(1) {
        // let remote: Multiaddr = addr.parse()?;
        // swarm.dial(remote)?;
        let peer_id: PeerId = addr.parse()?;
        swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
        println!("Dialed {addr}")
    }

    // This is weird: Why is IdentTopic a typedef for a generic? Are they trying to hide IdentityHash?
    let topic: gossipsub::IdentTopic = gossipsub::Topic::new("magicp2p-test");
    if !swarm.behaviour_mut().gossipsub.subscribe(&topic)? {
        println!("Could not sub to topic");
    }

    let user = UdpSocket::bind("0.0.0.0:0").await?;
    println!("Dialing {}", user.local_addr()?);
    let mut buffer = vec![0_u8];
    // Get tokio monitor working!!
    loop {
        select! {
            event = swarm.select_next_some() => {
                // println!("{:?}", event);
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {address:?}"),
                    SwarmEvent::Behaviour(event) => handle_network(event, &tx),
                    _ => {}
                }
            }
            peer = rx.recv() => {
                if let Some(x) = peer {
                    for addr in x.addrs {
                        swarm.behaviour_mut().kademlia.add_address(&x.peer_id, addr);
                    }
                }
            }
            _ = user.recv(&mut buffer) => {
                let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), "hello World".as_bytes());
            }
        }
    }
}
