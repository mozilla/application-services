use std::marker::PhantomData;

use crate::bn::{BigNum, BigNumContext};
use crate::error::{fatal, ErrorStack};
use crate::nid::Nid;
use crate::pkey::{Private, Public};

pub struct EcGroup {}

#[derive(Clone)]
pub struct EcKey<T> {
    phantom: PhantomData<T>,
}

pub struct EcPoint {}
pub enum PointConversionForm {
    UNCOMPRESSED,
}

impl EcPoint {
    pub fn from_bytes(_: &EcGroup, _: &[u8], _: &BigNumContext) -> Result<Self, ErrorStack> {
        fatal()
    }

    pub fn to_bytes(
        &self,
        _: &EcGroup,
        _: PointConversionForm,
        _: &BigNumContext,
    ) -> Result<Vec<u8>, ErrorStack> {
        fatal()
    }

    pub fn affine_coordinates_gfp(
        &self,
        _: &EcGroup,
        _: &mut BigNum,
        _: &mut BigNum,
        _: &BigNumContext,
    ) -> Result<(), ErrorStack> {
        fatal()
    }
}

impl<T> EcKey<T> {
    pub fn from_public_key(_: &EcGroup, _: &EcPoint) -> Result<Self, ErrorStack> {
        fatal()
    }

    pub fn generate(_: &EcGroup) -> Result<Self, ErrorStack> {
        fatal()
    }

    pub fn public_key(&self) -> EcPoint {
        fatal()
    }
}

impl EcKey<Public> {
    pub fn from_public_key_affine_coordinates(
        _: &EcGroup,
        _: &BigNum,
        _: &BigNum,
    ) -> Result<Self, ErrorStack> {
        fatal()
    }
}

impl EcKey<Private> {
    pub fn from_private_components(
        _: &EcGroup,
        _: &BigNum,
        _: EcPoint,
    ) -> Result<Self, ErrorStack> {
        fatal()
    }

    pub fn private_key(&self) -> &'static BigNum {
        fatal()
    }
}

impl EcGroup {
    pub fn from_curve_name(_: Nid) -> Result<Self, ErrorStack> {
        fatal()
    }
}
