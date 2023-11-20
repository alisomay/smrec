// Most of the lints we deny here have a good chance to be relevant for our project.
#![deny(clippy::all)]
// We warn for all lints on the planet. Just to filter them later for customization.
// It is impossible to remember all the lints so a subtractive approach keeps us updated, in control and knowledgeable.
#![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]
// Then in the end we allow ridiculous or too restrictive lints that are not relevant for our project.
// This list is dynamic and will grow in time which will define our style.
#![allow(
    clippy::multiple_crate_versions,
    clippy::blanket_clippy_restriction_lints,
    clippy::missing_docs_in_private_items,
    clippy::pub_use,
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
    clippy::implicit_return,
    clippy::missing_inline_in_public_items,
    clippy::similar_names,
    clippy::question_mark_used,
    clippy::expect_used,
    clippy::missing_errors_doc,
    clippy::pattern_type_mismatch,
    clippy::module_name_repetitions,
    clippy::empty_structs_with_brackets,
    clippy::as_conversions,
    clippy::self_named_module_files,
    clippy::cargo_common_metadata,
    clippy::exhaustive_structs,
    // It is a binary crate, panicing is usually fine.
    clippy::missing_panics_doc
)]

mod config;
mod list;
mod midi;
mod osc;
mod stream;
mod types;
mod wav;

use crate::{
    config::{choose_channels_to_record, SmrecConfig},
    midi::Midi,
};
use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use config::{choose_device, choose_host};
use cpal::traits::{DeviceTrait, StreamTrait};
use hound::WavWriter;
use osc::Osc;
use std::{
    cell::RefCell,
    fs::File,
    io::BufWriter,
    rc::Rc,
    sync::{Arc, Mutex},
};
use types::Action;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Minimalist multi-track audio recorder which may be controlled via OSC or MIDI.
You may visit <https://github.com/alisomay/smrec/blob/main/README.md> for a detailed tutorial."
)]
struct Cli {
    /// Specify audio host.
    /// Example: smrec --host "Asio"
    #[clap(long)]
    host: Option<String>,
    /// Specify audio device.
    /// Example: smrec --device "MacBook Pro Microphone"
    #[clap(long)]
    device: Option<String>,
    /// Include specified channels in recording.
    /// Example: smrec --include 1,2
    #[clap(long, value_delimiter = ',', num_args = 1..)]
    include: Option<Vec<usize>>,
    /// Exclude specified channels from recording.
    /// Example: smrec --exclude 1
    #[clap(long, value_delimiter = ',', num_args = 1..)]
    exclude: Option<Vec<usize>>,
    /// Specify path to configuration file.
    /// Example: smrec --config "./config.toml"
    #[clap(long)]
    config: Option<String>,
    /// Specify directory for recording output.
    /// Example: smrec --out ~/Music
    #[clap(long)]
    out: Option<String>,
    /// Specify recording duration in seconds.
    /// Example: smrec --duration 10
    #[clap(long)]
    duration: Option<String>,
    /// Configure OSC control.
    /// Example: smrec --osc "0.0.0.0:18000;255.255.255.255:18001"
    #[clap(long, value_delimiter = ';', num_args = 0..2, default_value = "EMPTY_HACK", hide_default_value = true)]
    osc: Vec<String>,
    /// Configure MIDI control.
    /// Example: smrec --midi my first port[(1,2,3), (15, 127, 126), (12,4,5)], my second port[(1,2,3)]
    #[clap(long, value_delimiter = ';', num_args = 0..2, default_value = "EMPTY_HACK", hide_default_value = true)]
    midi: Vec<String>,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Lists hosts, devices and configs.
    #[clap(about = "Lists hosts, devices and configs.")]
    List(List),
}

#[derive(Parser)]
struct List {
    /// List MIDI configurations.
    /// Example: smrec list --midi
    #[clap(long)]
    midi: bool,
    /// List audio configurations.
    /// Example: smrec list --audio
    #[clap(long)]
    audio: bool,
}

pub type WriterHandle = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;
pub type WriterHandles = Arc<Vec<WriterHandle>>;

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    let cli = Cli::parse();

    let host = choose_host(cli.host)?;

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

    let device = choose_device(&host, cli.device)?;
    let writers_container: Arc<Mutex<Option<WriterHandles>>> = Arc::new(Mutex::new(None));
    let stream_container: Rc<RefCell<Option<cpal::Stream>>> = Rc::new(RefCell::new(None));

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
            let mut osc = Osc::new(
                &osc_config,
                to_main_thread.clone(),
                from_main_thread.clone(),
            )?;
            osc.listen();
            Some(osc)
        } else {
            None
        };

        let midi = if let Some(midi) = cli_midi {
            let mut midi = Midi::new(to_main_thread, from_main_thread, &midi)?;
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
                &from_listener_thread,
                &to_listener_thread,
                &device,
                &stream_container,
                &writers_container,
                &smrec_config,
            ),
        }

        // No listeners, just start recording, for ever or for a certain duration.

        new_recording(
            &device,
            &stream_container,
            &writers_container,
            &smrec_config,
        )?;

        cli.duration.map_or_else(
            || {
                std::thread::park();
            },
            |dur| {
                let secs = dur
                    .parse::<u64>()
                    .expect("--duration must be a positive integer.");
                std::thread::park_timeout(std::time::Duration::from_secs(secs));
            },
        );

        stop_recording(&stream_container, &writers_container)?;
        println!("Recording complete!");
    } else {
        bail!("No default input config found for device.");
    }

    Ok(())
}

pub fn listen_and_block_main_thread(
    from_listener_thread: &crossbeam::channel::Receiver<Action>,
    to_listener_thread: &crossbeam::channel::Sender<Action>,
    device: &cpal::Device,
    stream_container: &Rc<RefCell<Option<cpal::Stream>>>,
    writers_container: &Arc<Mutex<Option<WriterHandles>>>,
    smrec_config: &SmrecConfig,
) {
    loop {
        match from_listener_thread.recv() {
            Ok(Action::Start) => {
                if let Err(err) =
                    new_recording(device, stream_container, writers_container, smrec_config)
                {
                    println!("Error starting recording: {err}");

                    to_listener_thread
                        .send(Action::Err(format!("Error starting recording: {err}")))
                        .expect("Internal thread error.");
                } else {
                    to_listener_thread
                        .send(Action::Start)
                        .expect("Internal thread error.");
                }
            }
            Ok(Action::Stop) => {
                if let Err(err) = stop_recording(stream_container, writers_container) {
                    println!("Error stopping recording: {err}");
                    to_listener_thread
                        .send(Action::Err(format!("Error starting recording: {err}")))
                        .expect("Internal thread error.");
                } else {
                    to_listener_thread
                        .send(Action::Stop)
                        .expect("Internal thread error.");
                }
            }
            // Should not be used here though, no user facing api anyway.
            Ok(Action::Err(err)) => {
                println!("Error: {err}");
            }
            Err(_) => {
                println!("Error receiving from listener thread.");
            }
        }
    }
}

pub fn new_recording(
    device: &cpal::Device,
    stream_container: &Rc<RefCell<Option<cpal::Stream>>>,
    writer_handles: &Arc<Mutex<Option<WriterHandles>>>,
    smrec_config: &SmrecConfig,
) -> Result<()> {
    // If there's an active stream, pause it and finalize the writers
    if let Some(stream) = stream_container.borrow_mut().as_mut() {
        stream.pause()?;
        finalize_writers_if_some(writer_handles).unwrap();
        println!("Restarting new recording...");
    } else {
        println!("Starting recording...");
    }

    // Make new writers
    let writers = smrec_config.writers()?;
    // Replace the old ones.
    writer_handles.lock().unwrap().replace(writers);

    // Errors when ctrl+c handler is already set. We ignore this error since we have no intention of a reset.
    let writer_handles_in_ctrlc = Arc::clone(writer_handles);
    let _ = ctrlc::try_set_handler(move || {
        // TODO: Necessary to drop stream?

        // TODO: Maybe inform user in unsuccessful operation?
        finalize_writers_if_some(&writer_handles_in_ctrlc).unwrap();

        // TODO: Better message, differentiate if the recording was stopped or interrupted.
        println!("\rRecording interrupted thus stopped.");
        std::process::exit(0);
    });

    // Create and start a new stream
    let new_stream = stream::build(
        device,
        smrec_config.supported_cpal_stream_config(),
        smrec_config.channels_to_record(),
        Arc::clone(writer_handles),
    )?;

    new_stream.play()?;
    println!("Recording started.");
    stream_container.borrow_mut().replace(new_stream);

    Ok(())
}

pub fn stop_recording(
    stream_container: &Rc<RefCell<Option<cpal::Stream>>>,
    writer_handles: &Arc<Mutex<Option<WriterHandles>>>,
) -> Result<()> {
    println!("Stopping recording...");

    if let Some(stream) = stream_container.borrow_mut().take() {
        stream.pause()?;
        finalize_writers_if_some(writer_handles)?;
        println!("Recording stopped.");
        return Ok(());
    }
    println!("There is no running recording to stop.");

    Ok(())
}

pub fn finalize_writers_if_some(writers: &Arc<Mutex<Option<WriterHandles>>>) -> Result<()> {
    let writers = writers.lock().unwrap().take();
    if let Some(writers) = writers {
        for writer in writers.iter() {
            if let Some(writer) = writer.lock().unwrap().take() {
                writer.finalize().unwrap();
            }
        }
    }
    Ok(())
}
