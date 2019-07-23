/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use failure::Fail;

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "Unknown engine: {}", _0)]
    UnknownEngine(String),
    #[fail(display = "Manager was compiled without support for {:?}", _0)]
    UnsupportedFeature(String),
    #[fail(display = "Database connection for '{}' is not open", _0)]
    ConnectionClosed(String),
    #[fail(display = "Handle is invalid: {}", _0)]
    InvalidHandle(#[fail(cause)] ffi_support::HandleError),
    #[fail(display = "Protobuf decode error: {}", _0)]
    ProtobufDecodeError(#[fail(cause)] prost::DecodeError),
}

error_support::define_error! {
    ErrorKind {
        (InvalidHandle, ffi_support::HandleError),
        (ProtobufDecodeError, prost::DecodeError),
    }
}
