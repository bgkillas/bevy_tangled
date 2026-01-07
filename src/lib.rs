#[cfg(feature = "tangled")]
mod ip;
#[cfg(feature = "steam")]
mod steam;
#[cfg(feature = "tangled")]
use crate::ip::IpClient;
#[cfg(feature = "steam")]
use crate::steam::SteamClient;
#[cfg(feature = "bevy")]
use bevy_app::{App, Plugin};
#[cfg(feature = "bevy")]
use bevy_ecs::component::Component;
#[cfg(feature = "bevy")]
use bevy_ecs::resource::Resource;
use bitcode::{Decode, Encode};
use bitcode::{DecodeOwned, decode, encode};
#[cfg(feature = "compress")]
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
#[cfg(feature = "steam")]
pub use steamworks::LobbyId;
#[cfg(feature = "steam")]
pub use steamworks::SteamError;
#[cfg(feature = "steam")]
use steamworks::networking_types::NetConnectionRealTimeInfo;
type ClientCallback = Option<Box<dyn FnMut(ClientTypeRef, PeerId) + Send + Sync + 'static>>;
pub struct Message<T> {
    pub src: PeerId,
    pub data: T,
}
#[derive(Copy, Debug, Clone, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub enum Reliability {
    Reliable,
    Unreliable,
}
#[derive(Copy, Debug, Clone, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub enum Compression {
    Compressed,
    Uncompressed,
}
#[derive(Encode, Decode, Copy, Debug, Clone, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[cfg_attr(feature = "bevy", derive(Component))]
pub struct PeerId(pub u64);
impl Deref for PeerId {
    type Target = u64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Display for PeerId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl PeerId {
    pub fn raw(&self) -> u64 {
        self.0
    }
}
#[allow(unused_variables)]
pub(crate) fn pack<T: Encode>(data: &T, compression: Compression) -> Vec<u8> {
    #[allow(unused_mut)]
    let mut data = encode(data);
    #[cfg(feature = "compress")]
    {
        match compression {
            Compression::Compressed => {
                data = compress_prepend_size(&data);
                data.push(1);
            }
            Compression::Uncompressed => {
                data.push(0);
            }
        };
        data
    }
    #[cfg(not(feature = "compress"))]
    {
        data
    }
}
pub(crate) fn unpack<T: DecodeOwned>(data: &[u8]) -> T {
    #[cfg(feature = "compress")]
    let data = if *data.last().unwrap() == 1 {
        &decompress_size_prepended(&data[..data.len() - 1]).unwrap()
    } else {
        &data[..data.len() - 1]
    };
    decode(data).unwrap()
}
pub(crate) enum ClientType {
    None,
    #[cfg(feature = "steam")]
    Steam(Box<SteamClient>),
    #[cfg(feature = "tangled")]
    Ip(IpClient),
}
pub enum ClientTypeRef<'a> {
    #[cfg(feature = "steam")]
    Steam(&'a SteamClient),
    #[cfg(feature = "tangled")]
    Ip(&'a IpClient),
    #[cfg(not(any(feature = "steam", feature = "tangled")))]
    None(&'a u8),
}
#[cfg_attr(feature = "bevy", derive(Resource))]
pub struct Client {
    client: ClientType,
    #[cfg(feature = "steam")]
    app_id: u32,
}
impl Default for Client {
    fn default() -> Self {
        Self {
            #[cfg(feature = "steam")]
            app_id: 480,
            client: ClientType::None,
        }
    }
}
impl Client {
    pub fn new(#[cfg(feature = "steam")] app_id: u32) -> Self {
        Self {
            #[cfg(feature = "steam")]
            app_id,
            ..Self::default()
        }
    }
    pub fn recv<T, F>(&mut self, f: F)
    where
        F: FnMut(ClientTypeRef, Message<T>),
        T: DecodeOwned,
    {
        match &mut self.client {
            ClientType::None => {}
            #[cfg(feature = "steam")]
            ClientType::Steam(client) => client.recv(f),
            #[cfg(feature = "tangled")]
            ClientType::Ip(client) => client.recv(f),
        }
    }
    pub fn recv_raw<F>(&mut self, f: F)
    where
        F: FnMut(ClientTypeRef, Message<&[u8]>),
    {
        match &mut self.client {
            ClientType::None => {}
            #[cfg(feature = "steam")]
            ClientType::Steam(client) => client.recv_raw(f),
            #[cfg(feature = "tangled")]
            ClientType::Ip(client) => client.recv_raw(f),
        }
    }
    #[allow(clippy::result_unit_err)]
    pub fn update(&mut self) -> UResult {
        match &mut self.client {
            ClientType::None => {}
            #[cfg(feature = "steam")]
            ClientType::Steam(client) => return client.update(),
            #[cfg(feature = "tangled")]
            ClientType::Ip(client) => client.update(),
        }
        Ok(())
    }
    pub fn info(&self) -> Option<NetworkingInfo> {
        match &self.client {
            ClientType::None => None,
            #[cfg(feature = "steam")]
            ClientType::Steam(client) => Some(client.info()),
            #[cfg(feature = "tangled")]
            ClientType::Ip(_) => None,
        }
    }
}
#[cfg(feature = "steam")]
type UResult = Result<(), SteamError>;
#[cfg(not(feature = "steam"))]
type UResult = Result<(), ()>;
pub struct NetworkingInfo(#[cfg(feature = "steam")] pub Vec<(PeerId, NetConnectionRealTimeInfo)>);
impl ClientTrait for Client {
    fn send<T: Encode>(
        &self,
        dest: PeerId,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        self.client.send(dest, data, reliability, compression)
    }
    fn broadcast<T: Encode>(
        &self,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        self.client.broadcast(data, reliability, compression)
    }
    fn send_raw(
        &self,
        dest: PeerId,
        data: &[u8],
        reliability: Reliability,
    ) -> Result<(), NetError> {
        self.client.send_raw(dest, data, reliability)
    }
    fn broadcast_raw(&self, data: &[u8], reliability: Reliability) -> Result<(), NetError> {
        self.client.broadcast_raw(data, reliability)
    }
    fn my_id(&self) -> PeerId {
        self.client.my_id()
    }
    fn host_id(&self) -> PeerId {
        self.client.host_id()
    }
    fn is_host(&self) -> bool {
        self.client.is_host()
    }
    fn peer_len(&self) -> usize {
        self.client.peer_len()
    }
    fn is_connected(&self) -> bool {
        self.client.is_connected()
    }
}
impl ClientTrait for ClientType {
    fn send<T: Encode>(
        &self,
        dest: PeerId,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        match &self {
            Self::None => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.send(dest, data, reliability, compression)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.send(dest, data, reliability, compression)?,
        }
        Ok(())
    }
    fn broadcast<T: Encode>(
        &self,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        match &self {
            Self::None => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.broadcast(data, reliability, compression)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.broadcast(data, reliability, compression)?,
        }
        Ok(())
    }
    fn send_raw(
        &self,
        dest: PeerId,
        data: &[u8],
        reliability: Reliability,
    ) -> Result<(), NetError> {
        match &self {
            Self::None => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.send_raw(dest, data, reliability)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.send_raw(dest, data, reliability)?,
        }
        Ok(())
    }
    fn broadcast_raw(&self, data: &[u8], reliability: Reliability) -> Result<(), NetError> {
        match &self {
            Self::None => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.broadcast_raw(data, reliability)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.broadcast_raw(data, reliability)?,
        }
        Ok(())
    }
    fn my_id(&self) -> PeerId {
        match &self {
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.my_id,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.my_id(),
            Self::None => PeerId(0),
        }
    }
    fn host_id(&self) -> PeerId {
        match &self {
            Self::None => PeerId(0),
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.host_id(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.host_id(),
        }
    }
    fn is_host(&self) -> bool {
        match &self {
            Self::None => true,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.is_host(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.is_host(),
        }
    }
    fn peer_len(&self) -> usize {
        match &self {
            Self::None => 0,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.peer_len(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.peer_len(),
        }
    }
    fn is_connected(&self) -> bool {
        match &self {
            Self::None => false,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.is_connected(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.is_connected(),
        }
    }
}
impl ClientTrait for ClientTypeRef<'_> {
    fn send<T: Encode>(
        &self,
        dest: PeerId,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.send(dest, data, reliability, compression)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.send(dest, data, reliability, compression)?,
        }
        Ok(())
    }
    fn broadcast<T: Encode>(
        &self,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.broadcast(data, reliability, compression)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.broadcast(data, reliability, compression)?,
        }
        Ok(())
    }
    fn send_raw(
        &self,
        dest: PeerId,
        data: &[u8],
        reliability: Reliability,
    ) -> Result<(), NetError> {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.send_raw(dest, data, reliability)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.send_raw(dest, data, reliability)?,
        }
        Ok(())
    }
    fn broadcast_raw(&self, data: &[u8], reliability: Reliability) -> Result<(), NetError> {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => {}
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.broadcast_raw(data, reliability)?,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.broadcast_raw(data, reliability)?,
        }
        Ok(())
    }
    fn my_id(&self) -> PeerId {
        match &self {
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.my_id,
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.my_id(),
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => PeerId(0),
        }
    }
    fn host_id(&self) -> PeerId {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => PeerId(0),
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.host_id(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.host_id(),
        }
    }
    fn is_host(&self) -> bool {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => true,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.is_host(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.is_host(),
        }
    }
    fn peer_len(&self) -> usize {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => 0,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.peer_len(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.peer_len(),
        }
    }
    fn is_connected(&self) -> bool {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => false,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.is_connected(),
            #[cfg(feature = "tangled")]
            Self::Ip(client) => client.is_connected(),
        }
    }
}
pub trait ClientTrait {
    fn send<T: Encode>(
        &self,
        dest: PeerId,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError>;
    fn broadcast<T: Encode>(
        &self,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError>;
    fn send_raw(&self, dest: PeerId, data: &[u8], reliability: Reliability)
    -> Result<(), NetError>;
    fn broadcast_raw(&self, data: &[u8], reliability: Reliability) -> Result<(), NetError>;
    fn my_id(&self) -> PeerId;
    fn host_id(&self) -> PeerId;
    fn is_host(&self) -> bool;
    fn peer_len(&self) -> usize;
    fn is_connected(&self) -> bool;
}
#[derive(Debug)]
pub enum NetError {
    #[cfg(feature = "tangled")]
    Tangled(tangled::NetError),
    #[cfg(feature = "steam")]
    Steam(SteamError),
}
impl Display for NetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
#[cfg(feature = "tangled")]
impl From<tangled::NetError> for NetError {
    fn from(value: tangled::NetError) -> Self {
        Self::Tangled(value)
    }
}
#[cfg(feature = "steam")]
impl From<SteamError> for NetError {
    fn from(value: SteamError) -> Self {
        Self::Steam(value)
    }
}
impl Error for NetError {}
#[cfg(feature = "bevy")]
impl Plugin for Client {
    fn build(&self, app: &mut App) {
        app.insert_resource(Self {
            #[cfg(feature = "steam")]
            app_id: self.app_id,
            client: ClientType::None,
        });
    }
}
#[cfg(feature = "bevy")]
pub fn update(mut client: bevy_ecs::system::ResMut<Client>) {
    let _ = client.update();
}
#[cfg(feature = "tangled")]
#[cfg(test)]
#[tokio::test]
async fn test_ip() {
    let mut host = Client::new(0);
    host.host_ip(None, None).unwrap();
    let mut peer1 = Client::new(0);
    peer1
        .join_ip("127.0.0.1".parse().unwrap(), None, None)
        .unwrap();
    let mut peer2 = Client::new(0);
    peer2
        .join_ip("127.0.0.1".parse().unwrap(), None, None)
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let _ = peer1.update();
    let _ = peer2.update();
    peer2
        .broadcast(
            &[0u8, 1, 5, 3],
            Reliability::Reliable,
            Compression::Uncompressed,
        )
        .unwrap();
    peer2
        .broadcast(
            &[0u8, 1, 5, 3],
            Reliability::Reliable,
            Compression::Compressed,
        )
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let mut has = false;
    let mut n = 0;
    peer1.recv::<[u8; 4], _>(|_, m| {
        has = m.data == [0, 1, 5, 3];
        assert!(has);
        n += 1;
    });
    assert!(has);
    assert_eq!(n, 2);
    let mut has = false;
    host.recv::<[u8; 4], _>(|_, m| {
        has = m.data == [0, 1, 5, 3];
        assert!(has);
    });
    assert!(has)
}
