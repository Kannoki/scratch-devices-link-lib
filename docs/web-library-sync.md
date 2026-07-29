# Web VM library synchronization

The web VM can persist Arduino libraries through the serial-port JSON-RPC
WebSocket. Send the request only after the socket is open, and await its
response before starting an upload.

```json
{
  "jsonrpc": "2.0",
  "id": 42,
  "method": "syncLibraries",
  "params": {
    "libraries": {
      "Adafruit_TCS34725": {
        "src/Adafruit_TCS34725.h": "...",
        "src/Adafruit_TCS34725.cpp": "...",
        "library.properties": "..."
      },
      "Adafruit_VL53L0X": {
        "src/Adafruit_VL53L0X.h": "...",
        "src/Adafruit_VL53L0X.cpp": "...",
        "library.properties": "..."
      }
    }
  }
}
```

Success returns an acknowledgement that the VM should verify:

```json
{
  "jsonrpc": "2.0",
  "id": 42,
  "result": {
    "librariesUpdated": 2,
    "filesWritten": 6,
    "bytesWritten": 12345,
    "warnings": [
      "TCS34725 and VL53L0X both default to I2C address 0x29..."
    ]
  }
}
```

Invalid paths, non-string contents, filesystem failures, or payload-limit
violations return a JSON-RPC `error` string. The VM must not upload when this
request fails.

Libraries are stored under the app's user-data directory at
`arduino/web-libraries`, outside the downloaded toolchain. Each supplied
library replaces its previous version completely, omitted libraries remain,
and the full merged snapshot is activated atomically. Arduino CLI receives
this search root before the libraries bundled with the toolchain.

## TCS34725 and VL53L0X

The libraries do not conflict at compile time because they use different
headers. The physical sensors do conflict when connected to the same I2C bus:
both power up at address `0x29`, while the TCS34725 address is fixed.

Use one of these hardware arrangements:

- Put the sensors on separate ESP32 I2C controllers (`Wire` and `Wire1`).
- Put one sensor behind an I2C multiplexer such as TCA9548A.
- Isolate or power-gate the TCS34725 while assigning the VL53L0X a different
  address, then enable both.

On boards with only one I2C controller, an I2C multiplexer is the reliable
option.
