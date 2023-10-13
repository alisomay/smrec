use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use midir::{Ignore, MidiInput, MidiOutput};

pub fn enumerate_audio() -> Result<()> {
    println!("Audio Hosts and Devices");
    println!("=======================");
    println!("  Supported hosts:\n    {:?}", cpal::ALL_HOSTS);
    let available_hosts = cpal::available_hosts();
    println!("  Available hosts:\n    {available_hosts:?}");

    for host_id in available_hosts {
        println!();
        println!("  {} Default Devices:", host_id.name());
        let host = cpal::host_from_id(host_id)?;

        host.default_input_device().map_or_else(
            || {
                println!("    Default Input Device:\n        None");
            },
            |d| {
                println!("    Default Input Device:\n        {}", d.name().unwrap());
            },
        );
        host.default_output_device().map_or_else(
            || {
                println!("    Default Output Device:\n        None");
            },
            |d| {
                println!("    Default Output Device:\n        {}", d.name().unwrap());
            },
        );

        let devices = host.devices()?;
        println!();
        println!("  {} Available Devices:", host_id.name());
        for (device_index, device) in devices.enumerate() {
            println!("    {}. \"{}\"", device_index + 1, device.name()?);

            // Input configs
            if let Ok(conf) = device.default_input_config() {
                // println!("      Default input stream config:\n      {:?}", conf);
                //   SupportedStreamConfig { channels: 16, sample_rate: SampleRate(44100), buffer_size: Range { min: 14, max: 4096 }, sample_format: F32 }
                println!("          Default input stream config:");
                println!(
                    "            Channels: {}\n            Sample Rate: {}\n            Buffer Size {}\n            Sample Format: {}",
                    conf.channels(),
                    conf.sample_rate().0,
                    match conf.buffer_size() {
                        cpal::SupportedBufferSize::Unknown => "unknown".to_string(),
                        cpal::SupportedBufferSize::Range { min, max } =>
                            format!("{{ min: {min}, max: {max} }}"),
                    },
                    conf.sample_format()
                );
            }
            let input_configs = match device.supported_input_configs() {
                Ok(f) => f.collect(),
                Err(err) => {
                    println!("          Error getting supported input configs: {err}");
                    Vec::new()
                }
            };
            if !input_configs.is_empty() {
                // TODO: If necessary list all supported stream configs
            }

            // Output configs
            if let Ok(conf) = device.default_output_config() {
                println!("          Default output stream config:");
                println!(
                    "            Channels: {}\n            Sample Rate: {}\n            Buffer Size {}\n            Sample Format: {}",
                    conf.channels(),
                    conf.sample_rate().0,
                    match conf.buffer_size() {
                        cpal::SupportedBufferSize::Unknown => "unknown".to_string(),
                        cpal::SupportedBufferSize::Range { min, max } =>
                            format!("{{ min: {min}, max: {max} }}"),
                    },
                    conf.sample_format()
                );
            }
            let output_configs = match device.supported_output_configs() {
                Ok(f) => f.collect(),
                Err(err) => {
                    println!("          Error getting supported output configs: {err}");
                    Vec::new()
                }
            };
            if !output_configs.is_empty() {
                // TODO: If necessary list all supported stream configs
            }
        }
    }

    Ok(())
}

pub fn enumerate_midi() -> Result<()> {
    let mut midi_in = MidiInput::new("dummy input")?;
    midi_in.ignore(Ignore::None);
    let midi_out = MidiOutput::new("dummy output")?;

    println!("Midi Ports");
    println!("==========");

    println!("  Available input ports:");
    for (i, p) in midi_in.ports().iter().enumerate() {
        println!("    {}: {}", i, midi_in.port_name(p)?);
    }

    println!("  Available output ports:");
    for (i, p) in midi_out.ports().iter().enumerate() {
        println!("    {}: {}", i, midi_out.port_name(p)?);
    }

    Ok(())
}
