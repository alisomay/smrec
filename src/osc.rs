use crate::types::Action;
use anyhow::Result;
use rosc::encoder::encode;
use rosc::{OscMessage, OscPacket, OscType};
use std::net::{SocketAddr, UdpSocket};
use std::str::FromStr;
use std::sync::Arc;

pub struct Osc {
    sender_socket: Arc<UdpSocket>,
    receiver_socket: Arc<UdpSocket>,
    sender_channel: crossbeam::channel::Sender<Action>,
    receiver_channel: crossbeam::channel::Receiver<Action>,
    udp_thread: Option<std::thread::JoinHandle<()>>,
    messaging_thread: Option<std::thread::JoinHandle<()>>,
}

impl Osc {
    pub fn new(
        osc_config: Vec<String>,
        sender_channel: crossbeam::channel::Sender<Action>,
        receiver_channel: crossbeam::channel::Receiver<Action>,
    ) -> Result<Self> {
        let recv_addr = if let Some(addr) = osc_config.get(0) {
            SocketAddr::from_str(addr)?
        } else {
            // Listen to all network and a random port by default.
            SocketAddr::from(([0, 0, 0, 0], 0))
        };

        let send_addr = if let Some(addr) = osc_config.get(1) {
            SocketAddr::from_str(addr)?
        } else {
            SocketAddr::from(([127, 0, 0, 1], 0))
        };

        let sender_socket = Arc::new(
            UdpSocket::bind(send_addr).expect("Failed to bind socket to address {send_addr}"),
        );

        let receiver_socket = Arc::new(
            UdpSocket::bind(recv_addr).expect("Failed to bind socket to address {recv_addr}"),
        );

        println!(
            "Will be sending OSC messages to {}",
            sender_socket.local_addr()?
        );
        println!(
            "Listening for OSC messages on {}",
            receiver_socket.local_addr()?
        );

        Ok(Self {
            sender_socket,
            receiver_socket,
            sender_channel,
            receiver_channel,
            udp_thread: None,
            messaging_thread: None,
        })
    }

    pub fn listen(&mut self) {
        if self.messaging_thread.is_none() {
            let socket = self.sender_socket.clone();
            let receiver_channel = self.receiver_channel.clone();
            self.messaging_thread = Some(std::thread::spawn(move || loop {
                match receiver_channel.recv() {
                    Ok(Action::Start) => {
                        if let Err(err) = socket.send(
                            &encode(&OscPacket::Message(OscMessage {
                                addr: "/smrec/start".to_string(),
                                args: Vec::new(),
                            }))
                            .expect("OSC packet should encode."),
                        ) {
                            println!("Error sending OSC packet: {}", err);
                        };
                    }
                    Ok(Action::Stop) => {
                        if let Err(err) = socket.send(
                            &encode(&OscPacket::Message(OscMessage {
                                addr: "/smrec/stop".to_string(),
                                args: Vec::new(),
                            }))
                            .expect("OSC packet should encode."),
                        ) {
                            println!("Error sending OSC packet: {}", err);
                        };
                    }
                    Ok(Action::Err(err)) => {
                        if let Err(err) = socket.send(
                            &encode(&OscPacket::Message(OscMessage {
                                addr: "/smrec/error".to_string(),
                                args: vec![OscType::String(err)],
                            }))
                            .expect("OSC packet should encode."),
                        ) {
                            println!("Error sending OSC packet: {}", err);
                        };
                    }
                    Err(e) => {
                        println!("Error receiving from channel: {}", e);
                    }
                }
            }));
        }

        if self.udp_thread.is_none() {
            let socket = self.receiver_socket.clone();
            let sender_channel = self.sender_channel.clone();
            self.udp_thread = Some(std::thread::spawn(move || {
                let mut buf = [0u8; rosc::decoder::MTU];

                loop {
                    match socket.recv_from(&mut buf) {
                        Ok((size, _addr)) => match rosc::decoder::decode_udp(&buf[..size]) {
                            Ok((_, osc_packet)) => {
                                handle_packet(&osc_packet, sender_channel.clone());
                            }
                            Err(e) => {
                                println!("Error decoding UDP packet: {}", e);
                            }
                        },
                        Err(e) => {
                            println!("Error receiving from socket: {}", e);
                        }
                    }
                }
            }));
        }
    }
}

fn handle_packet(packet: &OscPacket, channel: crossbeam::channel::Sender<Action>) {
    match packet {
        OscPacket::Message(message) => {
            handle_message(message, channel);
        }
        OscPacket::Bundle(bundle) => {
            bundle
                .content
                .iter()
                .for_each(|packet| handle_packet(packet, channel.clone()));
        }
    }
}

fn handle_message(message: &OscMessage, channel: crossbeam::channel::Sender<Action>) {
    match message.addr.as_str() {
        "/smrec/start" => {
            channel.send(Action::Start).unwrap();
        }
        "/smrec/stop" => {
            channel.send(Action::Stop).unwrap();
        }
        _ => {
            // Ignore
        }
    }
}
