use crate::IngressECIESValue;
use std::fmt;
use thiserror::Error;

/// An error that occurs while reading or writing to an ECIES stream.
pub struct ECIESError {
    inner: Box<ECIESErrorImpl>,
}

// === impl ===

impl ECIESError {
    /// Consumes the type and returns the error enum
    pub fn into_inner(self) -> ECIESErrorImpl {
        *self.inner
    }
}

impl fmt::Debug for ECIESError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.inner, f)
    }
}

impl fmt::Display for ECIESError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.inner, f)
    }
}

impl std::error::Error for ECIESError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }
}

/// An error that occurs while reading or writing to an ECIES stream.
#[derive(Debug, Error)]
pub enum ECIESErrorImpl {
    /// Error during IO
    #[error("IO error")]
    IO(std::io::Error),
    /// Error when checking the HMAC tag against the tag on the data
    #[error("tag check failure")]
    TagCheckFailed,
    /// Error when parsing AUTH data
    #[error("invalid auth data")]
    InvalidAuthData,
    /// Error when parsing ACK data
    #[error("invalid ack data")]
    InvalidAckData,
    /// Error when reading the header if its length is <3
    #[error("invalid body data")]
    InvalidHeader,
    /// Error when interacting with secp256k1
    #[error(transparent)]
    Secp256k1(secp256k1::Error),
    /// Error when decoding RLP data
    #[error(transparent)]
    RLPDecoding(reth_rlp::DecodeError),
    /// Error when convering to integer
    #[error(transparent)]
    FromInt(std::num::TryFromIntError),
    /// Error when trying to split an array beyond its length
    #[error("requested {idx} but array len is {len}")]
    OutOfBounds {
        /// The index you are trying to split at
        idx: usize,
        /// The length of the array
        len: usize,
    },
    /// Error when handshaking with a peer (ack / auth)
    #[error("invalid handshake: expected {expected:?}, got {msg:?} instead")]
    InvalidHandshake {
        /// The expected return value from the peer
        expected: IngressECIESValue,
        /// The actual value returned from the peer
        msg: Option<IngressECIESValue>,
    },
}

impl From<ECIESErrorImpl> for ECIESError {
    fn from(source: ECIESErrorImpl) -> Self {
        ECIESError { inner: Box::new(source) }
    }
}

impl From<std::io::Error> for ECIESError {
    fn from(source: std::io::Error) -> Self {
        ECIESErrorImpl::IO(source).into()
    }
}

impl From<secp256k1::Error> for ECIESError {
    fn from(source: secp256k1::Error) -> Self {
        ECIESErrorImpl::Secp256k1(source).into()
    }
}

impl From<reth_rlp::DecodeError> for ECIESError {
    fn from(source: reth_rlp::DecodeError) -> Self {
        ECIESErrorImpl::RLPDecoding(source).into()
    }
}

impl From<std::num::TryFromIntError> for ECIESError {
    fn from(source: std::num::TryFromIntError) -> Self {
        ECIESErrorImpl::FromInt(source).into()
    }
}
