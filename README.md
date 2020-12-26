# Intiface Proxy
This application provides a proxy around the [intiface](https://intiface.com/desktop/) websocket that significantly simplifies the [Buttplug Sex Device Control Standard](https://buttplug-spec.docs.buttplug.io/). This allows data sources to send commands to devices with significantly less programming and makes integration more feasible in environments when complex protocol implementations are difficult or impossible to implement.

## Features
- Extremely simple fire-and-forget protocol
- Automatic reconnection to intiface

### Planned Features
- Configuration file
  - Specify server addresses
  - Replace hardcoded motor tags with user config
- GUI maybe? Or do I just make people deal with CLI?

## Supported Devices
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

## Usage
1. Download the [latest release](https://github.com/runtime-shady-backroom/intiface-proxy/releases/latest).
2. Simply run intiface-proxy.exe. I recommend you launch it from a shell (e.g. command prompt or powershell) so that you can see the logs and kill the process easily. If you run it directly you will have to kill it from task manager, as there is no GUI.

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

If no command is received for 10 seconds, intiface-proxy will send a stop command to all connected devices. To avoid this, send commands periodically even if your desired motor state has not changed.

### Checking the Status
Send an HTTP GET to `http://127.0.0.1:3031/hapticstatus`. A 200 OK will be returned with body containing a plain text summary of the connection status and connected devices. This response is intended for debugging and is not intended to be parsed.


### Forcing a Device Scan
Send an HTTP POST to `http://127.0.0.1:3031/hapticscan` with an empty body. A 200 OK will be returned once the scan is initiated successfully. Forced device scans should not be required, but the route has been left in for testing.
