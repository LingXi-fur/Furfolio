//! Windows runner spikes. Production IO stays fail-closed until these contracts pass.
mod windows;

pub(crate) use windows::{identity, rename_exclusive};
