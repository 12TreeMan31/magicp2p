//! A basic messeging client for testing

use clap::Parser;
use futures::StreamExt;
use libp2p::swarm::{NetworkBehaviour, Swarm, SwarmEvent, dial_opts::DialOpts};
use libp2p::{Multiaddr, SwarmBuilder, gossipsub, identity::Keypair, noise, tcp, yamux};
use magicp2p::socket::{self, RequestType};
use std::error::Error;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::{select, task};
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

fn event_handle(
    swarm: &mut Swarm<Behaviour>,
    event: SwarmEvent<BehaviourEvent>,
    message_tx: &mut UnboundedSender<gossipsub::Event>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {}", address),
        SwarmEvent::ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint,
            established_in,
            ..
        } => {
            info!(
                "{} Connected to <{}> as a {:?} in {:?}",
                connection_id, peer_id, endpoint, established_in
            );
        }
        SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(e)) => {
            message_tx.send(e).expect("Channel closed")
        }
        _ => {}
    }
}

fn user_input_handle(swarm: &mut Swarm<Behaviour>, input: socket::RequestEvent) {
    let topic = gossipsub::IdentTopic::new(input.channel());

    match input.kind() {
        RequestType::JOIN => {
            if let Err(e) = swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                error!("{}: {}", e, input.channel());
            }
        }
        RequestType::PART => {
            swarm.behaviour_mut().gossipsub.unsubscribe(&topic);
        }
        RequestType::MESG(text) => {
            if let Err(e) = swarm
                .behaviour_mut()
                .gossipsub
                .publish(topic, text.as_bytes())
            {
                warn!("{e}")
            }
        }
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

    // Channel for keeping track of any requests that are made by the user (we are the receiver)
    let (user_input_tx, mut user_input_rx) = mpsc::unbounded_channel::<socket::RequestEvent>();
    // Chennel for sending any gossipsub events to the user thread (we are the sender)
    let (mut message_tx, message_rx) = mpsc::unbounded_channel::<gossipsub::Event>();

    // Spawns a seperate thread that is just for handling user input
    task::spawn_blocking(async move || {
        socket::user_socket_handler(
            &["/ip6/::/tcp/1234".parse().unwrap()],
            user_input_tx,
            message_rx,
        )
        .await
    });

    if let Some(addr) = args.relay {
        let addr: Multiaddr = addr.parse()?;
        let request = DialOpts::unknown_peer_id().address(addr.clone()).build();
        let _ = swarm.dial(request).unwrap();
        swarm.add_external_address(addr.clone());
    }

    let temp_topic = gossipsub::IdentTopic::new("magic");
    swarm.behaviour_mut().gossipsub.subscribe(&temp_topic)?;

    loop {
        select! {
            Some(input) = user_input_rx.recv() => user_input_handle(&mut swarm, input),
            event = swarm.select_next_some() => event_handle(&mut swarm, event, &mut message_tx)
        }
    }
}
