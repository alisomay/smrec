mod parse;

const CHANNEL_MASK: u8 = 0b0000_1111;
const ANY_CHANNEL_INTERNAL: u8 = 0xFF;

use crate::types::Action;
use anyhow::{bail, Result};
use midir::{
    MidiInput, MidiInputConnection, MidiInputPort, MidiOutput, MidiOutputConnection, MidiOutputPort,
};
use std::{
    collections::HashMap,
    ops::Deref,
    str::FromStr,
    sync::{Arc, Mutex},
};

enum MessageType {
    NoteOff,
    NoteOn,
    PolyphonicAfterTouch,
    ControlChange,
    ProgramChange,
    AfterTouch,
    PitchBendChange,
    Ignored,
}

const fn get_message_type(message: &[u8]) -> MessageType {
    match message[0] >> 4 {
        0x8 => MessageType::NoteOff,
        0x9 => MessageType::NoteOn,
        0xA => MessageType::PolyphonicAfterTouch,
        0xB => MessageType::ControlChange,
        0xC => MessageType::ProgramChange,
        0xD => MessageType::AfterTouch,
        0xE => MessageType::PitchBendChange,
        _ => MessageType::Ignored,
    }
}

const fn get_channel(message: &[u8]) -> u8 {
    message[0] & CHANNEL_MASK
}

const fn make_cc_message(channel: u8, cc_num: u8, value: u8) -> [u8; 3] {
    [0xB0 + channel, cc_num, value]
}

/// `HashMap` of port name to vector of (`channel_num`, `cc_num`[start], `cc_num`[stop])
#[derive(Debug, Clone)]
pub struct MidiConfig(HashMap<String, Vec<(u8, u8, u8)>>);

impl Deref for MidiConfig {
    type Target = HashMap<String, Vec<(u8, u8, u8)>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for MidiConfig {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse::parse_midi_config(s)
    }
}

#[allow(clippy::type_complexity)]
pub struct Midi {
    input: MidiInput,
    output: Option<MidiOutput>,
    input_config: MidiConfig,
    output_config: Option<MidiConfig>,
    sender_channel: crossbeam::channel::Sender<Action>,
    receiver_channel: crossbeam::channel::Receiver<Action>,
    input_connections: HashMap<String, MidiInputConnection<Vec<(u8, u8, u8)>>>,
    output_thread: Option<std::thread::JoinHandle<()>>,
}

impl Midi {
    fn find_input_ports(&self, pattern: &str) -> Result<Vec<(String, MidiInputPort)>> {
        let mut found = Vec::new();
        for port in self.input.ports() {
            let name = self.input.port_name(&port).unwrap();
            if glob_match::glob_match(pattern, &name) {
                found.push((name, port));
            }
        }
        if found.len() == 1 {
            println!("Started listening on MIDI input port: {:?}\n", found[0].0);
        }
        if found.len() > 1 {
            println!("Warning: Found more than one MIDI input port matching the pattern and listening on them.\nFound ports: {:?}", found.iter().map(|(name, _)| name).collect::<Vec<&String>>());
        }
        if found.is_empty() {
            bail!("No MIDI input port found matching the pattern.");
        }
        Ok(found)
    }

    fn find_output_ports(&self, pattern: &str) -> Result<Vec<(String, MidiOutputPort)>> {
        if let Some(ref output) = self.output {
            let mut found = Vec::new();
            for port in output.ports() {
                let name = output.port_name(&port).unwrap();
                if glob_match::glob_match(pattern, &name) {
                    found.push((name, port));
                }
            }
            if found.len() == 1 {
                println!(
                    "Notifications will be sent on MIDI output port: {:?}\n",
                    found[0].0
                );
            }
            if found.len() > 1 {
                println!("Warning: Found more than one MIDI output port matching the pattern and will send notifications to them.\nFound ports: {:?}", found.iter().map(|(name, _)| name).collect::<Vec<&String>>());
            }
            if found.is_empty() {
                bail!("No MIDI output port found matching the pattern.");
            }
            Ok(found)
        } else {
            bail!("No midi output configured.")
        }
    }

    pub fn new(
        sender_channel: crossbeam::channel::Sender<Action>,
        receiver_channel: crossbeam::channel::Receiver<Action>,
        cli_config: &[String],
    ) -> Result<Self> {
        let input = MidiInput::new("smrec")?;

        let input_config = if let Some(input_config) = cli_config.get(0) {
            MidiConfig::from_str(input_config)?
        } else {
            // Listen all ports and all channels by default.
            MidiConfig::from_str("[*[(*,16,17)]]")?
        };
        let output_config = if let Some(output_config) = cli_config.get(1) {
            Some(MidiConfig::from_str(output_config)?)
        } else {
            None
        };

        Ok(Self {
            input,
            output: if output_config.is_some() {
                Some(MidiOutput::new("smrec")?)
            } else {
                None
            },
            input_config,
            output_config,
            sender_channel,
            receiver_channel,
            input_connections: HashMap::new(),
            output_thread: None,
        })
    }

    // These are going to be addressed in a later refactor.
    #[allow(clippy::type_complexity)]
    fn input_ports_from_configs(&self) -> Result<Vec<(String, MidiInputPort, Vec<(u8, u8, u8)>)>> {
        self.input_config
            .iter()
            .filter_map(|(port_name, configs)| {
                let input_ports = self.find_input_ports(port_name).ok()?;
                Some(
                    input_ports
                        .into_iter()
                        .map(move |(name, port)| (name, port, configs.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .map(Ok)
            .collect::<Result<Vec<(String, MidiInputPort, Vec<(u8, u8, u8)>)>, anyhow::Error>>()
    }

    fn register_midi_input_hooks(&mut self) -> Result<()> {
        let input_ports = self.input_ports_from_configs()?;

        // Start listening for MIDI messages on all configured ports and channels.
        for (port_name, port, configs) in input_ports {
            let to_main_thread = self.sender_channel.clone();

            let input = MidiInput::new("smrec")?;
            self.input_connections.insert(
                port_name.clone(),
                input
                    .connect(
                        &port,
                        &port_name,
                        move |_stamp, message, configs| {
                            let channel = get_channel(message);
                            let message_type = get_message_type(message);
                            if matches!(message_type, MessageType::ControlChange) {
                                if let (Some(cc_number), Some(value)) =
                                    (message.get(1), message.get(2))
                                {
                                    let active_config = configs
                                        .iter()
                                        .filter(|(chn, start_cc_num, stop_cc_num)| {
                                            chn == &channel
                                                && (cc_number == start_cc_num
                                                    || cc_number == stop_cc_num)
                                        })
                                        .collect::<Vec<&(u8, u8, u8)>>();

                                    let any_channel_receive_configs = configs
                                        .iter()
                                        .filter(|(chn, start_cc_num, stop_cc_num)| {
                                            *chn == ANY_CHANNEL_INTERNAL
                                                && (cc_number == start_cc_num
                                                    || cc_number == stop_cc_num)
                                        })
                                        .collect::<Vec<&(u8, u8, u8)>>();

                                    // There can be only one channel and one message type so either the active config is empty or has one element.
                                    if !active_config.is_empty() {
                                        let (chn, start_cc_num, stop_cc_num) = active_config[0];

                                        if chn == &channel
                                            && cc_number == start_cc_num
                                            && *value == 127
                                        {
                                            to_main_thread.send(Action::Start).unwrap();
                                        }

                                        if chn == &channel
                                            && cc_number == stop_cc_num
                                            && *value == 127
                                        {
                                            to_main_thread.send(Action::Stop).unwrap();
                                        }
                                    }

                                    for (_, start_cc_num, stop_cc_num) in
                                        any_channel_receive_configs
                                    {
                                        if cc_number == start_cc_num && *value == 127 {
                                            to_main_thread.send(Action::Start).unwrap();
                                        }

                                        if cc_number == stop_cc_num && *value == 127 {
                                            to_main_thread.send(Action::Stop).unwrap();
                                        }
                                    }
                                } else {
                                    println!("Invalid CC message: {message:?}");
                                }
                            }
                        },
                        configs,
                    )
                    .expect("Could not bind to {port_name}"),
            );
        }

        Ok(())
    }

    // These are going to be addressed in a later refactor.
    #[allow(clippy::type_complexity)]
    fn output_connections_from_config(
        &self,
    ) -> Result<Option<Vec<(String, Arc<Mutex<MidiOutputConnection>>, Vec<(u8, u8, u8)>)>>> {
        if let Some(ref output_config) = self.output_config {
            let output_ports = output_config
                .iter()
                .filter_map(|(port_name, configs)| {
                    let output_ports = self.find_output_ports(port_name).ok()?;
                    Some(
                        output_ports
                            .into_iter()
                            .map(move |(name, port)| (name, port, configs.clone()))
                            .collect::<Vec<_>>(),
                    )
                })
                .flatten()
                .map(Ok)
                .collect::<Result<Vec<(String, MidiOutputPort, Vec<(u8, u8, u8)>)>, anyhow::Error>>(
                )?;

            return output_ports
                .iter()
                .map(|(port_name, port, configs)| {
                    let output = MidiOutput::new("smrec")?;
                    Ok(Some((
                        port_name.clone(),
                        Arc::new(Mutex::new(
                            output
                                .connect(port, port_name)
                                .expect("Could not bind to {port_name}"),
                        )),
                        configs.clone(),
                    )))
                })
                .collect::<Result<
                    Option<Vec<(String, Arc<Mutex<MidiOutputConnection>>, Vec<(u8, u8, u8)>)>>,
                    _,
                >>();
        }

        Ok(None)
    }

    fn spin_midi_output_thread_if_necessary(&mut self) -> Result<()> {
        let output_connections = self.output_connections_from_config()?;
        let receiver_channel = self.receiver_channel.clone();

        if let Some(output_connections) = output_connections {
            self.output_thread = Some(std::thread::spawn(move || {
                loop {
                    if let Ok(action) = receiver_channel.recv() {
                        match action {
                            Action::Start => {
                                for (port_name, connection, configs) in &output_connections {
                                    for (channel, start_cc_num, _) in configs {
                                        // Send to all channels if channel is 255.
                                        if *channel == ANY_CHANNEL_INTERNAL {
                                            for chn in 0..15 {
                                                if let Err(err) = connection
                                                    .lock()
                                                    .unwrap()
                                                    .send(&make_cc_message(chn, *start_cc_num, 127))
                                                {
                                                    println!(
                                                "Error sending CC message to {port_name}: {err} ",
                                            );
                                                }
                                            }
                                            continue;
                                        }

                                        if let Err(err) = connection
                                            .lock()
                                            .unwrap()
                                            .send(&make_cc_message(*channel, *start_cc_num, 127))
                                        {
                                            println!(
                                                "Error sending CC message to {port_name}: {err} ",
                                            );
                                        }
                                    }
                                }
                            }
                            Action::Stop => {
                                for (port_name, connection, configs) in &output_connections {
                                    for (channel, _, stop_cc_num) in configs {
                                        // Send to all channels if channel is 255.
                                        if *channel == ANY_CHANNEL_INTERNAL {
                                            for chn in 0..15 {
                                                if let Err(err) = connection
                                                    .lock()
                                                    .unwrap()
                                                    .send(&make_cc_message(chn, *stop_cc_num, 127))
                                                {
                                                    println!(
                                                "Error sending CC message to {port_name}: {err} ",
                                            );
                                                }
                                            }
                                            continue;
                                        }

                                        if let Err(err) = connection
                                            .lock()
                                            .unwrap()
                                            .send(&make_cc_message(*channel, *stop_cc_num, 127))
                                        {
                                            println!(
                                                "Error sending CC message to {port_name}: {err} ",
                                            );
                                        }
                                    }
                                }
                            }
                            Action::Err(_) => {
                                // Ignore, we don't send midi messages when errors occur.
                            }
                        }
                    }
                }
            }));
        }

        Ok(())
    }

    pub fn listen(&mut self) -> Result<()> {
        self.register_midi_input_hooks()?;
        self.spin_midi_output_thread_if_necessary()?;

        Ok(())
    }
}
