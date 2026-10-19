//! The kitty graphics protocol: commands, transfers, decoding and storage.
//! `term/kitty.rs` applies commands to the terminal.
mod command;
mod decode;
mod diacritics;
mod media;
mod placeholder;
mod receiver;
mod store;

pub(crate) use command::{Command, Error};
pub(crate) use decode::decode;
pub(crate) use media::load;
pub(crate) use placeholder::{Cell as PlaceholderInput, PLACEHOLDER, resolve_row};
pub(crate) use receiver::{Receiver, Transmission};
pub(crate) use store::Store;

/// Largest decoded payload (image data or compressed data) per transmission.
pub(crate) const MAX_TRANSFER: usize = 256 << 20;
