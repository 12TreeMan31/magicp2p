use clap::Parser;
use futures::prelude::*;
use libp2p::{
    Multiaddr, SwarmBuilder, gossipsub, identify, mdns, noise, rendezvous,
    swarm::{self, ConnectionId, Swarm, SwarmEvent, dial_opts::DialOpts},
    tcp, yamux,
};
use libp2p_identity::Keypair;
use std::error::Error;
use tokio::{self, io, io::AsyncBufReadExt, select};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use magicp2p::behaviour::{MainBehaviour, MainBehaviourEvent};

// Fun little thing
const BANNER: &str = r#"                       _           ____        
 _ __ ___   __ _  __ _(_) ___ _ __|___ \ _ __  
| '_ ` _ \ / _` |/ _` | |/ __| '_ \ __) | '_ \ 
| | | | | | (_| | (_| | | (__| |_) / __/| |_) |
|_| |_| |_|\__,_|\__, |_|\___| .__/_____| .__/ 
                 |___/       |_|        |_|    
"#;

fn dial_remote(
    swarm: &mut Swarm<MainBehaviour>,
    addr: &Multiaddr,
) -> Result<ConnectionId, swarm::DialError> {
    let dialer = DialOpts::unknown_peer_id().address(addr.clone()).build();
    let id = dialer.connection_id();
    swarm.dial(dialer)?;

    swarm.add_external_address(addr.clone());

    Ok(id)
}

fn rendezvous_handle() {}

fn network_handle(swarm: &mut Swarm<MainBehaviour>, event: MainBehaviourEvent) {
    match event {
        MainBehaviourEvent::Gossipsub(e) => match e {
            gossipsub::Event::Message {
                propagation_source: peer_id,
                message,
                ..
            } => println!("<{peer_id}>: {}", String::from_utf8_lossy(&message.data)),
            gossipsub::Event::Subscribed { peer_id, .. } => {
                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                swarm.dial(DialOpts::peer_id(peer_id).build()).unwrap();
            }
            event => println!("Unhandled: {:?}", event),
        },
        MainBehaviourEvent::Identify(e) => match e {
            identify::Event::Received { peer_id, info, .. } => {
                println!("Found peer: {}", peer_id);
                for addr in info.listen_addrs {
                    swarm.add_external_address(addr);
                }
            }
            identify::Event::Error { peer_id, error, .. } => {
                println!("Error with {} getting peer info: {}", peer_id, error);
            }
            _ => {}
        },
        MainBehaviourEvent::Mdns(e) => match e {
            mdns::Event::Discovered(list) => {
                for (id, addr) in list {
                    println!("New Peer {addr}");
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&id);
                }
            }
            mdns::Event::Expired(list) => {
                for (id, addr) in list {
                    println!("Remove Peer {addr}");
                    swarm.behaviour_mut().gossipsub.remove_blacklisted_peer(&id);
                }
            }
        },
        MainBehaviourEvent::Rendezvous(e) => println!("{:?}", e),
        MainBehaviourEvent::Autonat(e) => {
            if e.result.is_ok() {
                println!("Autonat: External address confirmed: {}", e.tested_addr);
                return;
            }
            println!(
                "Autonat: Got {} bytes on {} to {} and failed: {:?}",
                e.bytes_sent, e.tested_addr, e.server, e.result
            );
        }
        _ => {}
    }
}

fn network(
    swarm: &mut Swarm<MainBehaviour>,
    event: SwarmEvent<MainBehaviourEvent>,
    id: &ConnectionId,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => println!("Listening on {}", address),
        SwarmEvent::Behaviour(netinfo) => network_handle(swarm, netinfo),
        SwarmEvent::ExternalAddrConfirmed { address } => {
            println!("External address confirmed: {address}");
        }
        SwarmEvent::ConnectionEstablished {
            peer_id,
            connection_id,
            established_in,
            ..
        } => {
            if connection_id == *id {
                swarm
                    .behaviour_mut()
                    .rendezvous
                    .register(
                        rendezvous::Namespace::from_static("magic-test"),
                        peer_id,
                        None,
                    )
                    .unwrap();
                println!(
                    "Connected to server in {} seconds",
                    established_in.as_secs()
                );
            }
        }
        event => println!("Unhandled: {:?}", event),
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Opt {
    /// Connects client to the server at the provided multiaddr. Starts in server mode if none is given.
    #[arg(short, long, value_name = "multiaddr")]
    bootnode: Option<String>,
    /// multiaddr that the client will be listening on.
    #[arg(short, long, value_name = "multiaddr", default_value = "/ip6/::1")]
    address: Vec<String>,
    /// Disables mDNS
    #[arg(short, long)]
    mdns: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args: Opt = Opt::parse();

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
        .with_behaviour(|keys| MainBehaviour::new(keys, !args.mdns))?
        .build();

    for addr in args.address {
        swarm.listen_on(format!("{}/tcp/0", addr).parse()?)?;
        // swarm.listen_on(format!("{}/udp/0/quic-v1", addr).parse()?)?;
    }

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    let mut watched_connection = ConnectionId::new_unchecked(0);
    if let Some(bootnode) = args.bootnode {
        let bootnode: Multiaddr = bootnode.parse()?;
        watched_connection = dial_remote(&mut swarm, &bootnode)?;
    } else {
        println!("Starting in server mode!");
    }

    let topic = gossipsub::IdentTopic::new("hi-dave");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    print!("{}", BANNER);

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    println!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => network(&mut swarm, event, &watched_connection)
        }
    }
}
