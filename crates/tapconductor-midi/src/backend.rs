use crate::{MidiMessage, TimestampedMidiMessage};
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiPortDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiDeviceInfo {
    pub id: String,
    pub name: String,
    pub direction: MidiPortDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiBackendError {
    pub operation: &'static str,
    pub detail: String,
}

impl MidiBackendError {
    pub fn new(operation: &'static str, detail: impl Into<String>) -> Self {
        Self {
            operation,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for MidiBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MIDI {} failed: {}", self.operation, self.detail)
    }
}

impl std::error::Error for MidiBackendError {}

pub trait MidiInputConnection: Send {}

/// Stateful output connection. Calls occur from the native scheduler, never
/// the UI thread's notation renderer.
pub trait MidiOutputConnection: Send {
    fn send(&mut self, message: MidiMessage) -> Result<(), MidiBackendError>;
}

pub type MidiInputHandler = Box<dyn FnMut(TimestampedMidiMessage) + Send + 'static>;

/// Backend-neutral device discovery and connection API.
pub trait MidiBackend {
    fn input_devices(&self) -> Result<Vec<MidiDeviceInfo>, MidiBackendError>;
    fn output_devices(&self) -> Result<Vec<MidiDeviceInfo>, MidiBackendError>;

    fn connect_input(
        &self,
        device_id: &str,
        handler: MidiInputHandler,
    ) -> Result<Box<dyn MidiInputConnection>, MidiBackendError>;

    fn connect_output(
        &self,
        device_id: &str,
    ) -> Result<Box<dyn MidiOutputConnection>, MidiBackendError>;
}

#[cfg(feature = "midir-backend")]
mod midir_impl {
    use super::{
        MidiBackend, MidiBackendError, MidiDeviceInfo, MidiInputConnection, MidiInputHandler,
        MidiOutputConnection, MidiPortDirection,
    };
    use crate::{parse_midi1, MidiMessage, MidiTimestamp, TimestampedMidiMessage};
    use midir::{Ignore, MidiInput, MidiOutput};

    #[derive(Clone, Copy, Debug, Default)]
    pub struct MidirBackend;

    fn port_id(prefix: &str, index: usize, name: &str) -> String {
        format!("midir:{prefix}:{index}:{name}")
    }

    fn parse_index(id: &str, prefix: &str) -> Option<usize> {
        let mut pieces = id.splitn(4, ':');
        if pieces.next()? != "midir" || pieces.next()? != prefix {
            return None;
        }
        pieces.next()?.parse().ok()
    }

    impl MidiBackend for MidirBackend {
        fn input_devices(&self) -> Result<Vec<MidiDeviceInfo>, MidiBackendError> {
            let input = MidiInput::new("TapConductor discovery")
                .map_err(|error| MidiBackendError::new("input discovery", error.to_string()))?;
            let mut devices = Vec::new();
            for (index, port) in input.ports().iter().enumerate() {
                let name = input
                    .port_name(port)
                    .unwrap_or_else(|_| "Unknown MIDI input".into());
                devices.push(MidiDeviceInfo {
                    id: port_id("in", index, &name),
                    name,
                    direction: MidiPortDirection::Input,
                });
            }
            Ok(devices)
        }

        fn output_devices(&self) -> Result<Vec<MidiDeviceInfo>, MidiBackendError> {
            let output = MidiOutput::new("TapConductor discovery")
                .map_err(|error| MidiBackendError::new("output discovery", error.to_string()))?;
            let mut devices = Vec::new();
            for (index, port) in output.ports().iter().enumerate() {
                let name = output
                    .port_name(port)
                    .unwrap_or_else(|_| "Unknown MIDI output".into());
                devices.push(MidiDeviceInfo {
                    id: port_id("out", index, &name),
                    name,
                    direction: MidiPortDirection::Output,
                });
            }
            Ok(devices)
        }

        fn connect_input(
            &self,
            device_id: &str,
            mut handler: MidiInputHandler,
        ) -> Result<Box<dyn MidiInputConnection>, MidiBackendError> {
            let index = parse_index(device_id, "in").ok_or_else(|| {
                MidiBackendError::new("input selection", "invalid midir input ID")
            })?;
            let mut input = MidiInput::new("TapConductor input")
                .map_err(|error| MidiBackendError::new("input creation", error.to_string()))?;
            input.ignore(Ignore::None);
            let ports = input.ports();
            let port = ports.get(index).ok_or_else(|| {
                MidiBackendError::new("input selection", "MIDI input is unavailable")
            })?;
            let connection = input
                .connect(
                    port,
                    "TapConductor input connection",
                    move |timestamp, bytes, _| {
                        if let Ok(Some(message)) = parse_midi1(bytes) {
                            handler(TimestampedMidiMessage {
                                timestamp: MidiTimestamp(timestamp),
                                message,
                            });
                        }
                    },
                    (),
                )
                .map_err(|error| MidiBackendError::new("input connection", error.to_string()))?;
            Ok(Box::new(MidirInputConnection {
                _connection: connection,
            }))
        }

        fn connect_output(
            &self,
            device_id: &str,
        ) -> Result<Box<dyn MidiOutputConnection>, MidiBackendError> {
            let index = parse_index(device_id, "out").ok_or_else(|| {
                MidiBackendError::new("output selection", "invalid midir output ID")
            })?;
            let output = MidiOutput::new("TapConductor output")
                .map_err(|error| MidiBackendError::new("output creation", error.to_string()))?;
            let ports = output.ports();
            let port = ports.get(index).ok_or_else(|| {
                MidiBackendError::new("output selection", "MIDI output is unavailable")
            })?;
            let connection = output
                .connect(port, "TapConductor output connection")
                .map_err(|error| MidiBackendError::new("output connection", error.to_string()))?;
            Ok(Box::new(MidirOutputConnection(connection)))
        }
    }

    struct MidirInputConnection {
        _connection: midir::MidiInputConnection<()>,
    }
    impl MidiInputConnection for MidirInputConnection {}

    struct MidirOutputConnection(midir::MidiOutputConnection);
    impl MidiOutputConnection for MidirOutputConnection {
        fn send(&mut self, message: MidiMessage) -> Result<(), MidiBackendError> {
            let packet = message.to_midi1();
            self.0
                .send(packet.bytes())
                .map_err(|error| MidiBackendError::new("message send", error.to_string()))
        }
    }

    pub use self::MidirBackend as PublicMidirBackend;
}

#[cfg(feature = "midir-backend")]
pub use midir_impl::PublicMidirBackend as MidirBackend;
