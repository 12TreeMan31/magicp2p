// https://github.com/ipfs/kubo/blob/master/docs/config.md#addresses

use clap::Parser;
use futures::prelude::*;
use libp2p::{
    Multiaddr, SwarmBuilder,
    autonat::v2 as autonat,
    gossipsub::{self, MessageAuthenticity},
    identify, mdns, noise, rendezvous,
    swarm::{self, ConnectionId, NetworkBehaviour, Swarm, SwarmEvent, dial_opts::DialOpts},
    tcp, yamux,
};
use libp2p_identity::Keypair;
use std::error::Error;
use tokio::{self, io, io::AsyncBufReadExt, select};

#[derive(NetworkBehaviour)]
struct Behaviour {
    mdns: mdns::tokio::Behaviour,
    gossipsub: gossipsub::Behaviour,
    rendezvous: rendezvous::client::Behaviour,
    identify: identify::Behaviour,
    autonat: autonat::client::Behaviour,
}

impl Behaviour {
    pub fn new(keys: &Keypair) -> Self {
        let peer_id = keys.public().to_peer_id();

        let identify_cfg =
            identify::Config::new_with_signed_peer_record("magic-test/1.0.0".to_string(), keys);
        let identify = identify::Behaviour::new(identify_cfg);

        let rendezvous = rendezvous::client::Behaviour::new(keys.clone());

        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id).unwrap();

        // Set a custom gossipsub configuration
        let gossipsub_cfg = gossipsub::ConfigBuilder::default().build().unwrap();
        let gossipsub =
            gossipsub::Behaviour::new(MessageAuthenticity::Signed(keys.clone()), gossipsub_cfg)
                .unwrap();

        let autonat = autonat::client::Behaviour::default();

        Self {
            mdns,
            gossipsub,
            identify,
            rendezvous,
            autonat,
        }
    }
}

fn dial_remote(
    swarm: &mut Swarm<Behaviour>,
    addr: &Multiaddr,
) -> Result<ConnectionId, swarm::DialError> {
    let dialer = DialOpts::unknown_peer_id().address(addr.clone()).build();
    let id = dialer.connection_id();
    swarm.dial(dialer)?;

    swarm.add_external_address(addr.clone());

    Ok(id)
}

fn network_handle(swarm: &mut Swarm<Behaviour>, event: BehaviourEvent) {
    match event {
        BehaviourEvent::Gossipsub(e) => match e {
            gossipsub::Event::Message {
                propagation_source: peer_id,
                message,
                ..
            } => println!("<{peer_id}>: {}", String::from_utf8_lossy(&message.data)),
            event => println!("Unhandled: {:?}", event),
        },
        BehaviourEvent::Identify(e) => println!("{:?}", e),
        BehaviourEvent::Mdns(e) => match e {
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
        BehaviourEvent::Rendezvous(e) => println!("{:?}", e),
        BehaviourEvent::Autonat(e) => {
            if e.result.is_ok() {
                println!("Autonat: External address confirmed: {}", e.tested_addr);
                return;
            }
            println!(
                "Autonat: Got {} bytes on {} to {} and failed: {:?}",
                e.bytes_sent, e.tested_addr, e.server, e.result
            );
        }
    }
}

fn network(swarm: &mut Swarm<Behaviour>, event: SwarmEvent<BehaviourEvent>, id: &ConnectionId) {
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
    #[arg(short, long)]
    bootnode: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Opt = Opt::parse();
    let bootnode: Multiaddr = args.bootnode.parse()?;

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

    let watched_connection = dial_remote(&mut swarm, &bootnode)?;

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
            event = swarm.select_next_some() => network(&mut swarm, event, &watched_connection)
        }
    }
}
