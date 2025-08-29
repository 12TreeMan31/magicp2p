// https://github.com/ipfs/kubo/blob/master/docs/config.md#addresses

use futures::prelude::*;
use libp2p::{
    SwarmBuilder,
    gossipsub::{self, MessageAuthenticity},
    mdns, noise,
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux,
};
use libp2p_identity::Keypair;
use std::error::Error;
use tokio::{self, io, io::AsyncBufReadExt, select};

#[derive(NetworkBehaviour)]
struct Behaviour {
    mdns: mdns::tokio::Behaviour,
    gossipsub: gossipsub::Behaviour,
}

impl Behaviour {
    /// Creates a new NetworkBehaviour and also returns the bootstrap QuerryId
    pub fn new(keys: &Keypair) -> Self {
        let peer_id = keys.public().to_peer_id();

        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id).unwrap();

        // Set a custom gossipsub configuration
        let gossipsub_cfg = gossipsub::ConfigBuilder::default().build().unwrap();
        let gossipsub =
            gossipsub::Behaviour::new(MessageAuthenticity::Signed(keys.clone()), gossipsub_cfg)
                .unwrap();

        Self { mdns, gossipsub }
    }
}

fn network_event(swarm: &mut Swarm<Behaviour>, event: SwarmEvent<BehaviourEvent>) {
    match event {
        SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (id, addr) in list {
                println!("New Peer {addr}");
                swarm.behaviour_mut().gossipsub.add_explicit_peer(&id);
            }
        }
        SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
            for (id, addr) in list {
                println!("Remove Peer {addr}");
                swarm.behaviour_mut().gossipsub.remove_blacklisted_peer(&id);
            }
        }
        SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source: peer_id,
            message,
            ..
        })) => {
            println!("<{peer_id}>: {}", String::from_utf8_lossy(&message.data),)
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let keys = Keypair::generate_ed25519();
    println!("Local PeerID {}", keys.public().to_peer_id());

    let mut swarm = SwarmBuilder::with_existing_identity(keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_dns()?
        .with_behaviour(|keys| Behaviour::new(keys))?
        .build();
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    let topic = gossipsub::IdentTopic::new("hi-dave");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    println!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => network_event(&mut swarm, event)
        }
    }
}
