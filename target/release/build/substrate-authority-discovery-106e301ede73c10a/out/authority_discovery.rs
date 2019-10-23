/// First we need to serialize the addresses in order to be able to sign them.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthorityAddresses {
    #[prost(bytes, repeated, tag="1")]
    pub addresses: ::std::vec::Vec<std::vec::Vec<u8>>,
}
/// Then we need to serialize addresses and signature to send them over the wire.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedAuthorityAddresses {
    #[prost(bytes, tag="1")]
    pub addresses: std::vec::Vec<u8>,
    #[prost(bytes, tag="2")]
    pub signature: std::vec::Vec<u8>,
}
