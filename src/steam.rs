use crate::{
    Client, ClientCallback, ClientTrait, ClientType, ClientTypeRef, Compression, Message, NetError,
    NetworkingInfo, PeerId, Reliability, pack, unpack,
};
use bitcode::{DecodeOwned, Encode};
#[cfg(feature = "log")]
use log::info;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use steamworks::networking_sockets::{InvalidHandle, ListenSocket, NetConnection, NetPollGroup};
use steamworks::networking_types::{
    ListenSocketEvent, NetConnectionStatusChanged, NetworkingConnectionState, NetworkingIdentity,
    NetworkingMessage, SendFlags,
};
use steamworks::{
    CallbackResult, GameLobbyJoinRequested, LobbyId, LobbyType, SteamAPIInitError, SteamId,
};
pub(crate) struct Connection {
    pub(crate) net: NetConnection,
    pub(crate) connected: bool,
}
pub struct SteamClient {
    pub(crate) steam_client: steamworks::Client,
    pub(crate) my_id: PeerId,
    pub(crate) host_id: PeerId,
    pub(crate) lobby_id: LobbyId,
    pub(crate) connections: HashMap<PeerId, Connection>,
    pub(crate) poll_group: NetPollGroup,
    pub(crate) listen_socket: Option<Mutex<ListenSocket>>,
    pub(crate) peer_connected: ClientCallback,
    pub(crate) peer_disconnected: ClientCallback,
    pub(crate) buffer: Vec<NetworkingMessage>,
    rx: Arc<Mutex<Receiver<LobbyId>>>,
    tx: Arc<Mutex<Sender<LobbyId>>>,
}
unsafe impl Send for SteamClient {}
unsafe impl Sync for SteamClient {}
impl SteamClient {
    pub fn info(&self) -> NetworkingInfo {
        let mut v = Vec::new();
        let sockets = self.steam_client.networking_sockets();
        for (peer, con) in &self.connections {
            v.push((
                *peer,
                sockets
                    .get_realtime_connection_status(&con.net, 0)
                    .unwrap()
                    .0,
            ));
        }
        NetworkingInfo(v)
    }
    fn reset(&mut self) {
        self.host_id = PeerId(0);
        self.lobby_id = LobbyId::from_raw(0);
        self.connections = Default::default();
        self.listen_socket = None;
    }
    pub(crate) fn new(
        app_id: u32,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
    ) -> Result<Self, SteamAPIInitError> {
        let steam_client = steamworks::Client::init_app(app_id)?;
        steam_client.networking_utils().init_relay_network_access();
        let poll_group = steam_client.networking_sockets().create_poll_group();
        let my_id = steam_client.user().steam_id().into();
        let (tx, rx) = channel();
        Ok(Self {
            steam_client,
            my_id,
            host_id: PeerId(0),
            lobby_id: LobbyId::from_raw(0),
            connections: Default::default(),
            poll_group,
            peer_connected,
            peer_disconnected,
            buffer: Vec::with_capacity(64),
            listen_socket: None,
            rx: Arc::new(rx.into()),
            tx: Arc::new(tx.into()),
        })
    }
    pub(crate) fn host(&mut self) -> Result<(), InvalidHandle> {
        self.reset();
        self.host_id = self.my_id;
        self.listen_socket = Some(
            self.steam_client
                .networking_sockets()
                .create_listen_socket_p2p(0, None)?
                .into(),
        );
        let tx = self.tx.clone();
        self.steam_client
            .matchmaking()
            .create_lobby(LobbyType::FriendsOnly, 250, move |s| {
                if let Ok(s) = s {
                    let _ = tx.lock().unwrap().send(s);
                }
            });
        Ok(())
    }
    pub(crate) fn join(&mut self, id: LobbyId) {
        self.reset();
        let tx = self.tx.clone();
        self.steam_client.matchmaking().join_lobby(id, move |s| {
            if let Ok(s) = s {
                let _ = tx.lock().unwrap().send(s);
            }
        })
    }
    pub(crate) fn recv<T, F>(&mut self, mut f: F)
    where
        F: FnMut(ClientTypeRef, Message<T>),
        T: DecodeOwned,
    {
        self.poll_group.receive_messages_to_buffer(&mut self.buffer);
        while !self.buffer.is_empty() {
            for m in &self.buffer {
                let src = m.identity_peer().steam_id().unwrap().into();
                let data = unpack(m.data());
                f(ClientTypeRef::Steam(self), Message { src, data })
            }
            self.buffer.clear();
            self.poll_group.receive_messages_to_buffer(&mut self.buffer);
        }
    }
    pub(crate) fn recv_raw<F>(&mut self, mut f: F)
    where
        F: FnMut(ClientTypeRef, Message<&[u8]>),
    {
        self.poll_group.receive_messages_to_buffer(&mut self.buffer);
        while !self.buffer.is_empty() {
            for m in &self.buffer {
                let src = m.identity_peer().steam_id().unwrap().into();
                let data = &m.data();
                f(ClientTypeRef::Steam(self), Message { src, data })
            }
            self.buffer.clear();
            self.poll_group.receive_messages_to_buffer(&mut self.buffer);
        }
    }
    fn connect(&mut self, id: SteamId) {
        let peer_identity = NetworkingIdentity::new_steam_id(id);
        let connection = self
            .steam_client
            .networking_sockets()
            .connect_p2p(peer_identity, 0, None)
            .unwrap();
        connection.set_poll_group(&self.poll_group);
        self.connections.insert(
            id.into(),
            Connection {
                net: connection,
                connected: false,
            },
        );
    }
    pub(crate) fn update(&mut self) {
        while let Ok(event) = self.rx.clone().lock().unwrap().try_recv() {
            self.lobby_id = event;
            if !self.is_host() {
                let matchmaking = self.steam_client.matchmaking();
                let owner = matchmaking.lobby_owner(event);
                self.host_id = owner.into();
                for id in matchmaking.lobby_members(event) {
                    if id != self.my_id.into() {
                        self.connect(id)
                    }
                }
            }
        }
        if let Some(listen) = &self.listen_socket {
            let listen = listen.lock().unwrap();
            while let Some(event) = listen.try_receive_event() {
                match event {
                    ListenSocketEvent::Connecting(event) => {
                        #[cfg(feature = "log")]
                        info!("connecting to someone");
                        event.accept().unwrap();
                    }
                    ListenSocketEvent::Connected(event) => {
                        let id = event.remote().steam_id().unwrap();
                        #[cfg(feature = "log")]
                        info!("connected to {id:?}");
                        let connection = event.take_connection();
                        connection.set_poll_group(&self.poll_group);
                        let connection = Connection {
                            net: connection,
                            connected: true,
                        };
                        self.connections.insert(id.into(), connection);
                        if let Some(mut c) = self.peer_connected.take() {
                            c(ClientTypeRef::Steam(self), id.into());
                            self.peer_connected = Some(c);
                        }
                    }
                    ListenSocketEvent::Disconnected(event) => {
                        let id = event.remote().steam_id().unwrap();
                        self.connections.remove(&id.into());
                        if let Some(mut d) = self.peer_disconnected.take() {
                            d(ClientTypeRef::Steam(self), id.into());
                            self.peer_disconnected = Some(d);
                        }
                        #[cfg(feature = "log")]
                        info!("disconnected from {id:?}");
                    }
                }
            }
        } else {
            self.steam_client
                .clone()
                .process_callbacks(|callback| match callback {
                    CallbackResult::GameLobbyJoinRequested(GameLobbyJoinRequested {
                        lobby_steam_id,
                        ..
                    }) => self.join(lobby_steam_id),
                    CallbackResult::NetConnectionStatusChanged(NetConnectionStatusChanged {
                        connection_info,
                        ..
                    }) => match connection_info.state() {
                        Ok(NetworkingConnectionState::Connected) => {
                            let peer = connection_info
                                .identity_remote()
                                .unwrap()
                                .steam_id()
                                .unwrap();
                            if let Some(con) = self.connections.get_mut(&peer.into()) {
                                #[cfg(feature = "log")]
                                info!("connected to {peer:?}");
                                con.connected = true;
                                if let Some(mut c) = self.peer_connected.take() {
                                    c(ClientTypeRef::Steam(self), peer.into());
                                    self.peer_connected = Some(c);
                                }
                            }
                        }
                        Ok(NetworkingConnectionState::ClosedByPeer) => {
                            let peer = connection_info
                                .identity_remote()
                                .unwrap()
                                .steam_id()
                                .unwrap();
                            self.connections.remove(&peer.into());
                            #[cfg(feature = "log")]
                            info!("disconnected from {peer:?}");
                            if let Some(mut d) = self.peer_disconnected.take() {
                                d(ClientTypeRef::Steam(self), peer.into());
                                self.peer_disconnected = Some(d);
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                });
        }
    }
}
impl ClientTrait for SteamClient {
    fn send<T: Encode>(
        &self,
        dest: PeerId,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        self.send_raw(dest, &pack(data, compression), reliability)
    }
    fn broadcast<T: Encode>(
        &self,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        self.broadcast_raw(&pack(data, compression), reliability)
    }
    fn send_raw(
        &self,
        dest: PeerId,
        data: &[u8],
        reliability: Reliability,
    ) -> Result<(), NetError> {
        if let Some(con) = self.connections.get(&dest)
            && con.connected
        {
            con.net.send_message(data, reliability.into())?;
        }
        Ok(())
    }
    fn broadcast_raw(&self, data: &[u8], reliability: Reliability) -> Result<(), NetError> {
        for con in self.connections.values() {
            if con.connected {
                con.net.send_message(data, reliability.into())?;
            }
        }
        Ok(())
    }
    fn my_id(&self) -> PeerId {
        self.my_id
    }
    fn host_id(&self) -> PeerId {
        self.host_id
    }
    fn is_host(&self) -> bool {
        self.listen_socket.is_some()
    }
    fn peer_len(&self) -> usize {
        self.connections.len()
    }
}
impl From<Reliability> for SendFlags {
    fn from(value: Reliability) -> Self {
        match value {
            Reliability::Reliable => SendFlags::RELIABLE,
            Reliability::Unreliable => SendFlags::UNRELIABLE,
        }
    }
}
impl From<SteamId> for PeerId {
    fn from(value: SteamId) -> Self {
        Self(value.raw())
    }
}
impl From<PeerId> for SteamId {
    fn from(value: PeerId) -> Self {
        Self::from_raw(value.raw())
    }
}
impl Client {
    pub fn host_steam(&mut self) -> Result<(), InvalidHandle> {
        let ClientType::Steam(client) = &mut self.client else {
            #[cfg(feature = "log")]
            log!("steam not initialized, not hosting");
            return Ok(());
        };
        client.host()
    }
    pub fn join_steam(&mut self, lobby: u64) {
        let ClientType::Steam(client) = &mut self.client else {
            #[cfg(feature = "log")]
            log!("steam not initialized, not joining");
            return;
        };
        client.join(LobbyId::from_raw(lobby));
    }
    pub fn flush(&self) {
        if let ClientType::Steam(client) = &self.client {
            client.connections.values().for_each(|c| {
                if c.connected {
                    c.net.flush_messages().unwrap();
                }
            })
        }
    }
    pub fn init_steam(
        &mut self,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
    ) -> Result<(), SteamAPIInitError> {
        if !matches!(self.client, ClientType::Steam(_)) {
            self.client = ClientType::Steam(SteamClient::new(
                self.app_id,
                peer_connected,
                peer_disconnected,
            )?);
        }
        Ok(())
    }
}
