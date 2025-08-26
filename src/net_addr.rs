// MIT License
// Copyright (c) Valan Sai 2025
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.






use nymlib::nymsocket::SockAddr;
use nymlib::nymsocket::To;
use nymlib::nymsocket::impl_to_for;

use nymlib::serialize::Serialize;
use nymlib::serialize_derive::impl_serialize_for_struct;

use std::fmt;

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Address {
    pub address: SockAddr,     // Wrapped SockAddr (nym address, anonymous sendertag, or null)
    pub services: u32,         // Bitmask of supported services
    pub last_failed_time: u64, // Timestamp of the last failed connection attempt
}

impl Address {
    /// Create a new Address with the given socket address and services.
    pub fn new(address: SockAddr, services: u32) -> Self {
        Self {
            address,
            services,
            last_failed_time: 0,
        }
    }

    /// Returns true if the socket address represents an nym address.
    pub fn is_individual(&self) -> bool {
        self.address.is_individual()
    }

    /// Returns true if the address represents an anonymous sender tag.
    pub fn is_anonymous(&self) -> bool {
        self.address.is_anonymous()
    }

    /// Returns true if the address represents a null value.
    pub fn is_null(&self) -> bool {
        self.address.is_null()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " Address(address={:?}, services={})", self.address.to_string(), self.services)
    }
}

impl_serialize_for_struct! {
    target Address {
        readwrite(self.address); 
        readwrite(self.services); 
    }
}


impl From<SockAddr> for Address {
    fn from(addr: SockAddr) -> Self {
        Address::new(addr, 0)
    }
}

impl_to_for!(Address);