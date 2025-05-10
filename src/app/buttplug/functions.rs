// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! Various functions to work with buttplug devices

use std::collections::HashMap;
use std::sync::Arc;

use buttplug::client::ButtplugClientDevice;
use buttplug::core::message::{ButtplugDeviceMessageType, ClientGenericDeviceMessageAttributesV3};
use buttplug::server::device::ServerDeviceManager;

use crate::app::buttplug::structs::DeviceList;
use crate::app::structs::{ApplicationState, ApplicationStateDb, ApplicationStatus, DeviceStatus};
use crate::config::v3::{ActuatorType, MotorConfigurationV3, MotorTypeV3};
use crate::gui::TaggedMotor;

pub async fn get_tagged_devices(application_state_db: &ApplicationStateDb) -> Option<ApplicationStatus> {
    let application_state_mutex = application_state_db.read().await;
    match application_state_mutex.as_ref() {
        Some(application_state) => {
            let DeviceList { motors, mut devices } = get_devices(application_state).await;
            let configuration = &application_state.configuration;
            let tags = &configuration.tags;

            // convert tags to TaggedMotor
            let mut tagged_motors = motors_to_tagged(tags);

            // for each device not yet in TaggedMotor, generate a new dummy TaggedMotor
            let mut missing_motors: Vec<TaggedMotor> = motors
                .into_iter()
                .filter(|motor| {
                    !tagged_motors
                        .iter()
                        .any(|possible_match| &possible_match.motor == motor)
                })
                .map(|missing_motor| TaggedMotor::new(missing_motor, None))
                .collect();

            // merge results
            tagged_motors.append(&mut missing_motors);

            // sort the things
            tagged_motors.sort_unstable();
            devices.sort_unstable();

            Some(ApplicationStatus {
                motors: tagged_motors,
                devices,
                configuration: configuration.clone(),
            })
        }
        None => None,
    }
}

fn motors_to_tagged(tags: &HashMap<String, MotorConfigurationV3>) -> Vec<TaggedMotor> {
    tags.iter()
        .map(|(tag, motor)| TaggedMotor::new(motor.clone(), Some(tag.clone())))
        .collect()
}

/// Get display name for device.
#[inline(always)]
fn display_name_from_device(device: &ButtplugClientDevice) -> String {
    device.name().clone()
    // once we want to handle duplicate devices:
    //format!("{}#{}", device.name(), device.index())
}

/// Get unique identifier for a device. This should ALWAYS be the same for a given device.
#[inline(always)]
pub fn id_from_device(device: &ButtplugClientDevice, device_manager: &ServerDeviceManager) -> Option<String> {
    let device_info = device_manager.device_info(device.index())?;
    let device_id = device_info.identifier();
    Some(match device_id.identifier() {
        Some(attributes_identifier) => format!(
            "{}://{}/{}",
            device_id.protocol(),
            device_id.address(),
            attributes_identifier
        ),
        None => format!("{}://{}", device_id.protocol(), device_id.address()),
    })
}

/// Get a full debug name for a device. This is intended for logging.
pub fn debug_name_from_device(device: &ButtplugClientDevice, device_manager: &ServerDeviceManager) -> String {
    let name = display_name_from_device(device);
    match id_from_device(device, device_manager) {
        Some(id) => format!("{name}@{id}"),
        None => name,
    }
}

/// get all distinct motors
fn motor_configuration_from_devices(
    devices: Vec<Arc<ButtplugClientDevice>>,
    device_manager: &ServerDeviceManager,
) -> Vec<MotorConfigurationV3> {
    let mut motor_configuration_count: usize = 0;
    for device in devices.iter() {
        motor_configuration_count += device.message_attributes().scalar_cmd().as_ref().map_or(0, |v| v.len());
        motor_configuration_count += device.message_attributes().rotate_cmd().as_ref().map_or(0, |v| v.len());
        motor_configuration_count += device.message_attributes().linear_cmd().as_ref().map_or(0, |v| v.len());
    }

    let mut motor_configurations: Vec<MotorConfigurationV3> = Vec::with_capacity(motor_configuration_count);

    let empty_vec = Vec::new();

    for device in devices.into_iter() {
        let scalar_cmds: &Vec<ClientGenericDeviceMessageAttributesV3> =
            device.message_attributes().scalar_cmd().as_ref().unwrap_or(&empty_vec);
        for index in 0..scalar_cmds.len() {
            let message_attributes: &ClientGenericDeviceMessageAttributesV3 = scalar_cmds
                .get(index)
                .expect("I didn't know a vec could change mid-iteration");
            let actuator_type: ActuatorType = message_attributes.actuator_type().into();
            let motor_config = MotorConfigurationV3 {
                device_name: display_name_from_device(&device),
                device_identifier: id_from_device(&device, device_manager),
                feature_type: MotorTypeV3::Scalar { actuator_type },
                feature_index: index as u32,
            };
            motor_configurations.push(motor_config);
        }

        let rotate_cmds: &Vec<ClientGenericDeviceMessageAttributesV3> =
            device.message_attributes().rotate_cmd().as_ref().unwrap_or(&empty_vec);
        for index in 0..rotate_cmds.len() {
            let motor_config = MotorConfigurationV3 {
                device_name: display_name_from_device(&device),
                device_identifier: id_from_device(&device, device_manager),
                feature_type: MotorTypeV3::Rotation,
                feature_index: index as u32,
            };
            motor_configurations.push(motor_config);
        }

        let linear_cmds: &Vec<ClientGenericDeviceMessageAttributesV3> =
            device.message_attributes().linear_cmd().as_ref().unwrap_or(&empty_vec);
        for index in 0..linear_cmds.len() {
            let motor_config = MotorConfigurationV3 {
                device_name: display_name_from_device(&device),
                device_identifier: id_from_device(&device, device_manager),
                feature_type: MotorTypeV3::Linear,
                feature_index: index as u32,
            };
            motor_configurations.push(motor_config);
        }
    }

    motor_configurations
}

async fn get_devices(application_state: &ApplicationState) -> DeviceList {
    let devices = application_state.client.devices();
    let mut device_statuses: Vec<DeviceStatus> = Vec::with_capacity(devices.len());

    for device in devices.iter() {
        let battery_level = if device
            .message_attributes()
            .message_allowed(&ButtplugDeviceMessageType::BatteryLevelCmd)
        {
            device.battery_level().await.ok()
        } else {
            None
        };
        let rssi_level = if device
            .message_attributes()
            .message_allowed(&ButtplugDeviceMessageType::RSSILevelCmd)
        {
            device.rssi_level().await.ok()
        } else {
            None
        };
        let name: String = device.name().to_string();
        device_statuses.push(DeviceStatus {
            name,
            battery_level,
            rssi_level,
        })
    }

    let motors = motor_configuration_from_devices(devices, &application_state.device_manager);

    DeviceList {
        motors,
        devices: device_statuses,
    }
}
