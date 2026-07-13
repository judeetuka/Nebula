extern crate self as wacore;

pub use aes_gcm;
pub use wacore_appstate as appstate;
pub use wacore_noise as noise;

// Re-export derive macros
pub use wacore_derive::{EmptyNode, ProtocolNode, StringEnum};

pub mod broadcast;
pub mod call;
pub mod client;
pub mod connectionevents;
pub mod download;
pub mod iq;
pub mod protocol;
pub use wacore_noise::framing;
pub mod handshake;
pub mod history_sync;
pub mod ib;
pub use wacore_libsignal as libsignal;
pub mod mediaretry;
pub mod messages;
pub mod msgbuilders;
pub mod msgsecret;
pub mod net;
pub mod newsletter;
pub mod notification;
pub mod pair;
pub mod pair_code;
pub mod prekeys;
pub mod proto_helpers;
pub mod receipt;
pub mod reporting_token;
pub mod request;
pub mod retry;
pub mod send;
pub mod stanza;
pub mod store;
pub mod types;
pub mod upload;
pub mod usync;
pub mod version;
pub mod xml;
