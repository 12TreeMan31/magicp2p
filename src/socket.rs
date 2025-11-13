//! Code needed for server-client messaging.
//! This is going to be really basic just to get things started
//!
//! Operations we need right now:
//! * Joining/leaving channels
//! * Querrying channels
//! * Sending messeges
//!
//! Joining and leaving channels is pretty simple and can be impl with request-response
//! Now for sending a message what we can do is open a stream which a client program with
//! interact with. So once a stream is opened with a stream request you just send a topic,
//! and the message over the stream. Receaving messages would be done the same way.

use futures::StreamExt;
use libp2p::gossipsub::{self, Message, Topic, TopicHash};
use libp2p::swarm::{NetworkBehaviour, Swarm, SwarmEvent};
use libp2p::{
    Multiaddr, StreamProtocol, SwarmBuilder,
    request_response::{self, cbor},
};
use libp2p::{PeerId, noise, tcp, yamux};
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use tokio::select;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{error, info, warn};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RequestType {
    JOIN,
    PART,
    MESG(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestEvent {
    // TODO: Make a multihash
    channel: String,
    kind: RequestType,
}

impl RequestEvent {
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn kind(&self) -> &RequestType {
        &self.kind
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ResponseEvent {
    StatusOk,
    StatusErr,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    pub socket: cbor::Behaviour<RequestEvent, ResponseEvent>,
}

impl Behaviour {
    pub fn new() -> Self {
        let socket = cbor::Behaviour::new(
            [(
                StreamProtocol::new("magic"),
                request_response::ProtocolSupport::Full,
            )],
            request_response::Config::default(),
        );

        Self { socket }
    }
}

impl Deref for Behaviour {
    type Target = cbor::Behaviour<RequestEvent, ResponseEvent>;
    fn deref(&self) -> &Self::Target {
        &self.socket
    }
}

fn init_swarm(interface: &[Multiaddr]) -> Swarm<Behaviour> {
    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .expect("Could not configure transport")
        .with_behaviour(|_| Behaviour::new())
        .expect("Could not init behaviour")
        .build();

    for addr in interface {
        let res = swarm.listen_on(addr.clone());
        if let Err(err) = res {
            error!(target: "socket", "Could not bind on {}", err)
        }
    }

    swarm
}

fn inbound_handle(
    swarm: &mut Swarm<Behaviour>,
    event: SwarmEvent<BehaviourEvent>,
    input: &mut UnboundedSender<RequestEvent>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {}
        SwarmEvent::Behaviour(e) => match e {
            BehaviourEvent::Socket(e) => {}
        },
        _ => {}
    }
}

fn outbound_handle(swarm: &mut Swarm<Behaviour>, msg: gossipsub::Event) {
    match msg {
        gossipsub::Event::Message { message, .. } => {
            let message_text = match str::from_utf8(&message.data) {
                Ok(x) => x,
                Err(e) => {
                    error!("{}", e);
                    return;
                }
            };
            // Sometimes we don't get a peer_id
            match message.source {
                Some(peer_id) => info!("#{} <{}> {}", message.topic, peer_id, message_text),
                None => info!("#{} <Unknown> {}", message.topic, message_text),
            }

            let request = RequestEvent {
                channel: message.topic.into_string(),
                kind: RequestType::MESG(message_text.to_string()),
            };

            // We have to fight with the borrow checker here
            let connected_peers: Vec<PeerId> = swarm
                .connected_peers()
                .map(|peer_id| peer_id.clone())
                .collect();
            // For now broadcast to everyone
            for peer in connected_peers {
                swarm
                    .behaviour_mut()
                    .socket
                    .send_request(&peer, request.clone());
            }
        }
        gossipsub::Event::Subscribed { peer_id, topic } => {
            info!("#{} <{}> Connected", topic, peer_id);
        }
        gossipsub::Event::Unsubscribed { peer_id, topic } => {
            info!("#{} <{}> Disconnected", topic, peer_id);
        }
        _ => {}
    }
}

pub async fn user_socket_handler(
    interface: &[Multiaddr],
    mut tx: UnboundedSender<RequestEvent>,
    mut rx: UnboundedReceiver<gossipsub::Event>,
) {
    let mut swarm = init_swarm(interface);
    warn!("Broadcasting to all peers");

    select! {
        Some(event) = rx.recv() => outbound_handle(&mut swarm, event),
        event = swarm.select_next_some() => inbound_handle(&mut swarm, event, &mut tx),
    }
}
