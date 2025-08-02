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
    "/dns6/bootstrap.libp2p.io/tcp/4001/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dns6/bootstrap.libp2p.io/tcp/4001/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "/dns6/bootstrap.libp2p.io/tcp/4001/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "/dns6/bootstrap.libp2p.io/tcp/4001/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

#[derive(NetworkBehaviour)]
struct Behaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    identify: identify::Behaviour,
    // autonat: autonat::Behaviour,
}

impl Behaviour {
    /// Creates a new NetworkBehaviour and also returns the bootstrap QuerryId
    pub fn new(keys: &Keypair) -> Self {
        let peer_id = keys.public().to_peer_id();

        let kademlia_cfg = kad::Config::new(kad::PROTOCOL_NAME);
        let mut kademlia =
            kad::Behaviour::with_config(peer_id, MemoryStore::new(peer_id), kademlia_cfg);
        // We need the peerid but also need the peerid in the multiaddr because reasons.
        // While it may be overkill, in the real program this will allow us to add custom bootnodes
        // from just the multiaddr
        for addr in IPFS_BOOTNODES {
            let addr: Multiaddr = addr.parse().expect("Valid multiaddr");
            let peer_id = match addr.clone().pop().unwrap() {
                Protocol::P2p(peer_id) => peer_id,
                _ => unimplemented!("Handle this in the real app"),
            };

            kademlia.add_address(&peer_id, addr);
        }
        println!("Mode: {}", kademlia.mode());

        let identify_cfg =
            identify::Config::new(identify::PROTOCOL_NAME.to_string(), keys.public());
        let identify = identify::Behaviour::new(identify_cfg);

        Self { kademlia, identify }
    }
}

enum KadState {
    Bootstrap,
    Providing,
    Listening,
}
struct ProviderState {
    step: KadState,
    id: QueryId,
}

async fn handle_kad(
    kad: &mut kad::Behaviour<MemoryStore>,
    state: &mut ProviderState,
    event: kad::Event,
    key: kad::RecordKey,
) {
    match event {
        kad::Event::ModeChanged { new_mode } => println!("Mode has been updated to {}", new_mode),
        kad::Event::OutboundQueryProgressed { id, result, .. } => {
            if id != state.id {
                return;
            }

            match state.step {
                KadState::Bootstrap => {
                    let ok = match result {
                        QueryResult::Bootstrap(Ok(x)) => x,
                        _ => panic!("Failed to bootstrap {:?}", result),
                    };

                    print!("{}, ", ok.num_remaining);
                    if ok.num_remaining != 0 {
                        return;
                    }

                    println!("Connected to the DHT!");
                    let id = kad.start_providing(key.clone()).unwrap();

                    state.step = KadState::Providing;
                    state.id = id
                }
                KadState::Providing => {
                    let ok = match result {
                        QueryResult::StartProviding(Ok(x)) => x,
                        _ => panic!("Could not provide key: {:?}", result),
                    };
                    println!("Started providing key: {:?}", ok.key);

                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;

                    let id = kad.get_providers(ok.key);

                    state.step = KadState::Listening;
                    state.id = id;
                }
                KadState::Listening => {
                    let ok = match result {
                        QueryResult::GetProviders(Ok(x)) => x,
                        _ => panic!("Couldn't get providers: {:?}", result),
                    };

                    match ok {
                        kad::GetProvidersOk::FoundProviders { providers, .. } => {
                            for provider in providers {
                                println!("{}", provider);
                            }
                        }
                        kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. } => {}
                    }
                }
            }
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
        .with_quic()
        .with_dns()?
        .with_behaviour(|keys| Behaviour::new(keys))?
        .build();
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
    //     swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    swarm.listen_on("/ip6/::/udp/0/quic-v1".parse()?)?;
    //  swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    let bootstrap_querry_id = swarm
        .behaviour_mut()
        .kademlia
        .bootstrap()
        .expect("Could not bootstrap");

    // let mut get_providers_querry_id = None;
    // let mut providing_querry_id = None;

    let key: Vec<u8> = b"hello-dave".to_vec();
    let mut state: ProviderState = ProviderState {
        step: KadState::Bootstrap,
        id: bootstrap_querry_id,
    };

    loop {
        let total_peers = swarm.connected_peers().count();
        if total_peers != 0 {
            println!("Connected Peers: {}", total_peers);
        }
        let event = swarm.select_next_some().await;
        // println!("{:?}", event);
        match event {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {}", address),
            SwarmEvent::Behaviour(BehaviourEvent::Identify(x)) => match x {
                identify::Event::Received { peer_id, info, .. } => {
                    for addr in info.listen_addrs {
                        swarm
                            .behaviour_mut()
                            .kademlia
                            .add_address(&peer_id, addr.clone());
                        if !swarm.is_connected(&peer_id) {
                            if let Err(e) = swarm.dial(addr.clone()) {
                                println!(
                                    "Failed to dial peer {:?} at {:?}: {:?}",
                                    peer_id, addr, e
                                );
                            }
                        }
                    }
                }
                _ => println!("{:?}", x),
            },
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(x)) => {
                handle_kad(
                    &mut swarm.behaviour_mut().kademlia,
                    &mut state,
                    x,
                    key.clone().into(),
                )
                .await;
            }
            _ => {}
        }
    }
}
