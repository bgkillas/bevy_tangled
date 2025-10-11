use crate::{
    Client, ClientCallback, ClientTrait, ClientType, ClientTypeRef, Compression, Message, NetError,
    PeerId, Reliability, pack, unpack,
};
use bitcode::{DecodeOwned, Encode};
use std::net::{IpAddr, SocketAddr};
use tangled::TangledInitError;
use tangled::{NetworkEvent, Peer};
use tokio::runtime::Runtime;
pub const DEFAULT_PORT: u16 = 5143;
pub struct IpClient {
    pub(crate) peer: Peer,
    pub(crate) peer_connected: ClientCallback,
    pub(crate) peer_disconnected: ClientCallback,
    connected: bool,
}
impl IpClient {
    pub(crate) fn host(
        socket_addr: SocketAddr,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
    ) -> Result<Self, TangledInitError> {
        Ok(Self {
            peer: Peer::host(socket_addr, None)?,
            peer_connected,
            peer_disconnected,
            connected: true,
        })
    }
    pub(crate) fn join(
        socket_addr: SocketAddr,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
    ) -> Result<Self, TangledInitError> {
        Ok(Self {
            peer: Peer::connect(socket_addr, None)?,
            peer_connected,
            peer_disconnected,
            connected: false,
        })
    }
    pub(crate) fn recv<T, F>(&mut self, mut f: F)
    where
        F: FnMut(ClientTypeRef, Message<T>),
        T: DecodeOwned,
    {
        if self.connected {
            for n in self.peer.recv() {
                match n {
                    NetworkEvent::Message(m) => {
                        let src = m.src.into();
                        let data = unpack(&m.data);
                        f(ClientTypeRef::Ip(self), Message { src, data })
                    }
                    NetworkEvent::PeerConnected(peer) => {
                        if let Some(mut c) = self.peer_connected.take() {
                            c(ClientTypeRef::Ip(self), peer.into());
                            self.peer_connected = Some(c);
                        }
                    }
                    NetworkEvent::PeerDisconnected(peer) => {
                        if let Some(mut d) = self.peer_disconnected.take() {
                            d(ClientTypeRef::Ip(self), peer.into());
                            self.peer_disconnected = Some(d);
                        }
                    }
                }
            }
        }
    }
    pub(crate) fn recv_raw<F>(&mut self, mut f: F)
    where
        F: FnMut(ClientTypeRef, Message<&[u8]>),
    {
        if self.connected {
            for n in self.peer.recv() {
                match n {
                    NetworkEvent::Message(m) => {
                        let src = m.src.into();
                        let data = &m.data;
                        f(ClientTypeRef::Ip(self), Message { src, data })
                    }
                    NetworkEvent::PeerConnected(peer) => {
                        if let Some(mut c) = self.peer_connected.take() {
                            c(ClientTypeRef::Ip(self), peer.into());
                            self.peer_connected = Some(c);
                        }
                    }
                    NetworkEvent::PeerDisconnected(peer) => {
                        if let Some(mut d) = self.peer_disconnected.take() {
                            d(ClientTypeRef::Ip(self), peer.into());
                            self.peer_disconnected = Some(d);
                        }
                    }
                }
            }
        }
    }
    pub(crate) fn update(&mut self) {
        if !self.connected && self.peer.my_id().is_some() {
            self.connected = true
        }
    }
}
impl ClientTrait for IpClient {
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
        if self.connected {
            self.peer.send(dest.into(), data, reliability.into())?;
        }
        Ok(())
    }
    fn broadcast_raw(&self, data: &[u8], reliability: Reliability) -> Result<(), NetError> {
        if self.connected {
            self.peer.broadcast(data, reliability.into())?;
        }
        Ok(())
    }
    fn my_id(&self) -> PeerId {
        self.peer.my_id().unwrap().into()
    }
    fn host_id(&self) -> PeerId {
        PeerId(0)
    }
    fn is_host(&self) -> bool {
        self.peer.my_id().unwrap() == tangled::PeerId(0)
    }
    fn peer_len(&self) -> usize {
        self.peer.iter_peer_ids().count()
    }
}
impl From<Reliability> for tangled::Reliability {
    fn from(value: Reliability) -> Self {
        match value {
            Reliability::Reliable => tangled::Reliability::Reliable,
            Reliability::Unreliable => tangled::Reliability::Unreliable,
        }
    }
}
impl From<tangled::PeerId> for PeerId {
    fn from(value: tangled::PeerId) -> Self {
        Self(value.0.into())
    }
}
impl From<PeerId> for tangled::PeerId {
    fn from(value: PeerId) -> Self {
        Self(value.raw() as u16)
    }
}
impl Client {
    pub fn host_ip(
        &mut self,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
    ) -> Result<(), TangledInitError> {
        let socket = SocketAddr::new("::".parse().unwrap(), DEFAULT_PORT);
        self.client = ClientType::Ip(IpClient::host(socket, peer_connected, peer_disconnected)?);
        Ok(())
    }
    pub fn join_ip(
        &mut self,
        addr: IpAddr,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
    ) -> Result<(), TangledInitError> {
        let socket = SocketAddr::new(addr, DEFAULT_PORT);
        self.client = ClientType::Ip(IpClient::join(socket, peer_connected, peer_disconnected)?);
        Ok(())
    }
    pub fn host_ip_runtime(
        &mut self,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
        runtime: &Runtime,
    ) -> Result<(), TangledInitError> {
        let socket = SocketAddr::new("::".parse().unwrap(), DEFAULT_PORT);
        let client =
            runtime.block_on(async { IpClient::host(socket, peer_connected, peer_disconnected) });
        self.client = ClientType::Ip(client?);
        Ok(())
    }
    pub fn join_ip_runtime(
        &mut self,
        addr: IpAddr,
        peer_connected: ClientCallback,
        peer_disconnected: ClientCallback,
        runtime: &Runtime,
    ) -> Result<(), TangledInitError> {
        let socket = SocketAddr::new(addr, DEFAULT_PORT);
        let client =
            runtime.block_on(async { IpClient::join(socket, peer_connected, peer_disconnected) });
        self.client = ClientType::Ip(client?);
        Ok(())
    }
}
