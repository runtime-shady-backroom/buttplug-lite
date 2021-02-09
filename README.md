# Buttplug Lite
This application serves a websocket that runs a dramatically simplified version of the [Buttplug Sex Device Control Standard](https://buttplug-spec.docs.buttplug.io/) protocol. This allows commands to be sent to devices with significantly less programming, making integration feasible in more restricted environments where the buttplug.io protocol is difficult or impossible to implement.

## Features
- Extremely simple fire-and-forget protocol

### Planned Features
- **Rename the project to better reflect what it currently does**
- configuration
  - Specify server address
    - do I force you to restart, or support changing bind address live?
  - Replace hardcoded motor tags with user config
  - serialize/deserialize
- GUI
  - show status of devices
  - drive motor config live
- API
  - device status/battery/signal strength?
- duplicate device support?
- integrate a logging framework

## Supported Devices
A future version will allow the user to configure any [buttplug.io supported device](https://iostindex.com/?filtersChanged=1&filter0ButtplugSupport=7). The current version has hardcoded support for the following:

- Lovense Edge
- Lovense Hush
- Lovense Lush
- Lovense Max
- Lovense Nora (need confirmation I don't have the motors flipped)

### Motor Tags
| Tag | Device       | Motor
| --- | ------------ | -----
| `i` | Lovense Edge | Inner
| `o` | Lovense Edge | Outer
| `h` | Lovense Hush | *only has one motor*
| `l` | Lovense Lush | *only has one motor*
| `m` | Lovense Max  | Vibration
|     | Lovense Max  | ~~Suction~~ **currently unsupported**
| `n` | Lovense Nora | Vibration (needs confirmation)
| `r` | Lovense Nora | Rotation (needs confirmation)


## Integrations
### Neos VR
A template is available in this public folder: `neosrec:///U-runtime/R-1d65fb20-ab7b-46c1-89eb-90d176309ec2` (paste that link in-game to spawn it)

## Usage
1. Download the [latest release](https://github.com/runtime-shady-backroom/intiface-proxy/releases/latest).
2. Simply run buttplug-lite.exe. I recommend you launch it from a shell (e.g. command prompt or powershell) so that you can see the logs and kill the process easily. If you run it directly you will have to kill it from task manager, as there is no GUI.

### Sending Commands
Send text messages to `ws://127.0.0.1:3031/haptic`.

#### Message Format
The message format is a list of semicolon (`;`) delimited motor commands. Each motor command is a tag followed by a colon (`:`) followed by a floating-point number representing desired motor strength. Motor strength floats should be in the range [0, 1], and are represented internally using 64 bits.

##### An Arbitrary Example

| Tag | Strength
| --- | ---
| foo | 0.00
| bar | 0.30
| baz | 0.99

```
foo:0;bar:0.3;baz:0.99
```

##### A Practical Example
| Description | Tag | Strength
| ----------- | --- | ---
| Edge Inner  | i   | 0.25
| Edge Outer  | o   | 0.75

```
i:0.25;o:0.75
```

This will drive a Lovense Edge's inner and outer motors at the indicated levels.

#### Motor State
Motors will continue running at the strength last commanded until another update is received.

If no command is received for 10 seconds, buttplug-lite will send a stop command to all connected devices. To avoid this, send commands periodically even if your desired motor state has not changed.

### Checking the Status
Send an HTTP GET to `http://127.0.0.1:3031/hapticstatus`. A 200 OK will be returned with body containing a plain text summary of the connection status and connected devices. This response is intended for debugging and is not intended to be parsed. If you have a use case that requires parsing device status open an issue.
