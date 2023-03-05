// Copyright 2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! A cursed contraption to steal private fields from structs

use std::mem::size_of;

use buttplug::server::device::configuration::ProtocolAttributesType as ButtplugProtocolAttributesType;
use buttplug::server::device::server_device::ServerDeviceIdentifier as ButtplugServeDeviceIdentifier;

#[derive(Eq, PartialEq, Debug)]
pub struct ServerDeviceIdentifier {
    /// Address, as possibly serialized by whatever the managing library for the Device Communication Manager is.
    pub address: String,
    /// Name of the protocol used
    pub protocol: String,
    /// Internal identifier for the protocol used
    pub attributes_identifier: ProtocolAttributesType,
}

impl ServerDeviceIdentifier {
    /// make sure the compiler didn't screw us over by using different representations of identical structs
    pub fn test() {
        ServerDeviceIdentifier::test_same_size();
        ServerDeviceIdentifier::test_same_repr();
        ServerDeviceIdentifier::test_same_repr_empty_identifier();
        ServerDeviceIdentifier::test_same_repr_empty_everything();
        ServerDeviceIdentifier::test_same_repr_real_values();
    }

    fn test_same_size() {
        assert_eq!(size_of::<ServerDeviceIdentifier>(), size_of::<ButtplugServeDeviceIdentifier>());
    }

    fn test_same_repr() {
        let address = "abc";
        let protocol = "def";
        let identifier = "ghi";

        let buttplug_id: ButtplugServeDeviceIdentifier = ButtplugServeDeviceIdentifier::new(address, protocol, &ButtplugProtocolAttributesType::Identifier(identifier.to_string()));
        let buttplug_debug_string = format!("{buttplug_id:?}");
        let actual: ServerDeviceIdentifier = buttplug_id.into();
        let expected = ServerDeviceIdentifier {
            address: address.to_string(),
            protocol: protocol.to_string(),
            attributes_identifier: ProtocolAttributesType::Identifier(identifier.to_string()),
        };
        let our_debug_string = format!("{expected:?}");
        assert_eq!(actual, expected);
        assert_eq!(buttplug_debug_string, our_debug_string);
    }

    fn test_same_repr_empty_identifier() {
        let address = "jkl";
        let protocol = "mno";

        let buttplug_id: ButtplugServeDeviceIdentifier = ButtplugServeDeviceIdentifier::new(address, protocol, &ButtplugProtocolAttributesType::Default);
        let buttplug_debug_string = format!("{buttplug_id:?}");
        let actual: ServerDeviceIdentifier = buttplug_id.into();
        let expected = ServerDeviceIdentifier {
            address: address.to_string(),
            protocol: protocol.to_string(),
            attributes_identifier: ProtocolAttributesType::Default,
        };
        let our_debug_string = format!("{expected:?}");
        assert_eq!(actual, expected);
        assert_eq!(buttplug_debug_string, our_debug_string);
    }

    fn test_same_repr_empty_everything() {
        let buttplug_id: ButtplugServeDeviceIdentifier = ButtplugServeDeviceIdentifier::new("", "", &ButtplugProtocolAttributesType::Default);
        let buttplug_debug_string = format!("{buttplug_id:?}");
        let actual: ServerDeviceIdentifier = buttplug_id.into();
        let expected = ServerDeviceIdentifier {
            address: String::new(),
            protocol: String::new(),
            attributes_identifier: ProtocolAttributesType::Default,
        };
        let our_debug_string = format!("{expected:?}");
        assert_eq!(actual, expected);
        assert_eq!(buttplug_debug_string, our_debug_string);
    }

    fn test_same_repr_real_values() {
        let address = "PeripheralId(FB:1E:14:5B:4F:3F)";
        let protocol = "lovense";
        let identifier = "P";

        let buttplug_id: ButtplugServeDeviceIdentifier = ButtplugServeDeviceIdentifier::new(address, protocol, &ButtplugProtocolAttributesType::Identifier(identifier.to_string()));
        let buttplug_debug_string = format!("{buttplug_id:?}");
        let actual: ServerDeviceIdentifier = buttplug_id.into();
        let expected = ServerDeviceIdentifier {
            address: address.to_string(),
            protocol: protocol.to_string(),
            attributes_identifier: ProtocolAttributesType::Identifier(identifier.to_string()),
        };
        let our_debug_string = format!("{expected:?}");
        assert_eq!(actual, expected);
        assert_eq!(buttplug_debug_string, our_debug_string);
        assert_eq!(r#"ServerDeviceIdentifier { address: "PeripheralId(FB:1E:14:5B:4F:3F)", protocol: "lovense", attributes_identifier: Identifier("P") }"#, our_debug_string);
    }
}

impl From<ButtplugServeDeviceIdentifier> for ServerDeviceIdentifier {
    fn from(value: ButtplugServeDeviceIdentifier) -> Self {
        // evil hack that will break if rustc decides to represent identical structs differently (or if qdot changes the struct)
        unsafe {
            std::mem::transmute(value)
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
pub enum ProtocolAttributesType {
    /// Default for all devices supported by a protocol
    Default,
    /// Device class specific identification, with a string specific to the protocol.
    Identifier(String),
}

#[cfg(test)]
/// running the repr tests as part of `cargo test` is terrible, but it might not catch issues in the *real* release build.
mod tests {
    use super::*;

    #[test]
    fn identical_structs_have_same_size() {
        ServerDeviceIdentifier::test_same_size();
    }

    #[test]
    fn identical_structs_have_same_repr() {
        ServerDeviceIdentifier::test_same_repr();
    }

    #[test]
    fn identical_structs_have_same_repr_with_empty_identifier() {
        ServerDeviceIdentifier::test_same_repr_empty_identifier();
    }

    #[test]
    fn identical_structs_have_same_repr_with_empty_everything() {
        ServerDeviceIdentifier::test_same_repr_empty_everything();
    }

    #[test]
    fn identical_structs_have_same_repr_with_real_values() {
        ServerDeviceIdentifier::test_same_repr_real_values();
    }
}
