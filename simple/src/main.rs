use futures::prelude::*;
use libp2p::{
    SwarmBuilder, identify,
    kad::{self, QueryId, QueryResult, store::MemoryStore},
    multiaddr::{Multiaddr, Protocol},
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use libp2p_identity::Keypair;
use std::error::Error;

const IPFS_BOOTNODES: [&str; 4] = [
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

#[derive(NetworkBehaviour)]
struct Behaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    identify: identify::Behaviour,
}

impl Behaviour {
    pub fn new(keys: &Keypair) -> (Self, QueryId) {
        let bootstrap_querry_id;
        let peer_id = keys.public().to_peer_id();

        let kademlia = {
            let cfg = kad::Config::new(kad::PROTOCOL_NAME);
            let mut kademlia = kad::Behaviour::with_config(peer_id, MemoryStore::new(peer_id), cfg);

            for addr in IPFS_BOOTNODES {
                let addr: Multiaddr = addr.parse().expect("Valid multiaddr");
                let peer_id = match addr.clone().pop().unwrap() {
                    Protocol::P2p(peer_id) => peer_id,
                    _ => unimplemented!("Handle this in the real app"),
                };

                kademlia.add_address(&peer_id, addr);
            }
            bootstrap_querry_id = kademlia.bootstrap().expect("Could not bootstrap");

            kademlia
        };

        let identify = {
            let cfg = identify::Config::new(identify::PROTOCOL_NAME.to_string(), keys.public());
            let identify = identify::Behaviour::new(cfg);

            identify
        };

        (Self { kademlia, identify }, bootstrap_querry_id)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let keys = Keypair::generate_ed25519();
    println!("Local PeerID {}", keys.public().to_peer_id());

    let mut bootstrap_querry_id = None;

    let mut swarm = SwarmBuilder::with_existing_identity(keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_dns()?
        .with_behaviour(|keys| {
            let (behaviour, id) = Behaviour::new(keys);
            bootstrap_querry_id = Some(id);

            behaviour
        })?
        .build();
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let mut get_providers_querry_id = None;
    let mut providing_querry_id = None;

    let key: Vec<u8> = b"hello-dave".to_vec();

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
                    if Some(id) == bootstrap_querry_id {
                        if let QueryResult::Bootstrap(x) = result {
                            let ok = x.expect("Error bootstrapping");
                            print!("{}, ", ok.num_remaining);
                            if ok.num_remaining == 0 {
                                println!("\nConnected to the DHT!");

                                let id = swarm
                                    .behaviour_mut()
                                    .kademlia
                                    .start_providing(key.clone().into())?;
                                providing_querry_id = Some(id);
                                bootstrap_querry_id = None;
                            }
                        }
                    } else if Some(id) == providing_querry_id {
                        if let QueryResult::StartProviding(x) = result {
                            let ok = x.expect("Could not provide key");
                            println!("Started providing key: {:?}", ok.key);
                        }

                        let id = swarm
                            .behaviour_mut()
                            .kademlia
                            .get_providers(key.clone().into());

                        get_providers_querry_id = Some(id);
                        providing_querry_id = None;
                    } else if Some(id) == get_providers_querry_id {
                        if let QueryResult::GetProviders(x) = result {
                            let ok = x.expect("Could not get providers");
                            match ok {
                                kad::GetProvidersOk::FoundProviders { providers, .. } => {
                                    for provider in providers {
                                        println!("{}", provider);
                                    }
                                }
                                kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. } => {
                                    get_providers_querry_id = None;
                                }
                            }
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
