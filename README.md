# sds011-rs

This crate implements a driver for the SDS011 particle sensor based on
[`embedded-hal`](https://github.com/rust-embedded/embedded-hal).
Thanks to this abstraction layer, it can be used on full-fledged operating
systems as well as embedded devices.

## Features
* `sync`: To use the synchronous interface, enable this feature.
  By default, this library exposes an async API.

## Examples
The crate ships with two small CLI examples that utilize the library:
* [`cli.rs`](examples/cli.rs) uses the synchronous interface (embedded-io),
* [`cli_async.rs`](examples/cli_async.rs) uses the asynchronous interface
  (embedded-io-async).

The example below demonstrates how to use the sensor with an ESP32,
showcasing the strength of the embedded-hal abstractions.

```rust
#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer, Delay};
use esp_backtrace as _;
use esp_hal::{
    clock::ClockControl,
    gpio::Io,
    peripherals::Peripherals,
    prelude::*,
    system::SystemControl,
    timer::timg::TimerGroup,
    uart::{config::Config, TxRxPins, Uart},
};
use esp_println::println;
use sds011::SDS011;

#[main]
async fn main(_s: Spawner) -> ! {
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::max(system.clock_control).freeze();

    let timg0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    esp_hal_embassy::init(&clocks, timg0.timer0);

    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    let (tx_pin, rx_pin) = (io.pins.gpio3, io.pins.gpio2);
    let config = Config::default()
        .baudrate(9600)
        .rx_fifo_full_threshold(10);

    let mut uart1 =
        Uart::new_async_with_config(peripherals.UART1, config, &clocks, tx_pin, rx_pin).unwrap();

    let sds011 = SDS011::new(&mut uart1, sds011::Config::default());
    let mut sds011 = sds011.init(&mut Delay).await.unwrap();

    println!("SDS011 version {}, ID {}", sds011.version(), sds011.id());
    loop {
        let dust = sds011.measure(&mut Delay).await.unwrap();
        println!("{}", dust);

        Timer::after(Duration::from_millis(30_000)).await;
    }
}
```

## Technical Overview
The sensor has two operating modes:
* "query mode": The sensor does nothing until it is actively instructed to
  perform a measurement (we call this polling).
* "active mode": The sensor continuously produces data in a configurable
  interval (we call this periodic).

We abstract this into the following interface:
* A sensor created using `new()` is in `Uninitialized` state.
  No serial communication is performed during creation.
* You call `init()`. This will return a sensor in `Polling` state.
  The sensor is instructed via serial commands to switch to query mode and
  goes to sleep (fan off).
* The sensor can now be queried via the `measure()` function.
  This will wake the sensor, spin the fan for a configurable duration
  (which is necessary to get a correct measurement), read the sensor and
  put it back to sleep.
* Optionally (not recommended!), the sensor can be put into `Periodic` state
  by calling `make_periodic()` on a sensor in `Polling` state.
  This puts the sensor in charge of sleeping and waking up.
  Since it will continuously produce data, make sure to call `measure()`
  in time so the serial output buffer does not overflow.

## Limitations
This abstraction does not yet support sending commands only to a specific
sensor id (it effectively uses broadcast mode all the time).
This feature seemed irrelevant, but the backend code for it is completely
implemented, so this may change in a future version if there is demand.
Also, putting sensors into periodic mode can have the side effect of missing
package boundaries. The current version cannot recover from this; it will
return an error. Close the serial port and retry, or probably better,
just don't use periodic mode.

## Acknowledgements
Thank you to Tim Orme, who implemented sds011lib in Python
and wrote [documentation](https://timorme.github.io/sds011lib/resource/)
that pointed me in the right direction, especially to:
* [The Data Sheet](https://cdn-reichelt.de/documents/datenblatt/X200/SDS011-DATASHEET.pdf)
* [The Control Protocol](https://cdn.sparkfun.com/assets/parts/1/2/2/7/5/Laser_Dust_Sensor_Control_Protocol_V1.3.pdf)

for the SDS011 sensor.

License: MIT OR Apache-2.0
