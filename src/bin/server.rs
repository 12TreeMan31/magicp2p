use futures::StreamExt;
use libp2p::{
    Swarm, SwarmBuilder,
    autonat::v2 as autonat,
    gossipsub, identify,
    identity::Keypair,
    noise, relay, rendezvous,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use magicp2p::{self, behaviour::PROGRAM_PROTOCOL};
use std::error::Error;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    rendezvous: rendezvous::server::Behaviour,
    identify: identify::Behaviour,
    autonat: autonat::server::Behaviour,
    relay: relay::Behaviour,
}

impl Behaviour {
    pub fn new(keys: &Keypair) -> Self {
        let identify_cfg =
            identify::Config::new_with_signed_peer_record(PROGRAM_PROTOCOL.to_string(), keys);
        let identify = identify::Behaviour::new(identify_cfg);

        let rendezvous_cfg = rendezvous::server::Config::default();
        let rendezvous = rendezvous::server::Behaviour::new(rendezvous_cfg);

        let autonat = autonat::server::Behaviour::default();

        let relay_cfg = relay::Config::default();
        let relay = relay::Behaviour::new(keys.public().to_peer_id(), relay_cfg);

        let gossipsub_cfg = gossipsub::Config::default();
        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keys.clone()),
            gossipsub_cfg,
        )
        .unwrap();

        Self {
            gossipsub,
            rendezvous,
            identify,
            autonat,
            relay,
        }
    }
}

fn network_handle(event: BehaviourEvent, swarm: &Swarm<Behaviour>) {
    match event {
        BehaviourEvent::Identify(e) => match e {
            identify::Event::Received { peer_id, info, .. } => {
                info!("<{}> supports {:#?}", peer_id, info.protocols);
            }
            identify::Event::Error { peer_id, error, .. } => {
                error!("Error with <{}> getting peer info: {}", peer_id, error);
            }
            _ => {}
        },
        BehaviourEvent::Rendezvous(e) => match e {
            rendezvous::server::Event::PeerRegistered { peer, registration } => info!(
                target: "rendezvous", "Regestered: <{}> to {}: TTL {}, peer connected = {}",
                peer, registration.namespace, registration.ttl, swarm.is_connected(&peer)
            ),
            rendezvous::server::Event::PeerUnregistered { peer, namespace } => {
                info!(target: "rendezvous", "Deregestered {} from {}", peer, namespace)
            }
            rendezvous::server::Event::PeerNotRegistered {
                peer,
                namespace,
                error,
            } => warn!(
                "rendezvous: Failed to regester: {} to {}: {:?}",
                peer, namespace, error
            ),
            rendezvous::server::Event::DiscoverServed {
                enquirer,
                registrations,
            } => {
                info!(target: "rendezvous", "Served <{}> {} registration(s)", enquirer, registrations.len());
            }
            e => warn!(?e),
        },
        BehaviourEvent::Autonat(e) => {
            if e.result.is_ok() {
                info!("Autonat: External address confirmed: {}", e.tested_addr);
            } else {
                info!(
                    "Autonat: Sent {} bytes; test failed for {} on {}",
                    e.data_amount, e.client, e.tested_addr
                );
            }
        }
        BehaviourEvent::Relay(event) => info!("{:?}", event),
        BehaviourEvent::Gossipsub(_) => {}
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let keys = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(keys.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|keys| Behaviour::new(keys))?
        .build();
    swarm.listen_on("/ip6/::/tcp/8011".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/8011".parse()?)?;

    println!("{}", magicp2p::BANNER);

    /* REMOVE
     * I just want to have something that works
     */
    let temp_topic = gossipsub::IdentTopic::new("magic");
    swarm.behaviour_mut().gossipsub.subscribe(&temp_topic)?;

    loop {
        let event = swarm.select_next_some().await;
        match event {
            SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {}", address),
            SwarmEvent::Behaviour(netinfo) => network_handle(netinfo, &swarm),
            SwarmEvent::ConnectionEstablished {
                peer_id,
                established_in: inter,
                ..
            } => info!("Connected to <{}> in {}ms", peer_id, inter.as_millis()),
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                if let Some(err) = cause {
                    error!("Connection closed for <{}>: {}", peer_id, err)
                }
            }
            _ => {}
        }
    }
}
