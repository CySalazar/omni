//! # `omni-net`
//!
//! Userspace TCP/IP network stack service for OMNI OS.
//!
//! Implements layers N2.1 through N2.6 and N3.2 of the OMNI OS network
//! architecture:
//!
//! | Layer | Module | Description |
//! |-------|--------|-------------|
//! | N2.1 | [`arp`] | ARP table and IPv4 → MAC resolution |
//! | N2.2 | [`ip`] | IPv4 routing table, packet construction and parsing |
//! | N2.3 | [`icmp`] | ICMP echo/reply and unreachable generation |
//! | N2.4 | [`udp`] | UDP socket table and datagram delivery |
//! | N2.5 | [`tcp`] | TCP state machine (RFC 793 + Reno congestion control) |
//! | N2.6 | [`service`] | `NetworkService` main loop — frame ingress + timer |
//! | N3.2 | [`socket_api`] | Socket API dispatcher for userspace IPC |
//! | N5.1 | [`dns`] | DNS stub resolver (RFC 1035) with TTL cache |
//! | N6.1 | [`ifconfig`] | Network interface configuration IPC types |
//! | N6.2 | [`dhcp`] | DHCP v4 client state machine (RFC 2131) |
//!
//! ## Design principles
//!
//! - **`no_std + alloc`**: all code compiles without the standard library.
//!   Heap allocation is via `alloc::` types only.
//! - **No `unsafe`**: every public and private function is fully safe Rust.
//! - **Typed errors**: all fallible operations return `Result` with
//!   [`omni_types::socket::NetError`] or module-specific error types.
//! - **Big-endian wire format**: all multi-byte network fields are stored and
//!   transmitted in network (big-endian) byte order, following the convention
//!   established by [`omni_types::net`].
//!
//! ## Usage
//!
//! ```
//! use omni_net::service::NetworkService;
//! use omni_net::ip::{InterfaceConfig, Route};
//! use omni_types::net::{Cidr, Ipv4Addr, MacAddress};
//!
//! let mut svc = NetworkService::new();
//! svc.add_interface(InterfaceConfig {
//!     name: "eth0".into(),
//!     ip: Ipv4Addr([10, 0, 0, 1]),
//!     netmask: Cidr::new(Ipv4Addr([10, 0, 0, 0]), 8).unwrap(),
//!     mac: MacAddress([0x02, 0, 0, 0, 0, 1]),
//!     mtu: 1500,
//! });
//! // Tick at time 0 ms.
//! let outputs = svc.tick(0);
//! assert!(outputs.is_empty());
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Technical terms like IPv4, IPv6, RFC are industry-standard abbreviations and
// do not need to be wrapped in backticks in prose documentation.
#![allow(clippy::doc_markdown)]

extern crate alloc;

pub mod arp;
pub mod dhcp;
pub mod dns;
pub mod icmp;
pub mod ifconfig;
pub mod ip;
pub mod service;
pub mod socket_api;
pub mod tcp;
pub mod udp;
