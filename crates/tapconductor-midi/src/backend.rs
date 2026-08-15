// Copyright (c) 2026 Michael Saunders
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
    fn reload(&self) -> Result<(), MidiBackendError> {
        Ok(())
    }

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

    fn port_id(prefix: &str, native_id: &str) -> String {
        format!("midir:{prefix}:{native_id}")
    }

    fn native_port_id<'a>(id: &'a str, prefix: &str) -> Option<&'a str> {
        id.strip_prefix(&format!("midir:{prefix}:"))
            .filter(|id| !id.is_empty())
    }

    impl MidiBackend for MidirBackend {
        fn reload(&self) -> Result<(), MidiBackendError> {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            coremidi::restart().map_err(|status| {
                MidiBackendError::new(
                    "CoreMIDI reload",
                    format!("MIDIRestart returned OSStatus {status}"),
                )
            })?;
            Ok(())
        }

        fn input_devices(&self) -> Result<Vec<MidiDeviceInfo>, MidiBackendError> {
            let input = MidiInput::new("TapConductor discovery")
                .map_err(|error| MidiBackendError::new("input discovery", error.to_string()))?;
            let mut devices = Vec::new();
            for port in input.ports() {
                let name = input
                    .port_name(&port)
                    .unwrap_or_else(|_| "Unknown MIDI input".into());
                devices.push(MidiDeviceInfo {
                    id: port_id("in", &port.id()),
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
            for port in output.ports() {
                let name = output
                    .port_name(&port)
                    .unwrap_or_else(|_| "Unknown MIDI output".into());
                devices.push(MidiDeviceInfo {
                    id: port_id("out", &port.id()),
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
            let native_id = native_port_id(device_id, "in").ok_or_else(|| {
                MidiBackendError::new("input selection", "invalid midir input ID")
            })?;
            let mut input = MidiInput::new("TapConductor input")
                .map_err(|error| MidiBackendError::new("input creation", error.to_string()))?;
            input.ignore(Ignore::None);
            let port = input.find_port_by_id(native_id.to_owned()).ok_or_else(|| {
                MidiBackendError::new("input selection", "MIDI input is unavailable")
            })?;
            let connection = input
                .connect(
                    &port,
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
            let native_id = native_port_id(device_id, "out").ok_or_else(|| {
                MidiBackendError::new("output selection", "invalid midir output ID")
            })?;
            let output = MidiOutput::new("TapConductor output")
                .map_err(|error| MidiBackendError::new("output creation", error.to_string()))?;
            let port = output
                .find_port_by_id(native_id.to_owned())
                .ok_or_else(|| {
                    MidiBackendError::new("output selection", "MIDI output is unavailable")
                })?;
            let connection = output
                .connect(&port, "TapConductor output connection")
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

    #[cfg(test)]
    mod tests {
        use super::{native_port_id, port_id, MidirBackend};
        use crate::backend::MidiBackend;

        #[test]
        fn wrapped_port_ids_preserve_opaque_native_ids() {
            let wrapped = port_id("in", "2468:Controller Name");
            assert_eq!(native_port_id(&wrapped, "in"), Some("2468:Controller Name"));
            assert_eq!(native_port_id(&wrapped, "out"), None);
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn discovers_coremidi_endpoints_created_after_initial_enumeration() {
            use coremidi::{Client, Protocol};
            use std::{thread, time::Duration};

            let client = Client::new("TapConductor hotplug test").expect("CoreMIDI test client");
            let backend = MidirBackend;
            backend.input_devices().expect("initial input discovery");
            backend.output_devices().expect("initial output discovery");

            let suffix = format!("{}", std::process::id());
            let input_name = format!("TapConductor hotplug input {suffix}");
            let output_name = format!("TapConductor hotplug output {suffix}");
            let _source = client
                .virtual_source(&input_name)
                .expect("CoreMIDI virtual source");
            let _destination = client
                .virtual_destination_with_protocol(&output_name, Protocol::Midi10, |_| {})
                .expect("CoreMIDI virtual destination");

            let mut discovered = false;
            for _ in 0..20 {
                let inputs = backend.input_devices().expect("refreshed input discovery");
                let outputs = backend
                    .output_devices()
                    .expect("refreshed output discovery");
                if inputs.iter().any(|device| device.name == input_name)
                    && outputs.iter().any(|device| device.name == output_name)
                {
                    discovered = true;
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }

            assert!(
                discovered,
                "midir discovery did not observe CoreMIDI endpoints created after startup"
            );
        }
    }
}

#[cfg(feature = "midir-backend")]
pub use midir_impl::PublicMidirBackend as MidirBackend;
