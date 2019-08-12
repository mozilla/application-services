use std::marker::PhantomData;

use crate::ec::EcKey;
use crate::error::{fatal, ErrorStack};

pub struct PKey<T> {
    phantom: PhantomData<T>,
}

#[derive(Clone)]
pub enum Private {}

#[derive(Clone)]
pub enum Public {}

impl<T> PKey<T> {
    pub fn from_ec_key(_: EcKey<T>) -> Result<Self, ErrorStack> {
        fatal()
    }
}
