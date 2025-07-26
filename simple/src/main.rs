use futures::prelude::*;
use libp2p::{
    Multiaddr, SwarmBuilder,
    gossipsub::Sha256Topic,
    identify,
    kad::{self, store::MemoryStore},
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use libp2p_identity::{Keypair, PeerId};
use std::error::Error;

const IPFS_BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

#[derive(NetworkBehaviour)]
struct Behaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    identify: identify::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let key = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(key.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_dns()?
        .with_behaviour(|keys| {
            let kademlia_cfg = kad::Config::new(kad::PROTOCOL_NAME);
            let peer_id = keys.public().to_peer_id();
            let mut kademlia =
                kad::Behaviour::with_config(peer_id, MemoryStore::new(peer_id), kademlia_cfg);

            let bootaddr: Multiaddr = "/dnsaddr/bootstrap.libp2p.io"
                .parse()
                .expect("Valid multiaddr");

            for peer_id in IPFS_BOOTNODES {
                let peer_id: PeerId = peer_id.parse().expect("Valid PeerId");
                kademlia.add_address(&peer_id, bootaddr.clone());
            }
            let _ = kademlia.bootstrap();

            let identify_cfg =
                identify::Config::new(identify::PROTOCOL_NAME.to_string(), keys.public());
            let identify = identify::Behaviour::new(identify_cfg);

            println!("{:?}", peer_id);

            Behaviour { kademlia, identify }
        })?
        .build();
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let key = Sha256Topic::new("hello-dave").hash();
    println!("{}", key);
    let sub_id = swarm
        .behaviour_mut()
        .kademlia
        .start_providing(key.clone().into_string().into_bytes().into())
        .expect("Could not put key");

    let mut id2 = sub_id;
    loop {
        let event = swarm.select_next_some().await;
        match event {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {}", address),
            SwarmEvent::Behaviour(BehaviourEvent::Identify(x)) => match x {
                identify::Event::Received { peer_id, info, .. } => {
                    for addr in info.listen_addrs {
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    }
                }
                _ => {}
            },
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(x)) => match x {
                kad::Event::OutboundQueryProgressed { result, id, .. } => {
                    if sub_id == id {
                        println!("{:?}", result);
                        id2 = swarm
                            .behaviour_mut()
                            .kademlia
                            .get_providers(key.clone().into_string().into_bytes().into());
                    }
                    if id2 == id {
                        println!("{:?}", result);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
