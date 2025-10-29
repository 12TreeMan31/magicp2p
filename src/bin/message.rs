use clap::Parser;
use futures::StreamExt;
use libp2p::swarm::{NetworkBehaviour, Swarm, SwarmEvent, dial_opts::DialOpts};
use libp2p::{Multiaddr, PeerId, SwarmBuilder, gossipsub, identity::Keypair, noise, tcp, yamux};
use std::error::Error;
use tokio::{
    io::{self, AsyncBufReadExt},
    select,
    time::{self, Duration},
};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Opt {
    #[arg(short, long)]
    mdns: bool,
    #[arg(short, long)]
    relay: Option<String>,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
}

fn event_handle(swarm: &mut Swarm<Behaviour>, event: SwarmEvent<BehaviourEvent>) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {}", address),
        SwarmEvent::ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint,
            num_established,
            concurrent_dial_errors,
            established_in,
        } => {
            info!(
                "{} Connected to <{}> as a {:?} in {:?}",
                connection_id, peer_id, endpoint, established_in
            );
        }
        SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(e)) => match e {
            gossipsub::Event::Message { message, .. } => {
                println!(
                    "#{} <{}> {}",
                    message.topic,
                    message.source.unwrap_or(PeerId::random()),
                    String::from_utf8_lossy(&message.data)
                );
            }
            gossipsub::Event::Subscribed { peer_id, topic } => {
                println!("#{} <{}> Connected", topic, peer_id);
            }
            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                println!("#{} <{}> Disconnected", topic, peer_id);
            }
            _ => {}
        },
        _ => {}
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args = Opt::parse();

    let keys = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(keys)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|keys| {
            let gossipsub_cfg = gossipsub::Config::default();
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(keys.clone()),
                gossipsub_cfg,
            )
            .unwrap();

            Behaviour { gossipsub }
        })?
        .build();
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    swarm.listen_on("/ip6/::/tcp/0".parse()?)?;

    if let Some(addr) = args.relay {
        let addr: Multiaddr = addr.parse()?;
        let request = DialOpts::unknown_peer_id().address(addr.clone()).build();
        let _ = swarm.dial(request).unwrap();
        swarm.add_external_address(addr.clone());
    }

    let temp_topic = gossipsub::IdentTopic::new("magic");
    swarm.behaviour_mut().gossipsub.subscribe(&temp_topic)?;
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    let mut discover = time::interval(Duration::from_secs(5));
    let mut regester = time::interval(Duration::from_secs(5));

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(temp_topic.clone(), line.as_bytes()) {
                    warn!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => event_handle(&mut swarm, event)
        }
    }
}
