# Buttplug Lite
This application serves a websocket that runs a dramatically simplified version of the [Buttplug Sex Device Control Standard](https://buttplug-spec.docs.buttplug.io/) protocol. This allows commands to be sent to devices with significantly less programming, making integration feasible in more restricted environments where the buttplug.io protocol is difficult or impossible to implement.

## Features
- Extremely simple fire-and-forget protocol
- Standalone application requiring no other software

![screenshot of GUI](https://raw.githubusercontent.com/wiki/runtime-shady-backroom/buttplug-lite/images/buttplug-lite-2.0.0.png)


## Supported Devices
All [buttplug.io supported devices](https://iostindex.com/?filtersChanged=1&filter0ButtplugSupport=7) should work. This includes everything from Lovense devices to Xbox controllers.

## Integrations
### Neos VR
A Logix reference implementation is available in this public folder:  
`neosrec:///U-runtime/R-1d65fb20-ab7b-46c1-89eb-90d176309ec2` (paste that link in-game to spawn it).

Below is a screenshot of the reference implementation.

![screenshot of reference implementation](https://raw.githubusercontent.com/wiki/runtime-shady-backroom/buttplug-lite/images/reference-implementation-1.0.webp)

This implementation is designed to go on an avatar. The `avatar/user` input should contain the user currently in the avatar. This could be sourced from an AvatarUserReferenceAssigner or a [Get Active User](https://wiki.neos.com/Get_Active_User_(LogiX_node)). The top half of the Logix handles resetting the websocket connection when a new user enters the avatar, and can be omitted if the avatar will only ever be used by one user. The lower half of the logix sends updates to the buttplug-lite server at around 7 Hz. If you go too far beyond 7 Hz you may start to run into latency issues. The two float inputs should be between zero and one (inclusive) and represent the desired motor intensity. You could source this from any number of places, such as Nearest User Hand, VirtualHapticPointSampler, or even a simple UI slider.

## Usage
1. Download the [latest release](https://github.com/runtime-shady-backroom/buttplug-lite/releases/latest).
2. Run buttplug-lite.exe.
3. Add tags for the devices you plan to use.
4. Press "apply configuration" to save your settings and apply them to the current server.

### Sending Commands
Send text-type messages to `ws://127.0.0.1:3031/haptic`. Binary-type messages are not currently supported. Commands should be sent at most at a 10hz rate. Beyond that application performance may begin to degrade.

#### Message Format
The message format is a list of semicolon (`;`) delimited motor commands. There are three possible types of command: Scalar, Linear, and Rotation. All commands start with a motor tag, which is a user-defined string representing a specific motor on a specific device.

##### Scalar
`tag:strength`

Strength controls motor intensity and ranges from `0.0` to `1.0`.

##### Linear
`tag:duration:position`

Position controls target position and ranges from `0.0` to `1.0`.  
Duration controls time in milliseconds the device should take to move to the target position. Duration must be a positive integer.

##### Rotation
`tag:speed`

Speed controls the speed of rotation and ranges from `-1.0` to `1.0`. Positive numbers are clockwise, negative numbers are counterclockwise.

##### Contraction (Deprecated)
`tag:level`

**Only supported in versions  0.5.3 to 1.1.0**. Starting in version 2, contraction is handled via a scalar command.

Contraction controls the pump strength on the Lovense Max. It must be an integer between `0` and `3`, inclusive.

#### An Example Command

| Tag    | Type     | Strength | Duration | Position | Speed | Contraction |
|--------|----------|---------:|---------:|---------:|------:|------------:|
| foo    | Scalar   |       0% |          |          |       |             |
| bar    | Scalar   |      30% |          |          |       |             |
| baz    | Scalar   |     100% |          |          |       |             |
| gort   | Linear   |          |     20ms |      25% |       |             |
| klaatu | Linear   |          |    400ms |      75% |       |             |
| barada | Rotation |          |          |          | -0.75 |             |
| nikto  | Rotation |          |          |          |  0.26 |             |


```
foo:0;bar:0.3;baz:1;gort:20:0.25;klaatu:400:0.75;barada:-0.75;nikto:0.26;max:3
```

Note that you are not required to specify all the tagged motors if you don't want to. The following is also valid, but will of course only drive the `foo` motor.
```
foo:0.1
```

#### Motor State
Motors will continue running at the vibration and rotation speeds last commanded until another update is received.

If no command is received for 10 seconds, buttplug-lite will send a stop command to all connected devices. To avoid this, send commands periodically even if your desired motor state has not changed.

### Checking the Application Version
Send an HTTP GET to `http://127.0.0.1:3031/`. A 200 OK will be returned with body containing the application name and version. Example response:
```
buttplug-lite 0.7.0
```
Prior to version 0.7.0 this endpoint is a 404.

### Checking the Configuration
Send an HTTP GET to `http://127.0.0.1:3031/deviceconfig`. A 200 OK will be returned with body containing a machine-readable list of configured motors. Example response:
```
o;Lovense Edge;scalar
c;Lovense Max;scalar
i;Lovense Edge;scalar
m;Lovense Max;scalar
```

The response is a newline (LF) delimited list of motor configurations. There is a trailing newline. Each motor configuration line is a semicolon (`;`) delimited list of tag, device name, and motor type. In the case where there are no configured motors the response body will be an empty string.

Possible motors types are: `linear`, `rotation`, and `scalar`.

Prior to version 0.7.0 this endpoint is a 404.

### Checking the Status
Send an HTTP GET to `http://127.0.0.1:3031/hapticstatus`. A 200 OK will be returned with body containing a plain text summary of the connection status and connected devices. **This response is intended for debugging and is not intended to be parsed.** The response structure is subject to change. If you have a use case that requires parsing device status let me know by opening an issue.

Example response:
```
device server running=true
  Lovense Edge
    ScalarCmd: ClientGenericDeviceMessageAttributes { feature_descriptor: "No description available for feature", _actuator_type: Vibrate, step_count: 20 }
    ScalarCmd: ClientGenericDeviceMessageAttributes { feature_descriptor: "No description available for feature", _actuator_type: Vibrate, step_count: 20 }
  Lovense Hush
    ScalarCmd: ClientGenericDeviceMessageAttributes { feature_descriptor: "No description available for feature", _actuator_type: Vibrate, step_count: 20 }
  Lovense Max
    ScalarCmd: ClientGenericDeviceMessageAttributes { feature_descriptor: "Vibrator", _actuator_type: Vibrate, step_count: 20 }
    ScalarCmd: ClientGenericDeviceMessageAttributes { feature_descriptor: "Air Pump", _actuator_type: Constrict, step_count: 5 }
  The Handy
    LinearCmd: ClientGenericDeviceMessageAttributes { feature_descriptor: "No description available for feature", _actuator_type: Position, step_count: 100 }
```

### Checking Battery
Send an HTTP GET to `http://127.0.0.1:3031/batterystatus`. A 200 OK will be returned with body containing a plain text list of devices and battery levels. Devices are delimited by newlines, battery levels are delimited by `:`. If the device has an unknown battery level a `-1` will be returned. Example:
```
Lovense Edge:1
Lovense Max:0.45
```

## Feedback
If you have bugs to report or ideas to suggest please let me know by opening an [issue](https://github.com/runtime-shady-backroom/buttplug-lite/issues) or starting a [discussion](https://github.com/runtime-shady-backroom/buttplug-lite/discussions).


## License

Copyright 2022 [runtime-shady-backroom](https://github.com/runtime-shady-backroom) and [buttplug-lite contributors](https://github.com/runtime-shady-backroom/buttplug-lite/graphs/contributors).

Buttplug Lite is provided under the [AGPL-3.0 license](LICENSE).
