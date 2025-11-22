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

// use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use futures::{AsyncWriteExt, StreamExt};
use libp2p::request_response::{self as reqres, ProtocolSupport, cbor};
use libp2p::swarm::{NetworkBehaviour, Stream, Swarm, SwarmEvent, dial_opts::DialOpts};
use libp2p::{Multiaddr, PeerId, StreamProtocol, SwarmBuilder, gossipsub};
use libp2p_stream as stream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::info;

pub const MAGIC_PROTOCOL: StreamProtocol = StreamProtocol::new("/magic");

#[derive(Serialize, Deserialize, Debug, Clone, derive_more::FromStr)]
pub enum RequestType {
    JOIN,
    PART,
    MESG,
    STRM,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestEvent {
    pub kind: RequestType,
    // TODO: Make a multihash
    pub channel: String,
    // Right now is only for messages
    pub data: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ResponseEvent {
    Ok,
    Err,
}

#[derive(NetworkBehaviour)]
pub struct SocketBehaviour {
    socket: cbor::Behaviour<RequestEvent, ResponseEvent>,
    stream: stream::Behaviour,
}

impl SocketBehaviour {
    pub fn new_client() -> Self {
        let socket = cbor::Behaviour::new(
            [(MAGIC_PROTOCOL, ProtocolSupport::Outbound)],
            reqres::Config::default(),
        );
        let stream = stream::Behaviour::new();
        Self { socket, stream }
    }
    pub(crate) fn new_server() -> Self {
        let socket = cbor::Behaviour::new(
            [(MAGIC_PROTOCOL, ProtocolSupport::Inbound)],
            reqres::Config::default(),
        );

        let stream = stream::Behaviour::new();
        Self { socket, stream }
    }
    pub fn new_control(&self) -> stream::Control {
        self.stream.new_control()
    }

    pub fn send_request(
        &mut self,
        peer: &PeerId,
        request: RequestEvent,
    ) -> reqres::OutboundRequestId {
        self.socket.send_request(peer, request)
    }

    pub(crate) fn send_response(&mut self) -> Result<(), ResponseEvent> {
        unimplemented!()
    }
}

#[derive(Debug)]
pub enum ForwardRequest {
    Message { text: String, channel: String },
    Subscribe { channel: String },
    Unsubscribe { channel: String },
}

/// Actions to preform based on events ganerated
enum SwarmOpts {
    OpenStream,
    Forward(ForwardRequest),
    ClientFound(PeerId),
    ClientLost(PeerId),
}

fn init_swarm(interface: Multiaddr) -> Swarm<SocketBehaviour> {
    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_quic()
        .with_behaviour(|_| SocketBehaviour::new_server())
        .expect("Won't fail")
        .build();

    let dial = DialOpts::unknown_peer_id().address(interface).build();
    swarm.dial(dial).unwrap();

    swarm
}

fn swarm_event_handle(event: SwarmEvent<SocketBehaviourEvent>) -> Option<SwarmOpts> {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            info!("Listening on {}", address);
            None
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => Some(SwarmOpts::ClientFound(peer_id)),
        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => Some(SwarmOpts::ClientLost(peer_id)),
        // We don't need to handle the stream event
        SwarmEvent::Behaviour(SocketBehaviourEvent::Socket(e)) => match e {
            reqres::Event::Message { message, .. } => match message {
                // THIS IS WHERE WE GET `Event::InboundFailure` on the client!
                reqres::Message::Request {
                    request, channel, ..
                } => match request.kind {
                    RequestType::JOIN => Some(SwarmOpts::Forward(ForwardRequest::Subscribe {
                        channel: request.channel,
                    })),
                    RequestType::PART => Some(SwarmOpts::Forward(ForwardRequest::Unsubscribe {
                        channel: request.channel,
                    })),
                    RequestType::STRM => Some(SwarmOpts::OpenStream),
                    RequestType::MESG => match request.data {
                        Some(text) => Some(SwarmOpts::Forward(ForwardRequest::Message {
                            text,
                            channel: request.channel,
                        })),
                        _ => None, // Empty message so don't forward
                    },
                },
                _ => unreachable!(), // We shouldn't ever get a response
            },
            reqres::Event::ResponseSent { request_id, .. } => None,
            reqres::Event::InboundFailure {
                request_id, error, ..
            } => None,
            _ => unreachable!(), // We don't send requests
        },
        _ => None,
    }
}

async fn inbound_handle(
    event: SwarmEvent<SocketBehaviourEvent>,
    message_control: &mut libp2p_stream::Control,
    message_stream: &mut Option<Stream>,
    user_input_tx: &mut UnboundedSender<ForwardRequest>,
    client_id: &mut Option<PeerId>,
) {
    let opt = match swarm_event_handle(event) {
        Some(x) => x,
        _ => return,
    };

    match opt {
        SwarmOpts::ClientFound(peer_id) => *client_id = Some(peer_id),
        SwarmOpts::ClientLost(_) => *client_id = None,
        SwarmOpts::Forward(req) => user_input_tx.send(req).unwrap(),
        SwarmOpts::OpenStream => {
            if let Some(peer_id) = client_id {
                *message_stream = Some(
                    message_control
                        .open_stream(*peer_id, MAGIC_PROTOCOL)
                        .await
                        .unwrap(),
                );
            }
        }
    }
}

async fn outbound_handle(event: gossipsub::Event, message_stream: &mut Option<Stream>) {
    match event {
        gossipsub::Event::Message { message, .. } => {
            if let Some(stream) = message_stream {
                stream.write_all(&message.data).await.unwrap();
            }
        }
        _ => {}
    }
}

/// Handler for all local interactions. It will start its own swarm and
/// should only listen on local interfaces since this give absolute
/// control over the progeam.
///
/// `user_input_tx` is for messages that are generated by the user.
/// `message_rx` is for recving all pubsub event generated by magic
/// which will then be sent to the user as is.
pub async fn user_socket_handler(
    interface: Multiaddr,
    mut user_input_tx: UnboundedSender<ForwardRequest>,
    mut message_rx: UnboundedReceiver<gossipsub::Event>,
) -> ! {
    let mut swarm = init_swarm(interface);
    let mut client_id: Option<PeerId> = None;
    // Stream to send pubsub messages to user
    let mut message_control = swarm.behaviour().new_control();

    let mut message_stream: Option<Stream> = None;

    loop {
        tokio::select! {
            Some(event) = message_rx.recv() => outbound_handle(event, &mut message_stream).await,
            event = swarm.select_next_some() => inbound_handle(event, &mut message_control, &mut message_stream, &mut user_input_tx, &mut client_id).await,
        }
    }
}
