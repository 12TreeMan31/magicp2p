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

use libp2p::request_response::{self as reqres, ProtocolSupport, cbor};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{PeerId, StreamProtocol};
use libp2p_stream as stream;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, derive_more::FromStr)]
pub enum RequestType {
    JOIN,
    PART,
    MESG,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestEvent {
    pub kind: RequestType,
    // TODO: Make a multihash
    pub channel: String,
    pub data: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ResponseEvent {
    Ok,
    Err,
}

pub type Behaviour = cbor::Behaviour<RequestEvent, ResponseEvent>;
pub type Event = reqres::Event<RequestEvent, ResponseEvent>;

pub const PROTOCOL: StreamProtocol = StreamProtocol::new("/magic");
pub const CLIENT_PROTOCOL: (StreamProtocol, ProtocolSupport) =
    (PROTOCOL, ProtocolSupport::Outbound);
pub(crate) const SERVER_PROTOCOL: (StreamProtocol, ProtocolSupport) =
    (PROTOCOL, ProtocolSupport::Inbound);

#[derive(NetworkBehaviour)]
struct SocketBehaviour {
    socket: cbor::Behaviour<RequestEvent, ResponseEvent>,
    stream: stream::Behaviour,
}

impl SocketBehaviour {
    pub fn new_client() -> Self {
        let socket = cbor::Behaviour::new(
            [(PROTOCOL, ProtocolSupport::Outbound)],
            reqres::Config::default(),
        );
        let stream = stream::Behaviour::new();
        Self { socket, stream }
    }
    pub(crate) fn new_server() -> Self {
        let socket = cbor::Behaviour::new(
            [(PROTOCOL, ProtocolSupport::Inbound)],
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

/* #[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub socket: cbor::Behaviour<RequestEvent, ResponseEvent>,
}

impl Behaviour {
    pub fn new_client() -> Self {
        unimplemented!()
    }
    pub(crate) fn new_server() -> Self {
        unimplemented!()
    }
    pub fn join(&mut self, server_id: PeerId, channel: String) {}
    pub fn part(&mut self, server_id: PeerId, channel: String) {}
    pub fn publish(&mut self, server_id: PeerId, channel: String) {}
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = ;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.socket.handle_established_inbound_connection(
            _connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }
    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: libp2p::core::Endpoint,
        port_use: libp2p::core::transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.socket.handle_established_outbound_connection(
            _connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }
    fn handle_pending_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<(), libp2p::swarm::ConnectionDenied> {
        self.socket
            .handle_pending_inbound_connection(_connection_id, _local_addr, _remote_addr)
    }
    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: libp2p::core::Endpoint,
    ) -> Result<Vec<Multiaddr>, libp2p::swarm::ConnectionDenied> {
        self.socket.handle_pending_outbound_connection(
            _connection_id,
            _maybe_peer,
            _addresses,
            _effective_role,
        )
    }
    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: libp2p::swarm::ConnectionId,
        _event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        self.socket
            .on_connection_handler_event(_peer_id, _connection_id, _event);
    }
    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
        self.socket.on_swarm_event(event);
    }
    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        self.socket.poll(cx)
    }
}*/

////

////

/*impl Deref for Behaviour {
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
        SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {}", address),
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

/// Handler for all local interactions. It will start its own swarm and
/// should only listen on local interfaces since this give absolute
/// control over the progeam.
///
/// `user_input_tx` is for messages that are generated by the user.
/// `message_rx` is for recving all pubsub event generated by magic
/// which will then be sent to the user as is.
pub async fn user_socket_handler(
    interface: &[Multiaddr],
    mut user_input_tx: UnboundedSender<RequestEvent>,
    mut message_rx: UnboundedReceiver<gossipsub::Event>,
) -> ! {
    let mut swarm = init_swarm(interface);
    warn!("Broadcasting to all peers");

    loop {
        select! {
            Some(event) = message_rx.recv() => outbound_handle(&mut swarm, event),
            event = swarm.select_next_some() => inbound_handle(&mut swarm, event, &mut user_input_tx),
        }
    }
} */
