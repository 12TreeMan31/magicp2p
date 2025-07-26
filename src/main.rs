use crate::behaviour::{Behaviour, Event, RegisterType};
use futures::prelude::*;
use libp2p::{
    Multiaddr, PeerId, SwarmBuilder, gossipsub, identify,
    identity::Keypair,
    kad,
    kad::PeerInfo,
    noise,
    swarm::{Swarm, SwarmEvent},
    tcp, yamux,
};
use std::str;
use std::time::{Duration, Instant};
use std::{error::Error, num::NonZeroUsize};
use tokio::{self, net::UdpSocket, select, sync::mpsc, time};
use tracing_subscriber::prelude::*;
// use tracing_subscriber::EnvFilter;

mod behaviour;
mod dht;

// We should look into decision trees
fn network_handle(event: SwarmEvent<Event>, tx: &mpsc::UnboundedSender<RegisterType>) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {}", address),
        SwarmEvent::Behaviour(event) => match event {
            Event::Gossipsub(e) => match e {
                gossipsub::Event::Message {
                    propagation_source,
                    message_id,
                    message,
                } => {
                    println!(
                        "[{}:{}]: {:?}",
                        propagation_source,
                        message_id,
                        str::from_utf8(&message.data).unwrap()
                    );
                }
                _ => {}
            },
            Event::Kademlia(e) => match e {
                kad::Event::OutboundQueryProgressed { result, .. } => match result {
                    kad::QueryResult::GetProviders(x) => {
                        // Key will tell us what channel this is for
                        if let Ok(kad::GetProvidersOk::FoundProviders { key: _, providers }) = x {
                            let _ = tx.send(RegisterType::Provider(providers));
                        }
                    }
                    kad::QueryResult::StartProviding(x) => {
                        println!("{:#?}", x);
                    }
                    _ => {}
                },
                _ => {}
            },
            Event::Identify(e) => match e {
                identify::Event::Received { peer_id, info, .. } => {
                    if info.protocols.contains(&kad::PROTOCOL_NAME) {
                        let _ = tx.send(RegisterType::Identify(peer_id, vec![info.observed_addr]));
                    }
                }
                _ => {}
            },
        },
        _ => println!("{:?}", event),
    }
}

fn command_handle(buf: &[u8]) {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    /*let console_layer = console_subscriber::spawn();
    tracing_subscriber::registry()
        .with(console_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();*/

    // See libp2p
    let id_keys = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(id_keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_dns()?
        .with_behaviour(|key| Behaviour::new(&key))?
        .build();
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Do a manual bootstrap because we need to confirm our connection to the DHT????
    swarm.behaviour_mut().kademlia.bootstrap()?;

    // For user messeges
    let local_socket = UdpSocket::bind("[::1]:0").await?;
    let mut user_buffer = vec![0u8];

    // Data that is required to mutate swarm from a behaviour
    let (tx, mut rx) = mpsc::unbounded_channel::<RegisterType>();

    // Remove later just here for testing
    let mut discover = time::interval(Duration::from_secs(10));
    let topic = gossipsub::IdentTopic::new("magicp2p-test");
    let topic_key = topic.hash().into_string().into_bytes();
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    swarm
        .behaviour_mut()
        .kademlia
        .start_providing(topic_key.clone().into())?;

    discover.tick().await;

    loop {
        // TODO: Use a threadpool or something as this might become too slow later on
        select! {
            event = swarm.select_next_some() => network_handle(event, &tx),
            Some(peer_info) = rx.recv() => register(peer_info, &mut swarm),
            bytes = local_socket.recv(&mut user_buffer) => command_handle(&user_buffer[..bytes?]),
            _ = discover.tick() => {
                swarm.behaviour_mut().kademlia.get_providers(topic_key.clone().into());
                let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), b"Hello World");
            }
            // TODO: Add a stream of intervals for discovery
        }
    }
}

fn register(info: RegisterType, swarm: &mut Swarm<Behaviour>) {
    match info {
        RegisterType::Identify(peer_id, addrs) => {
            // println!("Found: {:?}", peer_id);
            for addr in addrs {
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, addr.clone());
                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                let _ = swarm.dial(addr);
            }
        }
        RegisterType::Provider(peer_ids) => {
            for peer_id in peer_ids {
                println!("{:?}", peer_id);
                if !swarm.is_connected(&peer_id) {
                    let _ = swarm.dial(peer_id);
                }
            }
        }
        _ => {}
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/*let _ = tracing_subscriber::fmt()
.with_env_filter(EnvFilter::from_default_env())
.try_init();*/
// console_subscriber::init();
/*let id_keys: Keypair = Keypair::generate_ed25519();

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

    let (tx, mut rx) = mpsc::unbounded_channel::<RegisterType>();

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

    let topic_key = topic.hash().into_string().into_bytes();
    swarm
        .behaviour_mut()
        .kademlia
        .start_providing(topic_key.clone().into())?;

    let user = UdpSocket::bind("127.0.0.1:0").await?;
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
                    kad_register(x, &mut swarm);
                }
            }
            _ = user.recv(&mut buffer) => {
                        swarm
                            .behaviour_mut()
                            .kademlia
                            .get_providers(topic_key.clone().into());
                let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), "hello World".as_bytes());
            }
        }
    }
}

fn kad_register(data: RegisterType, swarm: &mut Swarm<Behaviour>) {
    println!("{:?}", data);
    match data {
        RegisterType::Provider(x) => {
            if let Some(x) = x {
                for peer_id in x {
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    // swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
                }
            }
        }
        RegisterType::Discover(x) => {
            for peer in x {
                // swarm.dial()
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .add_explicit_peer(&peer.peer_id);
            }
        }
        /*RegisterType::Discover(x) => {
            for peer in x {
                for addr in peer.addrs {
                    swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer.peer_id, addr);
                }
            }
        }*/
        _ => {}
    }
}*/
