/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteSize(u64);

impl ByteSize {
    pub const fn b(value: u64) -> Self {
        Self(value)
    }

    pub const fn kib(value: u64) -> Self {
        Self(value.saturating_mul(1024))
    }

    pub const fn mib(value: u64) -> Self {
        Self(value.saturating_mul(1024).saturating_mul(1024))
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_size_constructors() {
        assert_eq!(ByteSize::b(1024).as_u64(), 1024);
        assert_eq!(ByteSize::mib(1).as_u64(), 1024 * 1024);
        assert_eq!(ByteSize::mib(10).as_u64(), 10 * 1024 * 1024);
    }

    #[test]
    fn test_byte_size_overflow() {
        assert_eq!(ByteSize::mib(u64::MAX).as_u64(), u64::MAX);
    }
}
