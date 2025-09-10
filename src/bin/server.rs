use futures::StreamExt;
use libp2p::{
    SwarmBuilder,
    autonat::v2 as autonat,
    identify,
    identity::Keypair,
    noise, relay, rendezvous,
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux,
};
use std::error::Error;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

// Fun little thing
const BANNER: &str = r#"                       _           ____        
 _ __ ___   __ _  __ _(_) ___ _ __|___ \ _ __  
| '_ ` _ \ / _` |/ _` | |/ __| '_ \ __) | '_ \ 
| | | | | | (_| | (_| | | (__| |_) / __/| |_) |
|_| |_| |_|\__,_|\__, |_|\___| .__/_____| .__/ 
                 |___/       |_|        |_|    
"#;

#[derive(NetworkBehaviour)]
struct Behaviour {
    rendezvous: rendezvous::server::Behaviour,
    identify: identify::Behaviour,
    autonat: autonat::server::Behaviour,
    relay: relay::Behaviour,
}

impl Behaviour {
    pub fn new(keys: &Keypair) -> Self {
        let identify_cfg =
            identify::Config::new_with_signed_peer_record("magic-test/1.0.0".to_string(), keys);
        let identify = identify::Behaviour::new(identify_cfg);

        let rendezvous_cfg = rendezvous::server::Config::default();
        let rendezvous = rendezvous::server::Behaviour::new(rendezvous_cfg);

        let autonat = autonat::server::Behaviour::default();

        let relay_cfg = relay::Config::default();
        let relay = relay::Behaviour::new(keys.public().to_peer_id(), relay_cfg);

        Self {
            rendezvous,
            identify,
            autonat,
            relay,
        }
    }
}

fn network_handle(swarm: &mut Swarm<Behaviour>, event: BehaviourEvent) {
    match event {
        BehaviourEvent::Identify(e) => match e {
            identify::Event::Received { peer_id, info, .. } => {
                info!("Found peer {}: supports {:#?}", peer_id, info.protocols);
            }
            identify::Event::Error { peer_id, error, .. } => {
                error!("Error with {} getting peer info: {}", peer_id, error);
            }
            _ => {}
        },
        BehaviourEvent::Rendezvous(e) => match e {
            rendezvous::server::Event::PeerRegistered { peer, registration } => info!(
                "rendezvous: Regestered: {} to {}: TTL {}",
                peer, registration.namespace, registration.ttl
            ),
            rendezvous::server::Event::PeerUnregistered { peer, namespace } => {
                info!("rendezvous: Deregestered {} from {}", peer, namespace)
            }
            rendezvous::server::Event::PeerNotRegistered {
                peer,
                namespace,
                error,
            } => warn!(
                "rendezvous: Failed to regester: {} to {}: {:?}",
                peer, namespace, error
            ),
            _ => {}
        },
        BehaviourEvent::Autonat(e) => {
            if e.result.is_ok() {
                info!("Autonat: External address confirmed: {}", e.tested_addr);
                return;
            }
            info!(
                "Autonat: Sent {} bytes; test failed for {} on {}",
                e.data_amount, e.client, e.tested_addr
            );
        }
        _ => {}
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

    println!("{}", BANNER);

    loop {
        let event = swarm.select_next_some().await;
        match event {
            SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {}", address),
            SwarmEvent::Behaviour(netinfo) => network_handle(&mut swarm, netinfo),
            SwarmEvent::ConnectionEstablished {
                peer_id,
                established_in: inter,
                ..
            } => info!("Connected to {} in {}ms", peer_id, inter.as_millis()),
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                info!("Connection closed for {}: {:?}", peer_id, cause)
            }
            _ => {}
        }
    }
}
