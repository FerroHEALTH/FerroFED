// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A base URL no connection can reach: port 0 on the loopback interface.
//!
//! A port bound and then released goes back to the operating system, which
//! can hand it to another test process before the connection is tried, so a
//! released port is no unreachable node. Port 0 cannot be listened on at
//! all: binding it asks the operating system for some other free port. A
//! connection to it fails at once, refused on Linux and refused as an
//! unassignable address on macOS, and never waits for a timeout. No
//! specification governs this: our own design.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// The address: port 0 on `127.0.0.1`.
pub const ADDRESS: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

/// The `http` base URL of [`ADDRESS`], in the form `MockServer::uri` gives.
pub const BASE: &str = "http://127.0.0.1:0";
