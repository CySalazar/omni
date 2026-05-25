//! Network service main loop (N2.6).
//!
//! [`NetworkService`] is the top-level orchestrator that ties together the
//! sub-systems:
//!
//! - ARP resolution ([`crate::arp`])
//! - IP routing ([`crate::ip`])
//! - ICMP echo/unreachable ([`crate::icmp`])
//! - UDP socket table ([`crate::udp`])
//! - TCP socket table ([`crate::tcp`])
//! - DNS stub resolver ([`crate::dns`])
//!
//! ## Frame ingress
//!
//! [`NetworkService::handle_frame`] parses an Ethernet frame and dispatches:
//! - `EtherType::ARP` → ARP module (update table, possibly send reply)
//! - `EtherType::IPv4`:
//!   - `IpProtocol::ICMP` → ICMP module
//!   - `IpProtocol::UDP`  → UDP socket table
//!   - `IpProtocol::TCP`  → TCP socket table
//!
//! ## Socket API ingress
//!
//! [`NetworkService::handle_socket_request`] translates a [`SocketRequest`]
//! into the appropriate sub-system call and returns a [`SocketResponse`].
//!
//! ## Timer ticks
//!
//! [`NetworkService::tick`] should be called periodically (e.g., every 100 ms)
//! to drive ARP expiry, TCP retransmission timeouts, and TIME_WAIT cleanup.

use alloc::vec::Vec;

use omni_types::net::{
    ArpPacket, EtherType, EthernetHeader, IcmpHeader, IpProtocol, Ipv4Addr, MacAddress, TcpHeader,
    UdpHeader,
};
use omni_types::socket::{NetError, SocketApiAddr, SocketHandle, SocketRequest, SocketResponse};

use crate::arp::{ARP_TIMEOUT_SECS, ArpHandleResult, ArpResolveResult, ArpTable, PendingPacket};
use crate::dns::DnsResolver;
use crate::icmp::{IcmpHandleResult, IcmpHandler};
use crate::ip::{InterfaceConfig, RoutingTable, build_ipv4_packet};
use crate::tcp::{TcpOutput, TcpSocketTable};
use crate::udp::UdpSocketTable;

// =============================================================================
// ServiceOutput
// =============================================================================

/// Actions the service loop must perform after processing a frame or request.
#[derive(Debug)]
pub enum ServiceOutput {
    /// Transmit a raw Ethernet frame on `interface`.
    SendFrame {
        /// Index into [`NetworkService::interfaces`].
        interface: usize,
        /// Complete frame bytes (Ethernet header + payload).
        data: Vec<u8>,
    },
    /// Return a response to the userspace caller.
    SocketResponse(SocketResponse),
}

// =============================================================================
// NetworkService
// =============================================================================

/// The OMNI OS userspace TCP/IP network stack service.
///
/// # Examples
///
/// ```
/// use omni_net::service::NetworkService;
///
/// let mut svc = NetworkService::new();
/// // Tick at time 0 — nothing to do yet.
/// let out = svc.tick(0);
/// assert!(out.is_empty());
/// ```
pub struct NetworkService {
    /// Network interfaces registered with this service.
    pub interfaces: Vec<InterfaceConfig>,
    /// ARP resolution table.
    pub arp: ArpTable,
    /// IP routing table.
    pub routing: RoutingTable,
    /// ICMP handler.
    pub icmp: IcmpHandler,
    /// UDP socket table.
    pub udp: UdpSocketTable,
    /// TCP socket table.
    pub tcp: TcpSocketTable,
    /// DNS stub resolver.
    pub dns: DnsResolver,
    /// Next socket handle to allocate.
    next_handle: u64,
}

impl Default for NetworkService {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkService {
    /// Construct an empty [`NetworkService`] with no interfaces configured.
    #[must_use]
    pub fn new() -> Self {
        Self {
            interfaces: Vec::new(),
            arp: ArpTable::new(crate::arp::ARP_MAX_ENTRIES),
            routing: RoutingTable::new(),
            icmp: IcmpHandler::new(),
            udp: UdpSocketTable::new(),
            tcp: TcpSocketTable::new(),
            dns: DnsResolver::new(Vec::new()),
            next_handle: 1,
        }
    }

    /// Register a network interface.
    ///
    /// Automatically adds a connected route for the interface's subnet.
    pub fn add_interface(&mut self, config: InterfaceConfig) {
        // Add connected route for the interface subnet.
        use crate::ip::Route;
        self.routing.add_route(Route {
            destination: config.netmask,
            gateway: None,
            interface: config.name.clone(),
            metric: 0,
        });
        self.interfaces.push(config);
    }

    /// Process an incoming Ethernet frame received on `interface_idx`.
    ///
    /// Returns a list of [`ServiceOutput`] items — frames to send and socket
    /// responses to deliver.
    pub fn handle_frame(
        &mut self,
        interface_idx: usize,
        frame: &[u8],
        now: u64,
    ) -> Vec<ServiceOutput> {
        let mut out = Vec::new();

        let Some((eth_hdr, payload)) = EthernetHeader::parse(frame) else {
            return out;
        };

        let our_mac = self
            .interfaces
            .get(interface_idx)
            .map_or(MacAddress([0; 6]), |iface| iface.mac);
        let our_ip = self
            .interfaces
            .get(interface_idx)
            .map_or(Ipv4Addr::UNSPECIFIED, |iface| iface.ip);

        match eth_hdr.ether_type {
            EtherType::ARP => {
                if let Some(arp_pkt) = ArpPacket::parse(payload) {
                    self.handle_arp_packet(interface_idx, &arp_pkt, our_mac, our_ip, &mut out);
                }
            }
            EtherType::IPV4 => {
                let Some((ip_hdr, ip_payload)) = crate::ip::parse_ipv4_packet(payload) else {
                    return out;
                };
                match ip_hdr.protocol {
                    IpProtocol::ICMP => {
                        if let Some((icmp_hdr, icmp_payload)) = IcmpHeader::parse(ip_payload) {
                            let result = self.icmp.handle_icmp(
                                icmp_hdr,
                                icmp_payload,
                                ip_hdr.src,
                                our_ip,
                                now,
                            );
                            match result {
                                IcmpHandleResult::Reply(reply) => {
                                    // Wrap reply in IPv4 and Ethernet.
                                    let ip_pkt = build_ipv4_packet(
                                        our_ip,
                                        ip_hdr.src,
                                        IpProtocol::ICMP,
                                        64,
                                        0,
                                        &reply.data,
                                    );
                                    if let Some(frame_data) =
                                        self.wrap_in_ethernet(ip_hdr.src, our_mac, &ip_pkt)
                                    {
                                        out.push(ServiceOutput::SendFrame {
                                            interface: interface_idx,
                                            data: frame_data,
                                        });
                                    }
                                }
                                _ => {} // Other ICMP types handled by application layer.
                            }
                        }
                    }
                    IpProtocol::UDP => {
                        if let Some((udp_hdr, udp_payload)) = UdpHeader::parse(ip_payload) {
                            self.udp
                                .handle_packet(udp_hdr, udp_payload, ip_hdr.src, ip_hdr.dst);
                        }
                    }
                    IpProtocol::TCP => {
                        if let Some((tcp_hdr, tcp_payload)) = TcpHeader::parse(ip_payload) {
                            let tcp_outs = self.tcp.handle_segment(
                                &tcp_hdr,
                                tcp_payload,
                                ip_hdr.src,
                                ip_hdr.dst,
                                now,
                            );
                            self.emit_tcp_outputs(tcp_outs, interface_idx, our_mac, &mut out);
                        }
                    }
                    _ => {} // Unhandled protocol.
                }
            }
            _ => {} // Unhandled EtherType.
        }

        out
    }

    /// Process a [`SocketRequest`] from a userspace program.
    ///
    /// Returns the appropriate [`SocketResponse`].
    // This function dispatches many socket request variants; the length is
    // inherent to the design of a socket API dispatcher.
    #[allow(clippy::too_many_lines)]
    pub fn handle_socket_request(&mut self, request: SocketRequest) -> SocketResponse {
        match request {
            SocketRequest::Socket { .. } => {
                // Allocate an opaque handle; actual socket creation happens on Bind/Connect.
                let h = SocketHandle(self.next_handle);
                self.next_handle += 1;
                SocketResponse::Handle(h)
            }
            SocketRequest::Bind { addr, .. } => {
                let port = addr.port;
                match self.udp.bind(port) {
                    Ok(p) => SocketResponse::Ok(u64::from(p)),
                    Err(e) => SocketResponse::Error(e),
                }
            }
            SocketRequest::Listen { handle, backlog } => {
                // Determine the port from context (simplified: use handle.0 as port).
                let port = u16::try_from(handle.0).unwrap_or(0);
                match self.tcp.listen(port, backlog as usize) {
                    Ok(()) => SocketResponse::Ok(0),
                    Err(e) => SocketResponse::Error(e),
                }
            }
            SocketRequest::Accept { handle } => {
                let port = u16::try_from(handle.0).unwrap_or(0);
                match self.tcp.accept(port) {
                    Some(_key) => {
                        let h = SocketHandle(self.next_handle);
                        self.next_handle += 1;
                        SocketResponse::Handle(h)
                    }
                    None => SocketResponse::Error(NetError::WouldBlock),
                }
            }
            SocketRequest::Connect { handle, addr } => {
                let local = SocketApiAddr {
                    ip: [127, 0, 0, 1],
                    port: u16::try_from(handle.0).unwrap_or(0),
                };
                let mut tcp_out = Vec::new();
                match self.tcp.connect(local, addr, &mut tcp_out) {
                    Ok(_key) => SocketResponse::Ok(0),
                    Err(e) => SocketResponse::Error(e),
                }
            }
            SocketRequest::Send { handle, data, .. } => {
                let local = SocketApiAddr {
                    ip: [127, 0, 0, 1],
                    port: u16::try_from(handle.0).unwrap_or(0),
                };
                let remote = SocketApiAddr {
                    ip: [0, 0, 0, 0],
                    port: 0,
                };
                let key = (local, remote);
                match self.tcp.send(&key, &data) {
                    Ok(n) => SocketResponse::Ok(n as u64),
                    Err(e) => SocketResponse::Error(e),
                }
            }
            SocketRequest::Recv {
                handle, max_len, ..
            } => {
                let local = SocketApiAddr {
                    ip: [127, 0, 0, 1],
                    port: u16::try_from(handle.0).unwrap_or(0),
                };
                let remote = SocketApiAddr {
                    ip: [0, 0, 0, 0],
                    port: 0,
                };
                let key = (local, remote);
                let mut buf = alloc::vec![0u8; max_len as usize];
                match self.tcp.recv(&key, &mut buf) {
                    Ok(n) => {
                        buf.truncate(n);
                        SocketResponse::Data(buf)
                    }
                    Err(e) => SocketResponse::Error(e),
                }
            }
            SocketRequest::SendTo { handle, data, addr } => {
                let port = u16::try_from(handle.0).unwrap_or(0);
                let iface_ip = self
                    .interfaces
                    .first()
                    .map_or(Ipv4Addr::UNSPECIFIED, |i| i.ip);
                let dst_ip = Ipv4Addr(addr.ip);
                match self.udp.sendto(port, addr, iface_ip, dst_ip, &data) {
                    Ok(pkt) => SocketResponse::Ok(pkt.len() as u64),
                    Err(e) => SocketResponse::Error(e),
                }
            }
            SocketRequest::RecvFrom { handle, .. } => {
                let port = u16::try_from(handle.0).unwrap_or(0);
                match self.udp.recvfrom(port) {
                    Some((src, data)) => SocketResponse::DataFrom(data, src),
                    None => SocketResponse::Error(NetError::WouldBlock),
                }
            }
            SocketRequest::Close { handle } => {
                let port = u16::try_from(handle.0).unwrap_or(0);
                self.udp.close(port);
                SocketResponse::Ok(0)
            }
            SocketRequest::Resolve { hostname } => {
                let now_secs = 0u64; // No real clock; caller must inject time.
                self.dns.resolve_cached(&hostname, now_secs).map_or(
                    SocketResponse::Error(NetError::HostUnreachable),
                    |addrs| {
                        let api_addrs: Vec<SocketApiAddr> = addrs
                            .iter()
                            .map(|a| SocketApiAddr { ip: a.0, port: 0 })
                            .collect();
                        SocketResponse::Addresses(api_addrs)
                    },
                )
            }
            SocketRequest::ListSockets => {
                // Return an empty list for now; the full implementation would
                // iterate tcp/udp socket tables.
                SocketResponse::SocketList(alloc::vec![])
            }
            // Remaining variants — return Ok(0) or meaningful defaults.
            SocketRequest::GetSockName { .. }
            | SocketRequest::GetPeerName { .. }
            | SocketRequest::SetSockOpt { .. }
            | SocketRequest::Shutdown { .. } => SocketResponse::Ok(0),
            _ => SocketResponse::Error(NetError::InvalidArgument),
        }
    }

    /// Drive periodic timers: ARP expiry and TCP retransmit/TIME_WAIT.
    ///
    /// `now` is the current monotonic timestamp in milliseconds.
    pub fn tick(&mut self, now: u64) -> Vec<ServiceOutput> {
        let mut out = Vec::new();

        // ARP expiry: convert ms timestamp to seconds for the ARP table.
        // Integer division is intentional here (truncating milliseconds).
        #[allow(clippy::integer_division)]
        let now_secs = now / 1000;
        self.arp.expire_stale(now_secs, ARP_TIMEOUT_SECS);

        // TCP tick.
        let mut tcp_outs = Vec::new();
        self.tcp.tick(now, &mut tcp_outs);
        // Emit TCP outputs without interface binding (use interface 0).
        let our_mac = self
            .interfaces
            .first()
            .map_or(MacAddress([0; 6]), |i| i.mac);
        self.emit_tcp_outputs(tcp_outs, 0, our_mac, &mut out);

        out
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Process an ARP packet and queue any reply that needs to be sent.
    fn handle_arp_packet(
        &mut self,
        interface_idx: usize,
        pkt: &ArpPacket,
        our_mac: MacAddress,
        our_ip: Ipv4Addr,
        out: &mut Vec<ServiceOutput>,
    ) {
        let result = self.arp.handle_arp_packet(pkt, our_mac, our_ip);
        match result {
            ArpHandleResult::SendReply(reply) => {
                // Wrap ARP reply in Ethernet frame.
                let mut frame =
                    alloc::vec![0u8; EthernetHeader::HEADER_LEN + ArpPacket::PACKET_LEN];
                let eth = EthernetHeader {
                    dst: pkt.sender_mac,
                    src: our_mac,
                    ether_type: EtherType::ARP,
                };
                if let Some(eth_slot) = frame.get_mut(..EthernetHeader::HEADER_LEN) {
                    eth.serialize(eth_slot);
                }
                if let Some(arp_slot) = frame.get_mut(EthernetHeader::HEADER_LEN..) {
                    reply.serialize(arp_slot);
                }
                out.push(ServiceOutput::SendFrame {
                    interface: interface_idx,
                    data: frame,
                });
                // Also drain any pending packets that were waiting for this ARP.
                let pending = self.arp.drain_pending(pkt.sender_ip);
                for pending_pkt in pending {
                    // Re-emit as a SendFrame.
                    if let Some(frame) =
                        self.wrap_in_ethernet(pending_pkt.next_hop_ip, our_mac, &pending_pkt.data)
                    {
                        out.push(ServiceOutput::SendFrame {
                            interface: interface_idx,
                            data: frame,
                        });
                    }
                }
            }
            ArpHandleResult::UpdatedTable => {
                // Drain any packets that were waiting for this MAC.
                let sender_ip = pkt.sender_ip;
                let pending = self.arp.drain_pending(sender_ip);
                for pending_pkt in pending {
                    if let Some(frame) =
                        self.wrap_in_ethernet(pending_pkt.next_hop_ip, our_mac, &pending_pkt.data)
                    {
                        out.push(ServiceOutput::SendFrame {
                            interface: interface_idx,
                            data: frame,
                        });
                    }
                }
            }
            ArpHandleResult::Ignored => {}
        }
    }

    /// Wrap `ip_payload` in an Ethernet frame addressed to `dst_ip`.
    ///
    /// Returns `None` if the ARP resolution for `dst_ip` is pending (the
    /// packet has been queued internally).
    fn wrap_in_ethernet(
        &mut self,
        dst_ip: Ipv4Addr,
        our_mac: MacAddress,
        ip_payload: &[u8],
    ) -> Option<Vec<u8>> {
        let dst_mac = match self.arp.resolve(
            dst_ip,
            Some(PendingPacket {
                data: ip_payload.to_vec(),
                next_hop_ip: dst_ip,
            }),
        ) {
            ArpResolveResult::Resolved(mac) => mac,
            ArpResolveResult::Pending => return None,
        };

        let mut frame = alloc::vec![0u8; EthernetHeader::HEADER_LEN + ip_payload.len()];
        let eth = EthernetHeader {
            dst: dst_mac,
            src: our_mac,
            ether_type: EtherType::IPV4,
        };
        if let Some(eth_slot) = frame.get_mut(..EthernetHeader::HEADER_LEN) {
            eth.serialize(eth_slot);
        }
        if let Some(payload_slot) = frame.get_mut(EthernetHeader::HEADER_LEN..) {
            payload_slot.copy_from_slice(ip_payload);
        }
        Some(frame)
    }

    /// Convert `TcpOutput` items into `ServiceOutput` items.
    fn emit_tcp_outputs(
        &mut self,
        tcp_outs: Vec<TcpOutput>,
        interface_idx: usize,
        our_mac: MacAddress,
        out: &mut Vec<ServiceOutput>,
    ) {
        for tcp_out in tcp_outs {
            if let TcpOutput::SendSegment { data, dst_ip } = tcp_out {
                // Attempt Ethernet wrap; if ARP pending the packet is queued.
                if let Some(frame) = self.wrap_in_ethernet(dst_ip, our_mac, &data) {
                    out.push(ServiceOutput::SendFrame {
                        interface: interface_idx,
                        data: frame,
                    });
                }
            }
            // Other TcpOutput variants (ConnectionEstablished, DataReceived, etc.)
            // are notifications for the upper application layer; the service loop
            // would forward them to waiting userspace processes.  For now we
            // silently drop them here — they are handled in integration scenarios.
        }
    }
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
        clippy::used_underscore_binding,
        unused_imports
    )]
    #[allow(clippy::wildcard_imports)]
    use super::*;
    use crate::ip::{InterfaceConfig, Route};
    use omni_types::net::{Cidr, EtherType, EthernetHeader, Ipv4Addr, MacAddress};
    use omni_types::socket::{SocketDomain, SocketHandle, SocketRequest, SocketType};

    fn make_interface() -> InterfaceConfig {
        InterfaceConfig {
            name: "eth0".into(),
            ip: Ipv4Addr([192, 168, 1, 1]),
            netmask: Cidr::new(Ipv4Addr([192, 168, 1, 0]), 24).unwrap(),
            mac: MacAddress([0x02, 0, 0, 0, 0, 1]),
            mtu: 1500,
        }
    }

    fn make_service() -> NetworkService {
        let mut svc = NetworkService::new();
        svc.add_interface(make_interface());
        // Pre-populate ARP for a peer so wrap_in_ethernet succeeds.
        svc.arp.insert(
            Ipv4Addr([192, 168, 1, 10]),
            MacAddress([0x02, 0, 0, 0, 0, 2]),
            0,
        );
        svc
    }

    fn make_eth_frame(payload: &[u8], ether_type: EtherType) -> Vec<u8> {
        let mut frame = alloc::vec![0u8; EthernetHeader::HEADER_LEN + payload.len()];
        let eth = EthernetHeader {
            dst: MacAddress([0x02, 0, 0, 0, 0, 1]),
            src: MacAddress([0x02, 0, 0, 0, 0, 2]),
            ether_type,
        };
        eth.serialize(&mut frame[..EthernetHeader::HEADER_LEN]);
        if let Some(dst) = frame.get_mut(EthernetHeader::HEADER_LEN..) {
            dst.copy_from_slice(payload);
        }
        frame
    }

    // -------------------------------------------------------------------------
    // Basic service construction
    // -------------------------------------------------------------------------

    #[test]
    fn new_service_has_no_interfaces() {
        let svc = NetworkService::new();
        assert!(svc.interfaces.is_empty());
    }

    #[test]
    fn add_interface_registers_interface() {
        let mut svc = NetworkService::new();
        svc.add_interface(make_interface());
        assert_eq!(svc.interfaces.len(), 1);
    }

    #[test]
    fn add_interface_adds_connected_route() {
        let mut svc = NetworkService::new();
        svc.add_interface(make_interface());
        let route = svc.routing.lookup(Ipv4Addr([192, 168, 1, 50]));
        assert!(route.is_some());
    }

    // -------------------------------------------------------------------------
    // handle_frame: ARP
    // -------------------------------------------------------------------------

    #[test]
    fn handle_frame_arp_request_produces_reply() {
        use omni_types::net::{ArpOperation, ArpPacket};

        let mut svc = make_service();
        let arp_pkt = ArpPacket {
            htype: 1,
            ptype: 0x0800,
            hlen: 6,
            plen: 4,
            operation: ArpOperation::REQUEST,
            sender_mac: MacAddress([0x02, 0, 0, 0, 0, 2]),
            sender_ip: Ipv4Addr([192, 168, 1, 10]),
            target_mac: MacAddress([0; 6]),
            target_ip: Ipv4Addr([192, 168, 1, 1]),
        };
        let mut payload = alloc::vec![0u8; ArpPacket::PACKET_LEN];
        arp_pkt.serialize(&mut payload);
        let frame = make_eth_frame(&payload, EtherType::ARP);
        let out = svc.handle_frame(0, &frame, 0);
        assert!(
            out.iter()
                .any(|o| matches!(o, ServiceOutput::SendFrame { .. }))
        );
    }

    // -------------------------------------------------------------------------
    // handle_frame: ICMP echo
    // -------------------------------------------------------------------------

    #[test]
    fn handle_frame_icmp_echo_request_produces_reply() {
        use crate::icmp::IcmpHandler;
        use crate::ip::build_ipv4_packet;
        use omni_types::net::IpProtocol;

        let mut svc = make_service();
        let icmp_bytes = IcmpHandler::build_echo_request(1, 1, b"ping");
        let ip_pkt = build_ipv4_packet(
            Ipv4Addr([192, 168, 1, 10]),
            Ipv4Addr([192, 168, 1, 1]),
            IpProtocol::ICMP,
            64,
            0,
            &icmp_bytes,
        );
        let frame = make_eth_frame(&ip_pkt, EtherType::IPV4);
        let out = svc.handle_frame(0, &frame, 0);
        assert!(
            out.iter()
                .any(|o| matches!(o, ServiceOutput::SendFrame { .. }))
        );
    }

    // -------------------------------------------------------------------------
    // handle_frame: UDP delivery
    // -------------------------------------------------------------------------

    #[test]
    fn handle_frame_udp_delivers_to_socket() {
        use crate::ip::build_ipv4_packet;
        use crate::udp::build_udp_packet;
        use omni_types::net::IpProtocol;

        let mut svc = make_service();
        svc.udp.bind(5000).unwrap();

        let udp_bytes = build_udp_packet(
            Ipv4Addr([192, 168, 1, 10]),
            Ipv4Addr([192, 168, 1, 1]),
            40000,
            5000,
            b"hello",
        );
        let ip_pkt = build_ipv4_packet(
            Ipv4Addr([192, 168, 1, 10]),
            Ipv4Addr([192, 168, 1, 1]),
            IpProtocol::UDP,
            64,
            0,
            &udp_bytes,
        );
        let frame = make_eth_frame(&ip_pkt, EtherType::IPV4);
        let _ = svc.handle_frame(0, &frame, 0);
        let pkt = svc.udp.recvfrom(5000);
        assert!(pkt.is_some());
        assert_eq!(pkt.unwrap().1, b"hello");
    }

    // -------------------------------------------------------------------------
    // handle_socket_request
    // -------------------------------------------------------------------------

    #[test]
    fn socket_request_socket_returns_handle() {
        let mut svc = make_service();
        let req = SocketRequest::Socket {
            domain: SocketDomain::Inet,
            sock_type: SocketType::Stream,
        };
        let resp = svc.handle_socket_request(req);
        assert!(matches!(resp, SocketResponse::Handle(_)));
    }

    #[test]
    fn socket_request_bind_success() {
        let mut svc = make_service();
        let req = SocketRequest::Bind {
            handle: SocketHandle(0),
            addr: omni_types::socket::SocketApiAddr {
                ip: [0, 0, 0, 0],
                port: 7000,
            },
        };
        let resp = svc.handle_socket_request(req);
        assert!(matches!(resp, SocketResponse::Ok(_)));
    }

    #[test]
    fn socket_request_bind_duplicate_returns_error() {
        let mut svc = make_service();
        let bind = |svc: &mut NetworkService, port: u16| {
            svc.handle_socket_request(SocketRequest::Bind {
                handle: SocketHandle(0),
                addr: omni_types::socket::SocketApiAddr {
                    ip: [0, 0, 0, 0],
                    port,
                },
            })
        };
        assert!(matches!(bind(&mut svc, 8080), SocketResponse::Ok(_)));
        assert!(matches!(bind(&mut svc, 8080), SocketResponse::Error(_)));
    }

    #[test]
    fn socket_request_recv_from_empty_queue_returns_wouldblock() {
        let mut svc = make_service();
        svc.udp.bind(9000).unwrap();
        let req = SocketRequest::RecvFrom {
            handle: SocketHandle(9000),
            max_len: 512,
        };
        let resp = svc.handle_socket_request(req);
        assert!(matches!(resp, SocketResponse::Error(NetError::WouldBlock)));
    }

    #[test]
    fn socket_request_close_succeeds() {
        let mut svc = make_service();
        svc.udp.bind(6000).unwrap();
        let resp = svc.handle_socket_request(SocketRequest::Close {
            handle: SocketHandle(6000),
        });
        assert!(matches!(resp, SocketResponse::Ok(0)));
    }

    // -------------------------------------------------------------------------
    // tick
    // -------------------------------------------------------------------------

    #[test]
    fn tick_returns_empty_when_nothing_to_do() {
        let mut svc = make_service();
        let out = svc.tick(0);
        assert!(out.is_empty());
    }

    #[test]
    fn handle_frame_malformed_returns_empty() {
        let mut svc = make_service();
        let out = svc.handle_frame(0, &[0xFF, 0xFF], 0);
        assert!(out.is_empty());
    }
}
