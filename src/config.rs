use crate::wav::wav_spec_from_config;
use crate::WriterHandles;
use anyhow::{anyhow, bail, Result};
use camino::Utf8PathBuf;
use chrono::{Datelike, Timelike, Utc};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::SupportedStreamConfig;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// Chooses which channels to record.
pub fn choose_channels_to_record(
    include: Option<Vec<usize>>,
    exclude: Option<Vec<usize>>,
    config: &cpal::SupportedStreamConfig,
) -> Result<Vec<usize>> {
    match (include, exclude) {
        // Includes only the channels in the list.
        (Some(include), None) => Ok(include.iter().map(|i| i - 1).collect()),
        // Includes all channels but excludes the ones in the list.
        (None, Some(exclude)) => {
            let mut channels = (0..config.channels() as usize).collect::<Vec<_>>();
            let exclude = exclude.iter().map(|i| i - 1).collect::<Vec<_>>();

            for channel in exclude {
                if let Some(pos) = channels.iter().position(|i| *i == channel) {
                    channels.remove(pos);
                } else {
                    bail!(
                        "Channel {} is meant to be excluded but it does not exist.",
                        channel + 1
                    );
                }
            }

            Ok(channels)
        }
        (Some(_), Some(_)) => bail!("Using --exclude and --include together is not allowed."),
        // Includes all channels by default.
        (None, None) => Ok((0..config.channels() as usize).collect()),
    }
}

/// Chooses the host to use.
pub fn choose_host(host: Option<String>, asio: bool) -> Result<cpal::Host> {
    #[cfg(target_os = "windows")]
    if asio {
        return Ok(cpal::host_from_id(cpal::HostId::Asio).expect("Failed to initialise ASIO host."));
    }

    if let Some(chosen_host_name) = host {
        let available_hosts = cpal::available_hosts();
        let host_id = available_hosts
            .iter()
            .find(|host_id| host_id.name() == chosen_host_name);
        if let Some(host_id) = host_id {
            cpal::host_from_id(*host_id).map_err(|e| anyhow::anyhow!(e))
        } else {
            bail!("Provided host {chosen_host_name} was not found.")
        }
    } else {
        // Use the default host when not provided.
        Ok(cpal::default_host())
    }
}

/// Chooses the device to use.
pub fn choose_device(host: &cpal::Host, device: Option<String>) -> Result<cpal::Device> {
    if let Some(chosen_device_name) = device {
        let devices = host.devices()?;
        let device = devices
            .enumerate()
            .find(|(_device_index, device)| device.name().expect("Later") == chosen_device_name);
        if let Some((_, device)) = device {
            Ok(device)
        } else {
            bail!("Provided device {chosen_device_name} not found.")
        }
    } else {
        // Try to use the default device when not provided.
        host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default audio device found."))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct SmrecConfig {
    #[serde(deserialize_with = "deserialize_usize_keys_greater_than_0")]
    channel_names: HashMap<usize, String>,
    #[serde(skip)]
    channels_to_record: Vec<usize>,
    #[serde(skip)]
    out_path: Option<String>,
    #[serde(skip)]
    cpal_stream_config: Option<SupportedStreamConfig>,
}

impl SmrecConfig {
    pub fn new(
        config_path: Option<String>,
        out_path: Option<String>,
        channels_to_record: Vec<usize>,
        cpal_stream_config: SupportedStreamConfig,
    ) -> Result<Self> {
        let current_dir_config = Utf8PathBuf::from("./.smrec/config.toml");

        let path = if let Some(path) = config_path {
            Utf8PathBuf::from_str(&path)?
        } else if current_dir_config.exists() {
            current_dir_config
        } else {
            Utf8PathBuf::from_path_buf(
                home::home_dir().ok_or_else(|| anyhow!("User home directory was not found."))?,
            )
            .map_err(|buf| {
                anyhow!(
                    "User home directory is not an Utf8 path. : {}",
                    buf.display()
                )
            })?
            .join(".smrec")
            .join("config.toml")
        };

        if path.exists() {
            let config = std::fs::read_to_string(path)?;
            let mut config: SmrecConfig = toml::from_str(&config)?;
            config.channels_to_record = channels_to_record;

            config.channels_to_record.iter().for_each(|channel| {
                if !config.channel_names.contains_key(&(channel + 1)) {
                    config
                        .channel_names
                        .insert(*channel + 1, format!("chn_{}.wav", channel + 1));
                }
            });
            config.cpal_stream_config = Some(cpal_stream_config);
            config.out_path = out_path;
            return Ok(config);
        }

        let mut channel_names = HashMap::new();
        for channel in &channels_to_record {
            channel_names.insert(*channel + 1, format!("chn_{}.wav", channel + 1));
        }
        Ok(Self {
            channel_names,
            channels_to_record,
            out_path,
            cpal_stream_config: Some(cpal_stream_config),
        })
    }

    pub fn supported_cpal_stream_config(&self) -> SupportedStreamConfig {
        self.cpal_stream_config.clone().unwrap()
    }

    pub fn channels_to_record(&self) -> &[usize] {
        &self.channels_to_record
    }

    pub fn channel_count(&self) -> usize {
        self.channels_to_record.len()
    }

    pub fn get_channel_name_from_0_indexed_channel_num(&self, index: usize) -> Result<String> {
        Ok(self
            .channel_names
            .get(&(index + 1))
            .ok_or_else(|| anyhow!("Channel {} does not exist.", index + 1))?
            .to_string())
    }

    pub fn writers(&self) -> Result<WriterHandles> {
        let now = Utc::now();

        // Format the date for YYYYMMDD_HHMMSS
        let dirname_date = format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );

        // Stamp base directory with date.
        let base = if let Some(out) = &self.out_path {
            Utf8PathBuf::from_str(out)?
        } else {
            Utf8PathBuf::from(".")
        };

        if !base.exists() {
            bail!("Output path which is provided {base} does not exist.");
        }

        let base = base.join(format!("rec_{dirname_date}"));

        // Create the base directory if it does not exist.
        if !base.exists() {
            std::fs::create_dir_all(&base)?;
        }

        // Make writers.
        let mut writers = Vec::new();
        for channel_num in &self.channels_to_record {
            let name = self.get_channel_name_from_0_indexed_channel_num(*channel_num)?;
            let spec = wav_spec_from_config(&self.supported_cpal_stream_config());
            let writer = hound::WavWriter::create(base.join(&name), spec)
                .expect("Failed to create wav writer.");
            writers.push(Arc::new(Mutex::new(Some(writer))));
        }

        Ok(Arc::new(writers))
    }
}

fn deserialize_usize_keys_greater_than_0<'de, D>(
    deserializer: D,
) -> Result<HashMap<usize, String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct UsizeKeyVisitor;

    impl<'de> Visitor<'de> for UsizeKeyVisitor {
        type Value = HashMap<usize, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map with string keys that represent usizes")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut map = HashMap::with_capacity(access.size_hint().unwrap_or(0));
            while let Some((key, value)) = access.next_entry::<String, String>()? {
                let usize_key = key.parse::<usize>().map_err(de::Error::custom)?;
                if usize_key < 1 {
                    return Err(de::Error::custom(
                        "a usize key must be greater than or equal to 1",
                    ));
                }
                map.insert(usize_key, value);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_map(UsizeKeyVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_external_config() {
        let config: &str = r#"
        [channel_names]
        1 = "channel_1.wav"
        2 = "channel_2.wav"
        3 = "channel_3.wav"
        4 = "channel_4.wav"
        5 = "channel_5.wav"
        6 = "channel_6.wav"
        7 = "channel_7.wav"
        8 = "channel_8.wav"
        "#;

        let config: SmrecConfig = toml::from_str(&config).unwrap();
        dbg!(config);
        // TODO: Finish the unit test.
    }

    #[test]

    fn glob() {
        assert!(glob_match::glob_match(
            "Behring*",
            "Behringer UMC1820 192k:192k"
        ));
    }
}
