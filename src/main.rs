mod config;
mod list;
mod midi;
mod osc;
mod stream;
mod types;
mod wav;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use config::{choose_device, choose_host};
use cpal::traits::{DeviceTrait, StreamTrait};
use hound::WavWriter;
use osc::Osc;
use std::fs::File;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};
use types::Action;

use crate::config::{choose_channels_to_record, SmrecConfig};
use crate::midi::Midi;

// TODO: Catch ctrl+c and stop recording.

#[derive(Parser)]
#[command(author, version, about, long_about = None)] // Read from `Cargo.toml`
struct Cli {
    #[arg(long)]
    asio: bool,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    device: Option<String>,
    #[clap(long, value_delimiter = ',', num_args = 1..)]
    include: Option<Vec<usize>>,
    #[clap(long, value_delimiter = ',', num_args = 1..)]
    exclude: Option<Vec<usize>>,
    #[arg(long)]
    config: Option<String>,
    #[arg(long)]
    out: Option<String>,
    #[arg(long)]
    duration: Option<String>,

    #[clap(long, value_delimiter = ';', num_args = 0..2, default_value = "EMPTY_HACK")]
    osc: Vec<String>,

    #[clap(long, value_delimiter = ';', num_args = 0..2, default_value = "EMPTY_HACK")]
    midi: Vec<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Lists hosts, devices and configs.
    List(List),
}

#[derive(Parser)]
struct List {
    #[clap(long)]
    midi: bool,
    #[clap(long)]
    audio: bool,
}

pub type WriterHandle = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;
pub type WriterHandles = Arc<Vec<WriterHandle>>;

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    let host = choose_host(cli.host, cli.asio)?;

    if let Some(command) = cli.command {
        match command {
            // Enumerate and exit.
            Commands::List(list) => {
                if list.midi {
                    list::enumerate_midi()?;
                }
                if list.audio {
                    list::enumerate_audio()?;
                }
                if !list.audio || !list.midi {
                    list::enumerate_audio()?;
                    println!();
                    list::enumerate_midi()?;
                }
            }
        };
        return Ok(());
    }

    let device = Arc::new(choose_device(&host, cli.device)?);
    let writers_container: Arc<Mutex<Option<WriterHandles>>> = Arc::new(Mutex::new(None));
    let stream_container: Arc<Mutex<Option<cpal::Stream>>> = Arc::new(Mutex::new(None));

    if let Ok(config) = device.default_input_config() {
        let smrec_config = Arc::new(SmrecConfig::new(
            cli.config,
            cli.out,
            choose_channels_to_record(cli.include, cli.exclude, &config)?,
            config.clone(),
        )?);

        let (to_main_thread, from_listener_thread) = crossbeam::channel::unbounded::<Action>();
        let (to_listener_thread, from_main_thread) = crossbeam::channel::unbounded::<Action>();

        let cli_osc = if cli.osc == vec!["EMPTY_HACK"] {
            None
        } else if cli.osc.is_empty() {
            Some(vec![])
        } else {
            Some(cli.osc)
        };

        let cli_midi = if cli.midi == vec!["EMPTY_HACK"] {
            None
        } else if cli.midi.is_empty() {
            Some(vec![])
        } else {
            Some(cli.midi)
        };

        let osc = if let Some(osc_config) = cli_osc {
            if osc_config.len() > 2 {
                bail!("Too many arguments for --osc");
            }
            let mut osc = Osc::new(osc_config, to_main_thread.clone(), from_main_thread.clone())?;
            osc.listen();
            Some(osc)
        } else {
            None
        };

        let midi = if let Some(midi) = cli_midi {
            let mut midi = Midi::new(to_main_thread, from_main_thread, midi)?;
            midi.listen()?;
            Some(midi)
        } else {
            None
        };

        match (midi, osc) {
            (None, None) => {
                // Pass
            }
            _ => listen_and_block_main_thread(
                from_listener_thread.clone(),
                to_listener_thread.clone(),
                device.clone(),
                stream_container.clone(),
                writers_container.clone(),
                smrec_config.clone(),
            ),
        }

        // No listeners, just start recording, for ever or for a certain duration.

        new_recording(
            device.clone(),
            stream_container.clone(),
            writers_container.clone(),
            smrec_config.clone(),
        )?;

        if let Some(dur) = cli.duration {
            let secs = dur
                .parse::<u64>()
                .expect("--duration must be a positive integer.");
            std::thread::park_timeout(std::time::Duration::from_secs(secs));
        } else {
            std::thread::park();
        }

        stop_recording(stream_container.clone(), writers_container.clone())?;
        println!("Recording complete!");
    } else {
        bail!("No default input config found for device");
    }

    Ok(())
}

pub fn listen_and_block_main_thread(
    from_listener_thread: crossbeam::channel::Receiver<Action>,
    to_listener_thread: crossbeam::channel::Sender<Action>,
    device: Arc<cpal::Device>,
    stream_container: Arc<Mutex<Option<cpal::Stream>>>,
    writers_container: Arc<Mutex<Option<WriterHandles>>>,
    smrec_config: Arc<SmrecConfig>,
) {
    loop {
        match from_listener_thread.recv() {
            Ok(Action::Start) => {
                if let Err(err) = new_recording(
                    device.clone(),
                    stream_container.clone(),
                    writers_container.clone(),
                    smrec_config.clone(),
                ) {
                    println!("Error starting recording: {}", err);

                    to_listener_thread
                        .send(Action::Err(format!("Error starting recording: {}", err)))
                        .expect("Internal thread error.");
                } else {
                    to_listener_thread
                        .send(Action::Start)
                        .expect("Internal thread error.");
                }
            }
            Ok(Action::Stop) => {
                if let Err(err) =
                    stop_recording(stream_container.clone(), writers_container.clone())
                {
                    println!("Error stopping recording: {}", err);
                    to_listener_thread
                        .send(Action::Err(format!("Error starting recording: {}", err)))
                        .expect("Internal thread error.");
                } else {
                    to_listener_thread
                        .send(Action::Stop)
                        .expect("Internal thread error.");
                }
            }
            // Should not be used here though.
            Ok(Action::Err(err)) => {
                println!("Error: {}", err);
            }
            Err(_) => {
                println!("Error receiving from listener thread.");
            }
        }
    }
}

pub fn new_recording(
    device: Arc<cpal::Device>,
    stream_container: Arc<Mutex<Option<cpal::Stream>>>,
    writer_handles: Arc<Mutex<Option<WriterHandles>>>,
    smrec_config: Arc<SmrecConfig>,
) -> Result<()> {
    let mut stream_guard = stream_container.lock().unwrap();
    let mut writer_handles_guard = writer_handles.lock().unwrap();

    // If there's an active stream, pause it and finalize the writers
    if let Some(stream) = stream_guard.as_mut() {
        stream.pause()?;

        if let Some(writers) = writer_handles_guard.as_mut() {
            for writer in writers.iter() {
                writer.lock().unwrap().take().unwrap().finalize()?;
            }
        }
        println!("Restarting new recording...");
    } else {
        println!("Starting recording...");
    }

    // New writers
    let writers = smrec_config.writers()?;
    writer_handles_guard.replace(writers);

    let writers_handles_in_ctrlc = writer_handles.clone();

    // Ignore error.
    let _ = ctrlc::try_set_handler(move || {
        // TODO: Necessary to drop stream?
        let writers = writers_handles_in_ctrlc.lock().unwrap().take();
        if let Some(writers) = writers {
            for writer in writers.iter() {
                if let Some(writer) = writer.lock().unwrap().take() {
                    writer.finalize().unwrap();
                }
            }
        }
        // TODO: Better message, differentiate if the recording was stopped or interrupted.
        println!("\rRecording interrupted!");
        std::process::exit(1);
    });

    // Create and start a new stream
    let new_stream = stream::build_stream(
        &device,
        smrec_config.supported_cpal_stream_config(),
        smrec_config.channels_to_record(),
        writer_handles.clone(),
    )?;

    new_stream.play()?;

    println!("Recording started.");

    stream_guard.replace(new_stream);

    Ok(())
}

pub fn stop_recording(
    stream_container: Arc<Mutex<Option<cpal::Stream>>>,
    writer_handles: Arc<Mutex<Option<WriterHandles>>>,
) -> Result<()> {
    println!("Stopping recording...");

    let mut stream_guard = stream_container.lock().unwrap();
    if let Some(stream) = stream_guard.take() {
        stream.pause()?;

        if let Some(writers) = writer_handles.lock().unwrap().as_ref() {
            for writer in writers.iter() {
                writer.lock().unwrap().take().unwrap().finalize()?;
            }
        }
        println!("Recording stopped.");
        return Ok(());
    }
    println!("There is no running recording to stop.");

    Ok(())
}
