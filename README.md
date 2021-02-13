# Buttplug Lite
This application serves a websocket that runs a dramatically simplified version of the [Buttplug Sex Device Control Standard](https://buttplug-spec.docs.buttplug.io/) protocol. This allows commands to be sent to devices with significantly less programming, making integration feasible in more restricted environments where the buttplug.io protocol is difficult or impossible to implement.

## Features
- Extremely simple fire-and-forget protocol
- Standalone application requiring no other software

![screenshot of GUI](https://raw.githubusercontent.com/wiki/runtime-shady-backroom/intiface-proxy/images/buttplug-lite-0.4.0.png)


## Supported Devices
All [buttplug.io supported devices](https://iostindex.com/?filtersChanged=1&filter0ButtplugSupport=7) should work. Currently only vibration is supported. Rotatation and linear drives will come soon, and will require additions to the protocol.

## Integrations
### Neos VR
A template is available in this public folder: `neosrec:///U-runtime/R-1d65fb20-ab7b-46c1-89eb-90d176309ec2` (paste that link in-game to spawn it)

## Usage
1. Download the [latest release](https://github.com/runtime-shady-backroom/intiface-proxy/releases/latest).
2. Run buttplug-lite.exe.
3. Press the "refresh devices" button to update the UI with all currently connected devices.
4. Add tags for the devices you plan to use.
5. Press "apply configuration" to save your settings and apply them to the current server.

### Sending Commands
Send text-type messages to `ws://127.0.0.1:3031/haptic`. Binary-type messages are not currently supported. Commands should be sent at most at a 10hz rate. Beyond that application performance may begin to degrade.

#### Message Format
The message format is a list of semicolon (`;`) delimited motor commands. Each motor command is a tag followed by a colon (`:`) followed by a floating-point number representing desired motor strength. Motor strength floats should be in the range [0, 1], and are represented internally using 64 bits.

##### An Example Command

| Tag | Strength
| --- | ---
| foo | 0.00
| bar | 0.30
| baz | 0.99

```
foo:0;bar:0.3;baz:0.99
```

#### Motor State
Motors will continue running at the strength last commanded until another update is received.

If no command is received for 10 seconds, buttplug-lite will send a stop command to all connected devices. To avoid this, send commands periodically even if your desired motor state has not changed.

### Checking the Status
Send an HTTP GET to `http://127.0.0.1:3031/hapticstatus`. A 200 OK will be returned with body containing a plain text summary of the connection status and connected devices. This response is intended for debugging and is not intended to be parsed. If you have a use case that requires parsing device status open an issue.
