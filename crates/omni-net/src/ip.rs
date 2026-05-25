//! IP routing table and packet construction (N2.2).
//!
//! Implements:
//! - [`RoutingTable`] — longest-prefix-match IPv4 routing
//! - [`build_ipv4_packet`] — construct a complete IPv4 packet with checksum
//! - [`parse_ipv4_packet`] — parse and checksum-verify an IPv4 packet
//!
//! ## Routing algorithm
//!
//! `lookup` iterates routes sorted by prefix length descending (most specific
//! first) and returns the first route whose CIDR contains the destination.
//! The sort is performed lazily on each lookup; for a typical table of ≤ 64
//! routes this is O(n log n) and acceptable.  A future optimisation could
//! maintain the sorted invariant on `add_route`.
//!
//! ## Default route
//!
//! A default route is expressed as `Cidr { addr: 0.0.0.0, prefix_len: 0 }`.
//! Because it has the lowest possible prefix length, it is always tried last.

use alloc::string::String;
use alloc::vec::Vec;

use omni_types::net::{Cidr, IpProtocol, Ipv4Addr, Ipv4Header, MacAddress};

// =============================================================================
// Types
// =============================================================================

/// A single routing table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// CIDR network this route covers.
    pub destination: Cidr,
    /// Next-hop gateway IP, or `None` if the destination is directly connected.
    pub gateway: Option<Ipv4Addr>,
    /// Outbound interface name (e.g., `"eth0"`).
    pub interface: String,
    /// Route priority; lower values win when multiple routes have the same
    /// prefix length.
    pub metric: u32,
}

/// Per-interface configuration used by the network service.
#[derive(Debug, Clone)]
pub struct InterfaceConfig {
    /// Interface name (e.g., `"eth0"`).
    pub name: String,
    /// Assigned IPv4 address.
    pub ip: Ipv4Addr,
    /// Network CIDR (address + prefix length).
    pub netmask: Cidr,
    /// Hardware (MAC) address.
    pub mac: MacAddress,
    /// Maximum transmission unit in bytes (payload only, excluding Ethernet
    /// header).
    pub mtu: u16,
}

/// IPv4 routing table with longest-prefix-match lookup.
///
/// # Examples
///
/// ```
/// use omni_net::ip::{RoutingTable, Route};
/// use omni_types::net::{Cidr, Ipv4Addr};
///
/// let mut rt = RoutingTable::new();
/// rt.add_route(Route {
///     destination: Cidr::new(Ipv4Addr([192, 168, 1, 0]), 24).unwrap(),
///     gateway: None,
///     interface: "eth0".into(),
///     metric: 0,
/// });
/// rt.add_route(Route {
///     destination: Cidr::new(Ipv4Addr([0, 0, 0, 0]), 0).unwrap(),
///     gateway: Some(Ipv4Addr([192, 168, 1, 1])),
///     interface: "eth0".into(),
///     metric: 100,
/// });
/// // Specific route wins.
/// let r = rt.lookup(Ipv4Addr([192, 168, 1, 50])).unwrap();
/// assert_eq!(r.gateway, None);
/// // Default route catches everything else.
/// let r = rt.lookup(Ipv4Addr([8, 8, 8, 8])).unwrap();
/// assert_eq!(r.gateway, Some(Ipv4Addr([192, 168, 1, 1])));
/// ```
#[derive(Debug, Default, Clone)]
pub struct RoutingTable {
    /// Unsorted route list; sorted by prefix_len descending on each lookup.
    routes: Vec<Route>,
}

impl RoutingTable {
    /// Construct an empty routing table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a route.  Duplicate destinations are allowed; the most specific
    /// and lowest-metric entry is preferred by [`Self::lookup`].
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
    }

    /// Remove all routes matching `destination` exactly.
    ///
    /// Returns `true` if at least one route was removed.
    pub fn remove_route(&mut self, destination: Cidr) -> bool {
        let before = self.routes.len();
        self.routes.retain(|r| r.destination != destination);
        self.routes.len() < before
    }

    /// Find the best route for `dst` using longest-prefix match.
    ///
    /// Among routes with the same prefix length, the one with the smallest
    /// `metric` wins.  Returns `None` only if the table contains no routes at
    /// all (not even a default route).
    #[must_use]
    pub fn lookup(&self, dst: Ipv4Addr) -> Option<&Route> {
        // Sort by (prefix_len DESC, metric ASC) to find the best route.
        // We collect indices rather than sorting the routes Vec in-place to
        // avoid mutating self through a shared reference.
        let mut candidates: Vec<usize> = (0..self.routes.len())
            .filter(|&i| {
                self.routes
                    .get(i)
                    .is_some_and(|r| r.destination.contains(dst))
            })
            .collect();

        candidates.sort_unstable_by(|&a, &b| {
            let ra = self.routes.get(a);
            let rb = self.routes.get(b);
            match (ra, rb) {
                (Some(ra), Some(rb)) => rb
                    .destination
                    .prefix_len
                    .cmp(&ra.destination.prefix_len)
                    .then(ra.metric.cmp(&rb.metric)),
                _ => core::cmp::Ordering::Equal,
            }
        });

        candidates.first().and_then(|&i| self.routes.get(i))
    }

    /// Return the full list of routes in insertion order.
    #[must_use]
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }
}

// =============================================================================
// Packet construction and parsing
// =============================================================================

/// Build a complete IPv4 packet (header + payload).
///
/// Sets the Don't-Fragment bit, computes the header checksum, and appends
/// `payload`.  Returns the packet as a contiguous byte vector.
///
/// # Arguments
///
/// * `src` — source IPv4 address
/// * `dst` — destination IPv4 address
/// * `protocol` — transport-layer protocol
/// * `ttl` — time to live (64 is a sensible default)
/// * `identification` — fragmentation identification field
/// * `payload` — transport-layer data
///
/// # Examples
///
/// ```
/// use omni_net::ip::build_ipv4_packet;
/// use omni_types::net::{Ipv4Addr, IpProtocol, Ipv4Header};
///
/// let pkt = build_ipv4_packet(
///     Ipv4Addr::LOOPBACK,
///     Ipv4Addr::LOOPBACK,
///     IpProtocol::UDP,
///     64,
///     1,
///     b"hello",
/// );
/// assert!(pkt.len() >= Ipv4Header::HEADER_LEN_MIN);
/// let (hdr, payload) = omni_net::ip::parse_ipv4_packet(&pkt).unwrap();
/// assert!(hdr.verify_checksum());
/// assert_eq!(payload, b"hello");
/// ```
#[must_use]
pub fn build_ipv4_packet(
    src: Ipv4Addr,
    dst: Ipv4Addr,
    protocol: IpProtocol,
    ttl: u8,
    identification: u16,
    payload: &[u8],
) -> Vec<u8> {
    let total_length =
        u16::try_from(Ipv4Header::HEADER_LEN_MIN + payload.len()).unwrap_or(u16::MAX);
    let mut hdr = Ipv4Header {
        version_ihl: 0x45, // version=4, IHL=5 (20 bytes, no options)
        dscp_ecn: 0,
        total_length,
        identification,
        flags_fragment: 0x4000, // Don't Fragment
        ttl,
        protocol,
        header_checksum: 0,
        src,
        dst,
    };
    hdr.header_checksum = hdr.compute_checksum();

    let mut out = Vec::with_capacity(Ipv4Header::HEADER_LEN_MIN + payload.len());
    // Extend with the serialized header bytes.
    let mut hdr_buf = [0u8; Ipv4Header::HEADER_LEN_MIN];
    // serialize returns None only if the buffer is too small, which it is not.
    let _ = hdr.serialize(&mut hdr_buf);
    out.extend_from_slice(&hdr_buf);
    out.extend_from_slice(payload);
    out
}

/// Parse an IPv4 packet and verify its header checksum.
///
/// Returns `(header, payload)` on success, `None` on malformed input or
/// checksum failure.
///
/// # Examples
///
/// ```
/// use omni_net::ip::{build_ipv4_packet, parse_ipv4_packet};
/// use omni_types::net::{Ipv4Addr, IpProtocol};
///
/// let pkt = build_ipv4_packet(Ipv4Addr::LOOPBACK, Ipv4Addr::LOOPBACK,
///                             IpProtocol::TCP, 64, 0, &[1, 2, 3]);
/// let (hdr, payload) = parse_ipv4_packet(&pkt).unwrap();
/// assert_eq!(payload, &[1, 2, 3]);
/// assert_eq!(hdr.protocol, IpProtocol::TCP);
/// ```
#[must_use]
pub fn parse_ipv4_packet(data: &[u8]) -> Option<(Ipv4Header, &[u8])> {
    let (hdr, payload) = Ipv4Header::parse(data)?;
    if !hdr.verify_checksum() {
        return None;
    }
    Some((hdr, payload))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        clippy::integer_division,
        clippy::map_unwrap_or,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::cognitive_complexity,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::used_underscore_binding
    )]
    #[allow(clippy::wildcard_imports)]
    use super::*;

    fn make_route(prefix: [u8; 4], len: u8, gw: Option<[u8; 4]>, metric: u32) -> Route {
        Route {
            destination: Cidr::new(Ipv4Addr(prefix), len).unwrap(),
            gateway: gw.map(Ipv4Addr),
            interface: "eth0".into(),
            metric,
        }
    }

    // -------------------------------------------------------------------------
    // RoutingTable tests
    // -------------------------------------------------------------------------

    #[test]
    fn new_table_is_empty() {
        let rt = RoutingTable::new();
        assert!(rt.routes().is_empty());
    }

    #[test]
    fn lookup_empty_table_returns_none() {
        let rt = RoutingTable::new();
        assert!(rt.lookup(Ipv4Addr([8, 8, 8, 8])).is_none());
    }

    #[test]
    fn lookup_default_route() {
        let mut rt = RoutingTable::new();
        rt.add_route(make_route([0, 0, 0, 0], 0, Some([192, 168, 1, 1]), 100));
        let r = rt.lookup(Ipv4Addr([8, 8, 8, 8])).unwrap();
        assert_eq!(r.gateway, Some(Ipv4Addr([192, 168, 1, 1])));
    }

    #[test]
    fn lookup_direct_route_preferred_over_default() {
        let mut rt = RoutingTable::new();
        rt.add_route(make_route([0, 0, 0, 0], 0, Some([192, 168, 1, 1]), 100));
        rt.add_route(make_route([192, 168, 1, 0], 24, None, 0));
        let r = rt.lookup(Ipv4Addr([192, 168, 1, 50])).unwrap();
        assert_eq!(r.gateway, None);
    }

    #[test]
    fn longest_prefix_match() {
        let mut rt = RoutingTable::new();
        rt.add_route(make_route([10, 0, 0, 0], 8, Some([192, 168, 0, 1]), 0));
        rt.add_route(make_route([10, 10, 0, 0], 16, Some([192, 168, 0, 2]), 0));
        rt.add_route(make_route([10, 10, 10, 0], 24, None, 0));
        // Most specific match (/24).
        let r = rt.lookup(Ipv4Addr([10, 10, 10, 5])).unwrap();
        assert!(r.gateway.is_none());
        // Next most specific (/16).
        let r = rt.lookup(Ipv4Addr([10, 10, 20, 1])).unwrap();
        assert_eq!(r.gateway, Some(Ipv4Addr([192, 168, 0, 2])));
        // Broad match (/8).
        let r = rt.lookup(Ipv4Addr([10, 99, 0, 1])).unwrap();
        assert_eq!(r.gateway, Some(Ipv4Addr([192, 168, 0, 1])));
    }

    #[test]
    fn lower_metric_wins_for_same_prefix() {
        let mut rt = RoutingTable::new();
        rt.add_route(Route {
            destination: Cidr::new(Ipv4Addr([10, 0, 0, 0]), 8).unwrap(),
            gateway: Some(Ipv4Addr([192, 168, 1, 1])),
            interface: "eth0".into(),
            metric: 100,
        });
        rt.add_route(Route {
            destination: Cidr::new(Ipv4Addr([10, 0, 0, 0]), 8).unwrap(),
            gateway: Some(Ipv4Addr([192, 168, 1, 2])),
            interface: "eth1".into(),
            metric: 10,
        });
        let r = rt.lookup(Ipv4Addr([10, 5, 5, 5])).unwrap();
        assert_eq!(r.gateway, Some(Ipv4Addr([192, 168, 1, 2])));
    }

    #[test]
    fn remove_route_returns_true_when_found() {
        let mut rt = RoutingTable::new();
        let cidr = Cidr::new(Ipv4Addr([10, 0, 0, 0]), 8).unwrap();
        rt.add_route(make_route([10, 0, 0, 0], 8, None, 0));
        assert!(rt.remove_route(cidr));
        assert!(rt.routes().is_empty());
    }

    #[test]
    fn remove_route_returns_false_when_absent() {
        let mut rt = RoutingTable::new();
        let cidr = Cidr::new(Ipv4Addr([10, 0, 0, 0]), 8).unwrap();
        assert!(!rt.remove_route(cidr));
    }

    #[test]
    fn routes_slice_is_in_insertion_order() {
        let mut rt = RoutingTable::new();
        rt.add_route(make_route([10, 0, 0, 0], 8, None, 0));
        rt.add_route(make_route([172, 16, 0, 0], 12, None, 0));
        let r = rt.routes();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].destination.prefix_len, 8);
        assert_eq!(r[1].destination.prefix_len, 12);
    }

    #[test]
    fn lookup_no_match_returns_none() {
        let mut rt = RoutingTable::new();
        rt.add_route(make_route([192, 168, 1, 0], 24, None, 0));
        assert!(rt.lookup(Ipv4Addr([10, 0, 0, 1])).is_none());
    }

    // -------------------------------------------------------------------------
    // build_ipv4_packet / parse_ipv4_packet tests
    // -------------------------------------------------------------------------

    #[test]
    fn build_ipv4_packet_correct_checksum() {
        let pkt = build_ipv4_packet(
            Ipv4Addr([1, 2, 3, 4]),
            Ipv4Addr([5, 6, 7, 8]),
            IpProtocol::UDP,
            64,
            0,
            &[],
        );
        let (hdr, _) = Ipv4Header::parse(&pkt).unwrap();
        assert!(hdr.verify_checksum());
    }

    #[test]
    fn build_ipv4_packet_ttl_and_protocol() {
        let pkt = build_ipv4_packet(
            Ipv4Addr::LOOPBACK,
            Ipv4Addr::LOOPBACK,
            IpProtocol::TCP,
            128,
            42,
            &[0xAB, 0xCD],
        );
        let (hdr, payload) = parse_ipv4_packet(&pkt).unwrap();
        assert_eq!(hdr.ttl, 128);
        assert_eq!(hdr.protocol, IpProtocol::TCP);
        assert_eq!(hdr.identification, 42);
        assert_eq!(payload, &[0xAB, 0xCD]);
    }

    #[test]
    fn build_ipv4_packet_sets_df_bit() {
        let pkt = build_ipv4_packet(
            Ipv4Addr::LOOPBACK,
            Ipv4Addr::LOOPBACK,
            IpProtocol::UDP,
            64,
            0,
            &[],
        );
        let (hdr, _) = parse_ipv4_packet(&pkt).unwrap();
        // DF bit is bit 14 of flags_fragment field (0x4000).
        assert_eq!(hdr.flags_fragment & 0x4000, 0x4000);
    }

    #[test]
    fn parse_ipv4_packet_rejects_bad_checksum() {
        let mut pkt = build_ipv4_packet(
            Ipv4Addr::LOOPBACK,
            Ipv4Addr::LOOPBACK,
            IpProtocol::UDP,
            64,
            0,
            &[],
        );
        // Corrupt the checksum field (bytes 10-11).
        if let Some(b) = pkt.get_mut(10) {
            *b ^= 0xFF;
        }
        assert!(parse_ipv4_packet(&pkt).is_none());
    }

    #[test]
    fn parse_ipv4_packet_rejects_truncated_input() {
        let pkt = build_ipv4_packet(
            Ipv4Addr::LOOPBACK,
            Ipv4Addr::LOOPBACK,
            IpProtocol::ICMP,
            64,
            0,
            &[1, 2, 3, 4],
        );
        assert!(parse_ipv4_packet(&pkt[..10]).is_none());
    }

    #[test]
    fn build_ipv4_packet_total_length_field() {
        let payload = &[0u8; 20];
        let pkt = build_ipv4_packet(
            Ipv4Addr::LOOPBACK,
            Ipv4Addr::LOOPBACK,
            IpProtocol::UDP,
            64,
            0,
            payload,
        );
        let (hdr, _) = parse_ipv4_packet(&pkt).unwrap();
        assert_eq!(
            hdr.total_length as usize,
            Ipv4Header::HEADER_LEN_MIN + payload.len()
        );
    }

    #[test]
    fn build_and_parse_roundtrip_src_dst() {
        let src = Ipv4Addr([10, 0, 0, 1]);
        let dst = Ipv4Addr([10, 0, 0, 2]);
        let pkt = build_ipv4_packet(src, dst, IpProtocol::ICMP, 64, 0, &[]);
        let (hdr, _) = parse_ipv4_packet(&pkt).unwrap();
        assert_eq!(hdr.src, src);
        assert_eq!(hdr.dst, dst);
    }

    #[test]
    fn lookup_host_route_prefix32() {
        let mut rt = RoutingTable::new();
        rt.add_route(make_route([10, 0, 0, 1], 32, None, 0));
        rt.add_route(make_route([10, 0, 0, 0], 8, Some([192, 168, 1, 1]), 0));
        // Exact host match should win.
        let r = rt.lookup(Ipv4Addr([10, 0, 0, 1])).unwrap();
        assert!(r.gateway.is_none());
    }

    #[test]
    fn lookup_multicast_via_route() {
        let mut rt = RoutingTable::new();
        rt.add_route(make_route([224, 0, 0, 0], 4, None, 0));
        let r = rt.lookup(Ipv4Addr([224, 0, 0, 251]));
        assert!(r.is_some());
    }
}
