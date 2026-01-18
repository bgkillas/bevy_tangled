#[cfg(feature = "tangled")]
mod ip;
#[cfg(feature = "steam")]
mod steam;
#[cfg(feature = "tangled")]
use crate::ip::IpClient;
#[cfg(feature = "steam")]
use crate::steam::SteamClient;
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
pub enum ClientTypeRef<'a> {
    #[cfg(feature = "steam")]
    Steam(&'a SteamClient),
    #[cfg(feature = "tangled")]
    Ip(&'a IpClient),
    #[cfg(not(any(feature = "steam", feature = "tangled")))]
    None(&'a ()),
}
#[cfg_attr(feature = "bevy", derive(Resource))]
pub struct Client {
    #[cfg(feature = "steam")]
    steam_client: SteamClient,
    #[cfg(feature = "tangled")]
    ip_client: Option<IpClient>,
}
pub enum ClientMode {
    Steam,
    Ip,
    None,
}
impl Client {
    pub fn new(
        #[cfg(feature = "steam")] app_id: u32,
        #[cfg(feature = "steam")] peer_connected: ClientCallback,
        #[cfg(feature = "steam")] peer_disconnected: ClientCallback,
    ) -> Option<Self> {
        Some(Self {
            #[cfg(feature = "steam")]
            steam_client: SteamClient::new(app_id, peer_connected, peer_disconnected).ok()?,
            #[cfg(feature = "tangled")]
            ip_client: None,
        })
    }
    pub fn recv<T, F>(&mut self, f: F)
    where
        F: FnMut(ClientTypeRef, Message<T>),
        T: DecodeOwned,
    {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &mut self.ip_client {
            ip.recv(f);
            return;
        }
        #[cfg(feature = "steam")]
        self.steam_client.recv(f)
    }
    pub fn recv_raw<F>(&mut self, f: F)
    where
        F: FnMut(ClientTypeRef, Message<&[u8]>),
    {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &mut self.ip_client {
            ip.recv_raw(f);
            return;
        }
        #[cfg(feature = "steam")]
        self.steam_client.recv_raw(f)
    }
    #[allow(clippy::result_unit_err)]
    pub fn update(&mut self) -> UResult {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &mut self.ip_client {
            ip.update();
            return Ok(());
        }
        #[cfg(feature = "steam")]
        self.steam_client.update()?;
        Ok(())
    }
    pub fn info(&self) -> NetworkingInfo {
        #[cfg(feature = "steam")]
        {
            self.steam_client.info()
        }
        #[cfg(not(feature = "steam"))]
        {
            NetworkingInfo()
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
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.send(dest, data, reliability, compression);
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.send(dest, data, reliability, compression)
        }
        #[cfg(not(feature = "steam"))]
        {
            Ok(())
        }
    }
    fn broadcast<T: Encode>(
        &self,
        data: &T,
        reliability: Reliability,
        compression: Compression,
    ) -> Result<(), NetError> {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.broadcast(data, reliability, compression);
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.broadcast(data, reliability, compression)
        }
        #[cfg(not(feature = "steam"))]
        {
            Ok(())
        }
    }
    fn send_raw(
        &self,
        dest: PeerId,
        data: &[u8],
        reliability: Reliability,
    ) -> Result<(), NetError> {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.send_raw(dest, data, reliability);
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.send_raw(dest, data, reliability)
        }
        #[cfg(not(feature = "steam"))]
        {
            Ok(())
        }
    }
    fn broadcast_raw(&self, data: &[u8], reliability: Reliability) -> Result<(), NetError> {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.broadcast_raw(data, reliability);
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.broadcast_raw(data, reliability)
        }
        #[cfg(not(feature = "steam"))]
        {
            Ok(())
        }
    }
    fn my_id(&self) -> PeerId {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.my_id();
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.my_id()
        }
        #[cfg(not(feature = "steam"))]
        {
            PeerId(0)
        }
    }
    fn host_id(&self) -> PeerId {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.host_id();
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.host_id()
        }
        #[cfg(not(feature = "steam"))]
        {
            PeerId(0)
        }
    }
    fn is_host(&self) -> bool {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.is_host();
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.is_host()
        }
        #[cfg(not(feature = "steam"))]
        {
            false
        }
    }
    fn peer_len(&self) -> usize {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.peer_len();
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.peer_len()
        }
        #[cfg(not(feature = "steam"))]
        {
            0
        }
    }
    fn is_connected(&self) -> bool {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.is_connected();
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.is_connected()
        }
        #[cfg(not(feature = "steam"))]
        {
            false
        }
    }
    fn mode(&self) -> ClientMode {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.mode();
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.mode()
        }
        #[cfg(not(feature = "steam"))]
        {
            ClientMode::None
        }
    }
    fn get_name(&self) -> Option<String> {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.get_name();
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.get_name()
        }
        #[cfg(not(feature = "steam"))]
        {
            None
        }
    }
    fn get_name_of(&self, id: PeerId) -> Option<String> {
        #[cfg(feature = "tangled")]
        if let Some(ip) = &self.ip_client {
            return ip.get_name_of(id);
        }
        #[cfg(feature = "steam")]
        {
            self.steam_client.get_name_of(id)
        }
        #[cfg(not(feature = "steam"))]
        {
            None
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
    fn mode(&self) -> ClientMode {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => ClientMode::None,
            #[cfg(feature = "steam")]
            Self::Steam(_) => ClientMode::Steam,
            #[cfg(feature = "tangled")]
            Self::Ip(_) => ClientMode::Ip,
        }
    }
    fn get_name(&self) -> Option<String> {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => None,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.get_name(),
            #[cfg(feature = "tangled")]
            Self::Ip(_) => None,
        }
    }
    fn get_name_of(&self, id: PeerId) -> Option<String> {
        match &self {
            #[cfg(not(any(feature = "steam", feature = "tangled")))]
            Self::None(_) => None,
            #[cfg(feature = "steam")]
            Self::Steam(client) => client.get_name_of(id),
            #[cfg(feature = "tangled")]
            Self::Ip(_) => None,
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
    fn mode(&self) -> ClientMode;
    fn get_name(&self) -> Option<String>;
    fn get_name_of(&self, id: PeerId) -> Option<String>;
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
pub fn update(mut client: bevy_ecs::system::ResMut<Client>) {
    if let Err(_s) = client.update() {
        #[cfg(feature = "log")]
        warn!("{_s}")
    }
}
#[cfg(not(feature = "steam"))]
#[cfg(feature = "tangled")]
#[cfg(test)]
#[tokio::test]
async fn test_ip() {
    let mut host = Client::new().unwrap();
    host.host_ip(None, None).unwrap();
    let mut peer1 = Client::new().unwrap();
    peer1
        .join_ip("127.0.0.1".parse().unwrap(), None, None)
        .unwrap();
    let mut peer2 = Client::new().unwrap();
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
