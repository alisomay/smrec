<p align="center">
  <img src="https://raw.githubusercontent.com/alisomay/smrec/main/assets/logo_transparent.png"/>
</p>

# smrec

Minimalist multi-track audio recorder which may be controlled via OSC or MIDI.

I did this because I needed a simple multi-track audio recorder which I could control via OSC or MIDI.

I didn't want and do not have the resources to use a DAW for this purpose in my setup and wanted mono wave files for each channel organized in a directory per recording by date and time

I'm using this recorder in a setup where I use a [Behringer XR18](https://www.behringer.com/product.html?modelCode=P0BI8) as an audio interface and a [LattePanda 3 Delta](https://www.lattepanda.com/lattepanda-3-delta) as a SBC.

Now let's record some sound! 🔔

---

## Installation

```
cargo install smrec
```

### Installing for Windows and ASIO support

`smrec` uses [`cpal`](https://github.com/RustAudio/cpal) as the underlying audio API. `cpal` supports WASAPI, DirectSound and ASIO on Windows. However, since `cargo` builds binaries from source in the target machine and it is [not very straight forward](https://github.com/RustAudio/cpal#asio-on-windows) to build `cpal` with ASIO support due to `asio-sys` build script, **there is a pre-build script provided** in this repository.

To install `smrec` on Windows, please follow these steps in order:

- Install the latest Visual Studio (if you don't have it already)
- Install the latest LLVM (if you don't have it already) from [here](https://releases.llvm.org/download.html)
- Open Command Prompt as administrator
- Set LLVM path in the environment variables (system wide)
  ```
  setx /M LIBCLANG_PATH "C:\Program Files\LLVM\bin"
  ```
- Open PowerShell as administrator
- Check if the environment variable is set correctly
  ```
  $env:LIBCLANG_PATH
  ```
- Source the pre-build script in the repository (assuming you are in the root of the repository)
  ```
  . .\pre-build-win.ps1
  ```
  this script will download the ASIO SDK, set Visual Studio environment variables and `CPAL_ASIO_DIR` variable for the current shell session.
- Install as usual
  ```
  cargo install smrec
  ```

If you know what you're doing feel free to skip these steps and consult the [`cpal` documentation](https://github.com/RustAudio/cpal#asio-on-windows).

### Pre-built binaries

Pre-built binaries as an alternative are available for Windows [here](https://github.com/alisomay/smrec/releases) due to the complicated process of building `cpal` with ASIO support on Windows currently.

## Tutorial

### Simply as a command

```
smrec
```

Runs with the default configuration which is:

- The default audio host
- The default audio device
- The default set of input channels
- The default directory to record which is the current working directory
- The recording goes on until `ctrl+c` is pressed and the program is interrupted.
- Creates a directory named `rec_YYYYMMDD_HHMMSS` in the current working directory and records the audio in that directory.
- The audio is recorded in `wav` format.
- The audio is recorded in the default sample rate and buffer size and sample format of the audio device.
- For every channel a separate file is created (mono) and the file name for each is `chn_XX.wav` where `XX` is the channel number.

To record for a specific duration, use the `--duration` flag and specify the duration in seconds.
The following command records for 10 seconds:

```
smrec --duration 10
```

By using the `--host` and `--device` flag , you can specify the audio host and device to use. The following command uses `MacBook Pro Microphone` as the audio device:

```
smrec --device "MacBook Pro Microphone"
```

#### Listing midi ports and audio hosts and devices

```
smrec list
```

#### Including and excluding channels from a recording

By default, all channels of the audio device are recorded. You can specify which channels to include or exclude from the recording by using the `--include` and `--exclude` flags. These flags can not be used together. The following command records only the first two channels of a 4 channel audio device:

```
smrec --include 1,2
```

And this command records all channels except the first channel of a 4 channel audio device:

```
smrec --exclude 1
```

as seen in the examples, the channel numbers start from 1 and they can be specified as a comma separated list.

#### Recording to a specific directory

By default, the recording is done in the current working directory. You can specify a directory to record to by using the `--directory` flag. The following command records to the `~/Music` directory:

```
smrec --out ~/Music
```

#### Configuring with a configuration file

`smrec` uses the cli arguments for configuration and they precede everything. However, you can configure some aspects (probably more to come) of `smrec` by using a configuration file so they replace the default configuration. The configuration file is a `toml` file and it is named `config.toml`. The configuration file is searched in the following order:

- `.smrec/config.toml` in the current working directory.
- `.smrec/config.toml` in the user home directory.
- If none of the above is found, the default configuration is used.

The configuration file can configure:

- Channel names

```toml
[channel_names]
1 = "Kick"
2 = "Snare"
3 = "Hi-Hat"
```

- More to come..

### OSC control

`smrec` normally starts recording as soon as it is run. However it also has options for various control methods.

Running, `smrec --osc` will not start recording immediately but instead it will wait for an OSC message to start recording.
The default OSC port for receiving and sending is chosen randomly by the os and the default addresses for sending and receiving is `127.0.0.1` and `0.0.0.0`.
After running the command above, the output might look like this:

```
Will be sending OSC messages to 127.0.0.1:61213
Listening for OSC messages on 0.0.0.0:51014
```

Currently `smrec` does not support IPv6.

In the default configuration:

- Listens for OSC messages on a randomly chosen port on all addresses.
- Sends OSC messages to localhost on a randomly chosen port.

To configure OSC further arguments could be added to the flag:

```
smrec --osc "<listen_address>:<listen_port>;<send_address>:<send_port>"
```

or

```
smrec --osc "<listen_address>:<listen_port>"
```

the second form would keep the default send address and port.

```
smrec --osc "0.0.0.0:18000;255.255.255.255:18001"
```

will listen for OSC messages on all addresses on port `18000` and send OSC messages to all addresses on port `18001`.
Yes, `smrec` can also broadcast OSC messages is the OS and the network allows it.

#### OSC messages

The messages which `smrec` listens for are:

- `/smrec/start` - Starts the recording, sending a second start will stop the running recording and starts a new one creating a new directory in the specified root.
- `/smrec/stop` - Stops the recording if there is a running one.

The messages which `smrec` sends are:

- `/smrec/start` - Sent when a new recording is started.
- `/smrec/stop` - Sent when a running recording is stopped.
- `/smrec/error <string>`- Sent when some errors occur and the error message is transferred a string in the argument.

### MIDI control

`smrec` can also be controlled via MIDI. It can even be controlled via OSC and MIDI simultaneously.
Though `smrec` is a simplistic application to serve a single purpose the MIDI communication the options it provides for configuring MIDI is extensive.

Running, `smrec --midi` will not start recording immediately but instead it will wait for a MIDI CC message to start recording.

Here is the default configuration:

- Finds all available MIDI ports and starts listening on them.
- Listens for any channel in those ports.
- Reacts to CC 16 to start the recording and CC 17 to stop the recording.
- As in OSC, sending subsequent CC 16 messages will stop the running recording and start a new one creating a new directory in the specified root.
- `smrec --midi` is synonymous with `smrec --midi "[*[(*,16,17)]]"` which will be explained below.

#### Configuration

The `--midi` flag accepts a string argument which is parsed as a configuration string which configures the input and output.
These strings are separated by a semicolon (`;`) and the first part configures the input and the second part configures the output.
Any part could be left out.

Deconstruction:

- `[..]` is a container for an input or output configuration.
- `[port name[..], ..]` a comma separated list of port names which `smrec` will connect to.
- `[port name[(..), ..], ..]` each port name should contain at least one channel/MIDI CC filter configuration.
- `(<channel number>, <cc number for starting the recording>, <cc number for stopping the recording>)` this is the structure of a channel/MIDI CC filter configuration.
- `(1,2,3)` here is an example, this will listen for CC 2 on channel 2 to start the recording and CC 3 on channel 2 to stop the recording. All other messages in that port is ignored. MIDI channels are 0 indexed!
- `[my nice port[(1,2,3), ..], ..]` this is how we use that tuple.
- `[my nice port[(1,2,3), (15, 127, 126), ..], ..]` as all the elements we can have multiples of those.
- `[ my first port[(1,2,3), (15, 127, 126), (12,4,5)], my second port[(1,2,3)] ]` here is a valid configuration string. It will listen for CC 2 on channel 2 to start the recording and CC 3 on channel 2 to stop the recording on `my first port` and listen for CC 2 on channel 2 to start the recording and CC 3 on channel 2 to stop the recording on `my second port`. All other messages in those ports are ignored.

Use of '\*' and glob patterns:

- Port names in the configuration string are treated as [glob patterns](<https://en.wikipedia.org/wiki/Glob_(programming)>).
- `*` matches any port. Which in the end means all ports.
- All valid glob patterns could be used to match port names.
- Deconstructing the default configuration string: `[*[(*,16,17)]]` now should make sense.
- Listen on all ports, do not filter by channel reacting to all MIDI CC messages which are CC 16 to start the recording and CC 17 to stop the recording.

`smrec` can also send midi messages on certain events.
If the output port is configured with a configuration, the configured CC messages will be sent on the configured port and channels on start and stop events.

#### Values

MIDI CC values are considered momentary.

Once a value `127` is received through a configured MIDI CC number the action is taken immediately.
**This is why sending bursts of MIDI CC messages is not a good idea.**
Every message would trigger a new recording if it is configured to start the recording.

`smrec` sends MIDI CC messages with a value of `127` on start and `127` on stop to the configured MIDI CC numbers if output is configured.

As a last example to get the hang of it, this configuration string will listen for CC 2 on channel 2 to start the recording and CC 3 on channel 2 to stop the recording on `my first port` and listen for CC 2 on channel 2 to start the recording and CC 3 on channel 2 to stop the recording on `my second port`. All other messages in those ports are ignored. On start and stop events, it will send CC 16 with a value of 127 on channel 2 on `my first port` and send CC 17 with a value of 127 on channel 2 on `my second port`.

```
[ my first port[(1,2,3), (15, 127, 126), (12,4,5)], my second port[(1,2,3)] ];[ my first port[(1,2,3), (15, 127, 126), (12,4,5)], my second port[(1,2,3)] ]
```

## Next steps

I'm going to make sure,

- Installation gets smoother
- Proper distribution packages are provided
- Documentation is complete
- Bugs are fixed
- Better messages to the user

But I don't plan to heavily maintain this project, I'll just make sure that it is usable enough and lives.

## Support

- Desktop
  - macOS:
    - `x86_64` ✅
    - `aarch64` ✅
  - linux:
    - `x86_64` ✅
    - `aarch64` ✅
  - windows:
    - `x86_64` ✅
    - `aarch64` ✅

## Contributing

- Be friendly and productive
- Follow common practice open source contribution culture
- Rust [code of conduct](https://www.rust-lang.org/policies/code-of-conduct) applies

Thank you 🙏

## Last words

It is something I needed to resolve a specific problem and I shared it publicly.
I hope it resolves your problem too.
