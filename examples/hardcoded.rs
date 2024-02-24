use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    time::Duration,
};

use anyhow::Result;
use log::{debug, info};
use mdns_browser::{
    network_interface::get_or_select_ip_address,
    pack::Packable,
    packets::{packet::MDNSPacket, response::MDNSResponse, MDNSTYPE},
};

use bitvec::prelude::*;

const MDNS_PORT: u16 = 5353;
const MDNS_MULTICAST_IPV4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_MULTICAST_IPV6: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 0x00fb);
const MDNS_MULTICAST_SOCKETV4: SocketAddr =
    SocketAddr::new(IpAddr::V4(MDNS_MULTICAST_IPV4), MDNS_PORT);
const MDNS_MULTICAST_SOCKETV6: SocketAddr =
    SocketAddr::new(IpAddr::V6(MDNS_MULTICAST_IPV6), MDNS_PORT);

const QUERY_WRITE_TIMEOUT: Duration = Duration::from_secs(3);
const RESPONSE_READ_TIMEOUT: Duration = Duration::from_secs(3);

fn configured_mdns_socket(source: (u32, IpAddr)) -> Result<UdpSocket> {
    let socket = UdpSocket::bind((source.1, 0))?;

    socket.set_read_timeout(Some(RESPONSE_READ_TIMEOUT))?;
    socket.set_write_timeout(Some(QUERY_WRITE_TIMEOUT))?;

    match source.1 {
        IpAddr::V4(v4) => socket.join_multicast_v4(&MDNS_MULTICAST_IPV4, &v4),
        IpAddr::V6(_) => socket.join_multicast_v6(&MDNS_MULTICAST_IPV6, source.0),
    }
    .expect("Failed to join multicast group.");

    Ok(socket)
}

fn oneshot_mdns_query(source: (u32, IpAddr)) -> Result<()> {
    let is_ipv6 = source.1.is_ipv6();
    let socket = configured_mdns_socket(source).expect("Failed to configure mDNS socket.");
    let target_address: SocketAddr = if is_ipv6 {
        MDNS_MULTICAST_SOCKETV6
    } else {
        MDNS_MULTICAST_SOCKETV4
    };

    let packet = MDNSPacket::new("_ni._tcp.local", MDNSTYPE::PTR);

    debug!(
        "Sending mDNS query from {} to {}",
        socket.local_addr()?,
        target_address
    );

    // Send the packet.
    socket.send_to(&packet.pack().into_vec(), target_address)?;

    // // Receive the responses.
    let mut buf = [0; 1024];
    let mut responses = vec![];

    debug!("Waiting for responses...");

    while let Ok((num_bytes, _)) = socket.recv_from(&mut buf) {
        // Send back the response.
        socket.send_to(&buf[..num_bytes], target_address)?;

        let mut data = buf[..num_bytes].view_bits().to_bitvec();
        responses.push(MDNSResponse::unpack(&mut data).expect("Failed to unpack response."));
    }

    info!(
        "Received {} responses from {:#?}.",
        responses.len(),
        responses
            .iter()
            .map(|r| r
                .get_resource_record_of_type(MDNSTYPE::SRV)
                .unwrap()
                .rr_name
                .to_string())
            .collect::<Vec<_>>()
    );

    Ok(())
}

fn main() -> Result<()> {
    pretty_env_logger::init();

    let ip = get_or_select_ip_address().expect("Failed to get IP address!");
    oneshot_mdns_query(ip)?;

    Ok(())
}